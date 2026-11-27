//! Local localhost HTTP + SSE daemon for `hypercode web` (HYP-75).
//!
//! Strictly binds to `127.0.0.1` with constant-time auth token protection
//! (`Authorization: Bearer` or `HttpOnly; SameSite=Strict` cookie) and
//! strict localhost `Origin` checks. Serves the embedded SvelteKit UI
//! (`apps/hypercode-web`) when compiled with `--features embedded-web`.

use crate::agent::{Agent, AgentConfig, AgentEvent, ApprovalHandler, ChatBackend, EventSink};
use crate::cli::format_tool_call_preview;
use crate::context::WorkspaceContext;
use crate::tool::{ToolContext, ToolRegistry};
use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, Request, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use hyperinfer_core::ToolCall;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const AUTH_COOKIE_NAME: &str = "hypercode_web_token";

/// Configuration for [`WebServer`].
#[derive(Debug, Clone)]
pub struct WebServerConfig {
    /// Auth token required on all `/api/*` endpoints (generated randomly if empty).
    pub auth_token: String,
    /// Human-readable sandbox label shown in the web UI.
    pub sandbox_label: String,
}

impl Default for WebServerConfig {
    fn default() -> Self {
        Self {
            auth_token: generate_auth_token(),
            sandbox_label: "Host (secret-env stripped)".to_string(),
        }
    }
}

/// Generate a 256-bit random hex auth token.
pub fn generate_auth_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn is_allowed_localhost_origin(origin: &str) -> bool {
    let lower = origin.trim().to_ascii_lowercase();
    lower == "http://127.0.0.1"
        || lower.starts_with("http://127.0.0.1:")
        || lower == "http://localhost"
        || lower.starts_with("http://localhost:")
        || lower == "http://[::1]"
        || lower.starts_with("http://[::1]:")
}

fn extract_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let (k, v) = pair.trim().split_once('=')?;
        if k.trim() == name {
            return Some(v.trim());
        }
    }
    None
}

// ── Wire Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebInfoResponse {
    pub workspace: String,
    pub git_branch: Option<String>,
    pub model: String,
    pub approval_mode: String,
    pub sandbox: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_events: Vec<WebToolExecution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebToolExecution {
    pub tool_name: String,
    pub arguments: String,
    pub output: String,
    pub is_error: bool,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentStatusEntry {
    pub id: String,
    pub role: String,
    pub status: String,
    pub summary: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApprovalInfo {
    pub approval_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub preview: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub message_count: usize,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub running: bool,
    pub messages: Vec<WebChatMessage>,
    pub command_logs: Vec<WebToolExecution>,
    pub subagents: Vec<SubagentStatusEntry>,
    pub pending_approvals: Vec<PendingApprovalInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFileStatus {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffResponse {
    pub branch: Option<String>,
    pub files: Vec<GitFileStatus>,
    pub diff: String,
}

/// Typed SSE events streamed by `POST /api/sessions/{id}/messages`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebStreamEvent {
    TurnStarted {
        session_id: String,
        prompt: String,
    },
    StepStarted {
        step: usize,
    },
    AssistantDelta {
        text: String,
    },
    ToolApprovalRequired {
        approval_id: String,
        tool_name: String,
        preview: String,
        reason: String,
    },
    ToolApprovalResolved {
        approval_id: String,
        tool_name: String,
        approved: bool,
        reason: String,
    },
    ToolExecuted {
        tool_name: String,
        arguments: String,
        output: String,
        is_error: bool,
    },
    SubagentUpdate {
        id: String,
        role: String,
        status: String,
        summary: String,
    },
    ContextCompacted {
        tokens_before: usize,
        tokens_after: usize,
    },
    TurnCompleted {
        final_response: String,
        steps_taken: usize,
        input_tokens: u32,
        output_tokens: u32,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Deserialize)]
struct AuthRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SendMessageRequest {
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct ResolveApprovalRequest {
    approved: bool,
}

// ── Internal Session & Approval State ────────────────────────────────────────

struct WebSession {
    id: String,
    title: String,
    created_at: DateTime<Utc>,
    running: bool,
    messages: Vec<WebChatMessage>,
    command_logs: Vec<WebToolExecution>,
    subagents: Vec<SubagentStatusEntry>,
    agent: Agent,
    active_cancel: Option<CancellationToken>,
}

struct PendingApproval {
    info: PendingApprovalInfo,
    tx: oneshot::Sender<bool>,
}

struct WebState {
    auth_token: String,
    sandbox_label: String,
    agent_config: AgentConfig,
    backend: Arc<dyn ChatBackend>,
    tools: ToolRegistry,
    tool_ctx: ToolContext,
    workspace_ctx: WorkspaceContext,
    sessions: RwLock<HashMap<String, Arc<Mutex<WebSession>>>>,
    approvals: RwLock<HashMap<String, PendingApproval>>,
}

/// Interactive `hypercode web` local HTTP + SSE server.
pub struct WebServer {
    state: Arc<WebState>,
}

impl WebServer {
    pub fn new(
        config: WebServerConfig,
        agent_config: AgentConfig,
        backend: Arc<dyn ChatBackend>,
        tools: ToolRegistry,
        tool_ctx: ToolContext,
        workspace_ctx: WorkspaceContext,
    ) -> Self {
        let auth_token = if config.auth_token.trim().is_empty() {
            generate_auth_token()
        } else {
            config.auth_token
        };
        Self {
            state: Arc::new(WebState {
                auth_token,
                sandbox_label: config.sandbox_label,
                agent_config,
                backend,
                tools,
                tool_ctx,
                workspace_ctx,
                sessions: RwLock::new(HashMap::new()),
                approvals: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Return the active authentication token for this server instance.
    pub fn auth_token(&self) -> &str {
        &self.state.auth_token
    }

    /// Build the Axum [`Router`] with auth middleware, API routes, and embedded SPA fallback.
    pub fn router(&self) -> Router {
        let protected_api = Router::new()
            .route("/api/info", get(info_handler))
            .route(
                "/api/sessions",
                get(list_sessions_handler).post(create_session_handler),
            )
            .route("/api/sessions/{id}", get(get_session_handler))
            .route("/api/sessions/{id}/messages", post(send_message_handler))
            .route("/api/sessions/{id}/cancel", post(cancel_session_handler))
            .route(
                "/api/approvals/{approval_id}",
                post(resolve_approval_handler),
            )
            .route("/api/git/diff", get(git_diff_handler))
            .layer(middleware::from_fn_with_state(
                self.state.clone(),
                web_auth_middleware,
            ));

        let public_api = Router::new()
            .route("/api/health", get(health_handler))
            .route("/api/auth", post(auth_handler))
            .layer(middleware::from_fn(origin_guard_middleware));

        Router::new()
            .merge(public_api)
            .merge(protected_api)
            .fallback(spa_handler)
            .with_state(self.state.clone())
    }

    /// Serve strictly on `127.0.0.1:<port>`.
    pub async fn serve_localhost(&self, port: u16) -> Result<(), String> {
        let bind_addr = format!("127.0.0.1:{}", port);
        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .map_err(|e| format!("Failed to bind {}: {}", bind_addr, e))?;
        axum::serve(listener, self.router())
            .await
            .map_err(|e| format!("Web server error: {}", e))
    }
}

// ── Middleware ───────────────────────────────────────────────────────────────

async fn origin_guard_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        let allowed = origin
            .to_str()
            .map(is_allowed_localhost_origin)
            .unwrap_or(false);
        if !allowed {
            return (StatusCode::FORBIDDEN, "Cross-origin requests are forbidden").into_response();
        }
    }
    next.run(req).await
}

async fn web_auth_middleware(
    State(state): State<Arc<WebState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        let allowed = origin
            .to_str()
            .map(is_allowed_localhost_origin)
            .unwrap_or(false);
        if !allowed {
            return (StatusCode::FORBIDDEN, "Cross-origin requests are forbidden").into_response();
        }
    }

    let bearer = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    let cookie = extract_cookie(req.headers(), AUTH_COOKIE_NAME);

    let candidate = bearer.or(cookie);
    let valid = candidate
        .map(|tok| constant_time_eq(tok.as_bytes(), state.auth_token.as_bytes()))
        .unwrap_or(false);

    if !valid {
        return (StatusCode::UNAUTHORIZED, "Invalid or missing auth token").into_response();
    }

    next.run(req).await
}

// ── Route Handlers ───────────────────────────────────────────────────────────

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn auth_handler(
    State(state): State<Arc<WebState>>,
    Json(body): Json<AuthRequest>,
) -> Response {
    if !constant_time_eq(body.token.trim().as_bytes(), state.auth_token.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "Invalid auth token").into_response();
    }

    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Strict; Path=/",
        AUTH_COOKIE_NAME, state.auth_token
    );
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({ "authenticated": true })),
    )
        .into_response()
}

async fn info_handler(State(state): State<Arc<WebState>>) -> impl IntoResponse {
    let branch = read_git_branch(&state.workspace_ctx.workspace_root).await;
    Json(WebInfoResponse {
        workspace: state.workspace_ctx.workspace_root.display().to_string(),
        git_branch: branch,
        model: state.agent_config.model.clone(),
        approval_mode: format!("{:?}", state.agent_config.approval_mode),
        sandbox: state.sandbox_label.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn list_sessions_handler(State(state): State<Arc<WebState>>) -> impl IntoResponse {
    let session_arcs: Vec<Arc<Mutex<WebSession>>> =
        state.sessions.read().await.values().cloned().collect();
    let mut summaries = Vec::with_capacity(session_arcs.len());
    for arc in session_arcs {
        let s = arc.lock().await;
        summaries.push(SessionSummary {
            id: s.id.clone(),
            title: s.title.clone(),
            created_at: s.created_at,
            message_count: s.messages.len(),
            running: s.running,
        });
    }
    summaries.sort_by_key(|s| std::cmp::Reverse(s.created_at));
    Json(summaries)
}

async fn create_session_handler(
    State(state): State<Arc<WebState>>,
    body: Option<Json<CreateSessionRequest>>,
) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    let title = body
        .and_then(|Json(b)| b.title)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "New Session".to_string());
    let created_at = Utc::now();

    let agent = Agent::new(
        state.agent_config.clone(),
        state.backend.clone(),
        state.tools.clone(),
        state.tool_ctx.clone(),
    )
    .with_discovered_context(&state.workspace_ctx);

    let session = WebSession {
        id: id.clone(),
        title: title.clone(),
        created_at,
        running: false,
        messages: Vec::new(),
        command_logs: Vec::new(),
        subagents: Vec::new(),
        agent,
        active_cancel: None,
    };

    state
        .sessions
        .write()
        .await
        .insert(id.clone(), Arc::new(Mutex::new(session)));

    (
        StatusCode::CREATED,
        Json(SessionSummary {
            id,
            title,
            created_at,
            message_count: 0,
            running: false,
        }),
    )
}

async fn get_session_handler(
    State(state): State<Arc<WebState>>,
    Path(id): Path<String>,
) -> Response {
    let session_arc = match state.sessions.read().await.get(&id).cloned() {
        Some(arc) => arc,
        None => return (StatusCode::NOT_FOUND, "Session not found").into_response(),
    };
    let pending_approvals: Vec<PendingApprovalInfo> = state
        .approvals
        .read()
        .await
        .values()
        .filter(|p| p.info.session_id == id)
        .map(|p| p.info.clone())
        .collect();

    let s = session_arc.lock().await;
    Json(SessionDetail {
        id: s.id.clone(),
        title: s.title.clone(),
        created_at: s.created_at,
        running: s.running,
        messages: s.messages.clone(),
        command_logs: s.command_logs.clone(),
        subagents: s.subagents.clone(),
        pending_approvals,
    })
    .into_response()
}

async fn cancel_session_handler(
    State(state): State<Arc<WebState>>,
    Path(id): Path<String>,
) -> Response {
    let session_arc = match state.sessions.read().await.get(&id).cloned() {
        Some(arc) => arc,
        None => return (StatusCode::NOT_FOUND, "Session not found").into_response(),
    };

    // Deny any pending approvals for this session so blocked turns unwind immediately.
    let mut approvals = state.approvals.write().await;
    let to_remove: Vec<String> = approvals
        .iter()
        .filter(|(_, p)| p.info.session_id == id)
        .map(|(k, _)| k.clone())
        .collect();
    for key in to_remove {
        if let Some(pending) = approvals.remove(&key) {
            let _ = pending.tx.send(false);
        }
    }
    drop(approvals);

    let s = session_arc.lock().await;
    if let Some(ref cancel) = s.active_cancel {
        cancel.cancel();
    }
    Json(serde_json::json!({ "cancelled": true })).into_response()
}

async fn resolve_approval_handler(
    State(state): State<Arc<WebState>>,
    Path(approval_id): Path<String>,
    Json(body): Json<ResolveApprovalRequest>,
) -> Response {
    let pending = state.approvals.write().await.remove(&approval_id);
    match pending {
        Some(p) => {
            let _ = p.tx.send(body.approved);
            Json(serde_json::json!({
                "resolved": true,
                "approval_id": approval_id,
                "approved": body.approved
            }))
            .into_response()
        }
        None => (StatusCode::NOT_FOUND, "Approval request not found").into_response(),
    }
}

struct WebEventSink {
    tx: mpsc::UnboundedSender<WebStreamEvent>,
    tool_events: Arc<std::sync::Mutex<Vec<WebToolExecution>>>,
}

impl EventSink for WebEventSink {
    fn on_event(&self, event: &AgentEvent) {
        match event {
            AgentEvent::TurnStarted { .. } => {}
            AgentEvent::StepStarted { step } => {
                let _ = self.tx.send(WebStreamEvent::StepStarted { step: *step });
            }
            AgentEvent::AssistantDelta { text } => {
                let _ = self
                    .tx
                    .send(WebStreamEvent::AssistantDelta { text: text.clone() });
            }
            AgentEvent::AssistantResponse { .. } => {}
            AgentEvent::ToolApprovalChecked {
                tool_call,
                reason,
                approved,
            } => {
                let _ = self.tx.send(WebStreamEvent::ToolApprovalResolved {
                    approval_id: tool_call.id.clone(),
                    tool_name: tool_call.name.clone(),
                    approved: *approved,
                    reason: reason.clone(),
                });
            }
            AgentEvent::ToolExecuted { tool_call, output } => {
                let exec = WebToolExecution {
                    tool_name: tool_call.name.clone(),
                    arguments: tool_call.arguments.clone(),
                    output: output.content.clone(),
                    is_error: output.is_error,
                    timestamp: Utc::now(),
                };
                if let Ok(mut guard) = self.tool_events.lock() {
                    guard.push(exec.clone());
                }
                let _ = self.tx.send(WebStreamEvent::ToolExecuted {
                    tool_name: exec.tool_name,
                    arguments: exec.arguments,
                    output: exec.output,
                    is_error: exec.is_error,
                });
            }
            AgentEvent::ContextCompacted {
                tokens_before,
                tokens_after,
            } => {
                let _ = self.tx.send(WebStreamEvent::ContextCompacted {
                    tokens_before: *tokens_before,
                    tokens_after: *tokens_after,
                });
            }
            AgentEvent::TurnCompleted { .. } => {}
        }
    }
}

struct WebApprovalHandler {
    state: Arc<WebState>,
    session_id: String,
    tx: mpsc::UnboundedSender<WebStreamEvent>,
}

#[async_trait]
impl ApprovalHandler for WebApprovalHandler {
    async fn approve(&self, call: &ToolCall, reason: &str) -> bool {
        let approval_id = Uuid::new_v4().to_string();
        let preview = format_tool_call_preview(call);
        let (resp_tx, resp_rx) = oneshot::channel();

        let info = PendingApprovalInfo {
            approval_id: approval_id.clone(),
            session_id: self.session_id.clone(),
            tool_name: call.name.clone(),
            preview: preview.clone(),
            reason: reason.to_string(),
        };

        self.state
            .approvals
            .write()
            .await
            .insert(approval_id.clone(), PendingApproval { info, tx: resp_tx });

        let _ = self.tx.send(WebStreamEvent::ToolApprovalRequired {
            approval_id: approval_id.clone(),
            tool_name: call.name.clone(),
            preview,
            reason: reason.to_string(),
        });

        let approved = resp_rx.await.unwrap_or(false);
        self.state.approvals.write().await.remove(&approval_id);
        approved
    }
}

async fn send_message_handler(
    State(state): State<Arc<WebState>>,
    Path(id): Path<String>,
    Json(body): Json<SendMessageRequest>,
) -> Response {
    let prompt = body.prompt.trim().to_string();
    if prompt.is_empty() {
        return (StatusCode::BAD_REQUEST, "Prompt must not be empty").into_response();
    }

    let session_arc = match state.sessions.read().await.get(&id).cloned() {
        Some(arc) => arc,
        None => return (StatusCode::NOT_FOUND, "Session not found").into_response(),
    };

    let (tx, rx) = mpsc::unbounded_channel::<WebStreamEvent>();
    let state_clone = state.clone();
    let session_id = id.clone();

    tokio::spawn(async move {
        let mut session = session_arc.lock().await;
        if session.title == "New Session" {
            let short: String = prompt.chars().take(60).collect();
            session.title = short;
        }
        session.running = true;
        session.messages.push(WebChatMessage {
            id: Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: prompt.clone(),
            timestamp: Utc::now(),
            tool_events: Vec::new(),
        });

        let cancel = CancellationToken::new();
        session.active_cancel = Some(cancel.clone());

        let tool_events = Arc::new(std::sync::Mutex::new(Vec::new()));
        session.agent.set_event_sink(WebEventSink {
            tx: tx.clone(),
            tool_events: tool_events.clone(),
        });
        session.agent.set_approval_handler(WebApprovalHandler {
            state: state_clone,
            session_id: session_id.clone(),
            tx: tx.clone(),
        });

        let _ = tx.send(WebStreamEvent::TurnStarted {
            session_id: session_id.clone(),
            prompt: prompt.clone(),
        });

        let outcome = session.agent.run_turn_with_cancel(prompt, cancel).await;
        session.running = false;
        session.active_cancel = None;

        let captured_tools = tool_events.lock().map(|g| g.clone()).unwrap_or_default();
        for t in &captured_tools {
            session.command_logs.push(t.clone());
        }

        match outcome {
            Ok(turn) => {
                session.messages.push(WebChatMessage {
                    id: Uuid::new_v4().to_string(),
                    role: "assistant".to_string(),
                    content: turn.final_response.clone(),
                    timestamp: Utc::now(),
                    tool_events: captured_tools,
                });
                let _ = tx.send(WebStreamEvent::TurnCompleted {
                    final_response: turn.final_response,
                    steps_taken: turn.steps_taken,
                    input_tokens: turn.total_usage.input_tokens,
                    output_tokens: turn.total_usage.output_tokens,
                });
            }
            Err(err) => {
                let _ = tx.send(WebStreamEvent::Error {
                    message: err.to_string(),
                });
            }
        }
    });

    let stream: std::pin::Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            while let Some(ev) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&ev) {
                    return Some((Ok(Event::default().data(json)), rx));
                }
            }
            None
        }));

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn git_command(root: &std::path::Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("git");
    cmd.current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    cmd
}

async fn git_diff_handler(State(state): State<Arc<WebState>>) -> impl IntoResponse {
    let root = state.workspace_ctx.workspace_root.clone();
    let branch = read_git_branch(&root).await;

    let status_out = tokio::time::timeout(
        Duration::from_secs(5),
        git_command(&root).args(["status", "--porcelain"]).output(),
    )
    .await;

    let mut files = Vec::new();
    if let Ok(Ok(out)) = status_out {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.len() >= 4 {
                    let status = line[..2].trim().to_string();
                    let path = line[3..].trim().to_string();
                    files.push(GitFileStatus { path, status });
                }
            }
        }
    }

    let diff_out = tokio::time::timeout(
        Duration::from_secs(5),
        git_command(&root).args(["diff", "HEAD"]).output(),
    )
    .await;

    let diff = match diff_out {
        Ok(Ok(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => match tokio::time::timeout(
            Duration::from_secs(5),
            git_command(&root).args(["diff"]).output(),
        )
        .await
        {
            Ok(Ok(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            _ => String::new(),
        },
    };

    Json(GitDiffResponse {
        branch,
        files,
        diff,
    })
}

async fn read_git_branch(root: &std::path::Path) -> Option<String> {
    let out = tokio::time::timeout(
        Duration::from_secs(3),
        git_command(root)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

// ── Embedded SvelteKit SPA Handler ───────────────────────────────────────────

#[cfg(feature = "embedded-web")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../apps/hypercode-web/build/"]
struct EmbeddedWebUi;

#[cfg(feature = "embedded-web")]
async fn spa_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }

    if !path.is_empty() {
        if let Some(content) = EmbeddedWebUi::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            return ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response();
        }
    }

    if let Some(index) = EmbeddedWebUi::get("index.html") {
        return ([(header::CONTENT_TYPE, "text/html")], index.data).into_response();
    }

    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

#[cfg(not(feature = "embedded-web"))]
async fn spa_handler(uri: Uri) -> impl IntoResponse {
    let _ = uri;
    (
        StatusCode::NOT_FOUND,
        "HyperCode Web UI not built. Run: cd apps/hypercode-web && bun run build, then rebuild with --features embedded-web",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::ApprovalMode;
    use axum_test::TestServer;
    use hyperinfer_core::{
        ChatMessage, ChatRequest, ChatResponse, Choice, HyperInferError, MessageRole, Usage,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockWebBackend {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ChatBackend for MockWebBackend {
        async fn chat(
            &self,
            _key: &str,
            _req: ChatRequest,
        ) -> Result<ChatResponse, HyperInferError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Ok(ChatResponse {
                    id: "resp-1".to_string(),
                    model: "mock-model".to_string(),
                    choices: vec![Choice {
                        index: 0,
                        message: ChatMessage {
                            role: MessageRole::Assistant,
                            content: "Writing hello.txt...".to_string(),
                            name: None,
                            content_parts: None,
                            tool_calls: Some(vec![ToolCall {
                                id: "tc-1".to_string(),
                                name: "write_file".to_string(),
                                arguments: r#"{"path":"hello.txt","content":"hello from web\n"}"#
                                    .to_string(),
                            }]),
                            tool_call_id: None,
                        },
                        finish_reason: Some("tool_calls".to_string()),
                    }],
                    usage: Usage {
                        input_tokens: 100,
                        output_tokens: 40,
                        cache_read_tokens: None,
                        cache_creation_tokens: None,
                    },
                })
            } else {
                Ok(ChatResponse {
                    id: "resp-2".to_string(),
                    model: "mock-model".to_string(),
                    choices: vec![Choice {
                        index: 0,
                        message: ChatMessage {
                            role: MessageRole::Assistant,
                            content: "Created hello.txt successfully.".to_string(),
                            name: None,
                            content_parts: None,
                            tool_calls: None,
                            tool_call_id: None,
                        },
                        finish_reason: Some("stop".to_string()),
                    }],
                    usage: Usage {
                        input_tokens: 120,
                        output_tokens: 20,
                        cache_read_tokens: None,
                        cache_creation_tokens: None,
                    },
                })
            }
        }
    }

    fn test_web_server(root: &std::path::Path) -> (WebServer, TestServer, String) {
        let token = "test-secret-token-12345".to_string();
        let ws = WorkspaceContext::discover(root);
        let server = WebServer::new(
            WebServerConfig {
                auth_token: token.clone(),
                sandbox_label: "Test".to_string(),
            },
            AgentConfig {
                model: "mock-model".to_string(),
                approval_mode: ApprovalMode::ApproveAll,
                ..Default::default()
            },
            Arc::new(MockWebBackend {
                calls: AtomicUsize::new(0),
            }),
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(root),
            ws,
        );
        let test_server = TestServer::new(server.router()).unwrap();
        (server, test_server, token)
    }

    #[tokio::test]
    async fn test_auth_origin_guard_and_cookie_exchange() {
        let dir = tempfile::tempdir().unwrap();
        let (_web, server, token) = test_web_server(dir.path());

        // Unauthenticated API request rejected
        server
            .get("/api/info")
            .await
            .assert_status(StatusCode::UNAUTHORIZED);

        // Cross-origin request rejected even with valid token
        server
            .get("/api/info")
            .add_header("Authorization", format!("Bearer {token}"))
            .add_header("Origin", "https://evil.example")
            .await
            .assert_status(StatusCode::FORBIDDEN);

        // Exchange token for HttpOnly cookie
        let auth = server
            .post("/api/auth")
            .json(&serde_json::json!({ "token": token }))
            .await;
        auth.assert_status_ok();
        let set_cookie = auth.header(header::SET_COOKIE);
        assert!(set_cookie.to_str().unwrap().contains("HttpOnly"));
        assert!(set_cookie.to_str().unwrap().contains("SameSite=Strict"));

        // Authenticated request succeeds with Bearer token
        let info = server
            .get("/api/info")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        info.assert_status_ok();
        let body: WebInfoResponse = info.json();
        assert_eq!(body.model, "mock-model");
    }

    #[tokio::test]
    async fn test_session_create_and_git_diff() {
        let dir = tempfile::tempdir().unwrap();
        let run_git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .output()
                .unwrap()
        };
        run_git(&["init"]);
        std::fs::write(dir.path().join("README.md"), "v1\n").unwrap();
        run_git(&["add", "README.md"]);
        run_git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "init",
        ]);
        std::fs::write(dir.path().join("README.md"), "v2\n").unwrap();

        let (_web, server, token) = test_web_server(dir.path());

        let created = server
            .post("/api/sessions")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({ "title": "My Session" }))
            .await;
        created.assert_status(StatusCode::CREATED);
        let summary: SessionSummary = created.json();
        assert_eq!(summary.title, "My Session");

        let diff_resp = server
            .get("/api/git/diff")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        diff_resp.assert_status_ok();
        let diff: GitDiffResponse = diff_resp.json();
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.files[0].path, "README.md");
        assert!(diff.diff.contains("+v2"));
    }
}
