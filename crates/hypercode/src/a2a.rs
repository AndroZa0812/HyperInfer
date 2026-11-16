//! Agent2Agent (A2A) JSON-RPC 2.0 protocol server for HyperCode.
//!
//! Exposes HyperCode over both HTTP/SSE (`/.well-known/agent.json` + `POST /`) and
//! newline-delimited JSON-RPC over stdio (`--a2a-stdio`), supporting:
//! - Agent discovery via `AgentCard`
//! - Persistent multi-turn sessions (`sessionId`)
//! - Real-time task streaming (`tasks/sendSubscribe` / `message/stream`)
//! - Bidirectional `input-required` approval gates for mutating/unsandboxed tool calls

use crate::agent::{Agent, AgentConfig, AgentEvent, ApprovalHandler, ChatBackend, EventSink};
use crate::cli::format_tool_call_preview;
use crate::context::WorkspaceContext;
use crate::tool::{ToolContext, ToolRegistry};
use async_trait::async_trait;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use futures::stream::Stream;
use hyperinfer_core::ToolCall;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, oneshot, Mutex, Notify, RwLock};
use uuid::Uuid;

// ── A2A Protocol Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub streaming: bool,
    pub push_notifications: bool,
    pub state_transition_history: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// A2A Agent Card served at `GET /.well-known/agent.json` and `agent/card`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    pub version: String,
    pub capabilities: AgentCapabilities,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
    pub skills: Vec<AgentSkill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Part {
    Text { text: String },
    Data { data: serde_json::Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: String,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    pub fn agent_text(text: impl Into<String>) -> Self {
        Self {
            role: "agent".to_string(),
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } => Some(text.as_str()),
                Part::Data { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<Part>,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub status: TaskStatus,
    #[serde(default)]
    pub history: Vec<Message>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusUpdateEvent {
    pub id: String,
    pub status: TaskStatus,
    #[serde(rename = "final")]
    pub is_final: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactUpdateEvent {
    pub id: String,
    pub artifact: Artifact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum A2aStreamEvent {
    Status(TaskStatusUpdateEvent),
    Artifact(TaskArtifactUpdateEvent),
}

// ── JSON-RPC 2.0 Envelope ────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSendParams {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub message: Message,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskIdParams {
    pub id: String,
}

// ── Task Execution & Approval Plumbing ───────────────────────────────────────

struct TaskRecord {
    task: Task,
    events_tx: broadcast::Sender<A2aStreamEvent>,
    state_notify: Arc<Notify>,
    pending_approval: Option<oneshot::Sender<bool>>,
    abort_handle: Option<tokio::task::AbortHandle>,
}

impl TaskRecord {
    fn new(id: String, session_id: Option<String>, initial_message: Message) -> Self {
        let (events_tx, _) = broadcast::channel(128);
        Self {
            task: Task {
                id,
                session_id,
                status: TaskStatus {
                    state: TaskState::Submitted,
                    message: None,
                    timestamp: Utc::now().to_rfc3339(),
                },
                history: vec![initial_message],
                artifacts: Vec::new(),
            },
            events_tx,
            state_notify: Arc::new(Notify::new()),
            pending_approval: None,
            abort_handle: None,
        }
    }

    fn set_status(&mut self, state: TaskState, message: Option<Message>, is_final: bool) {
        if let Some(ref msg) = message {
            self.task.history.push(msg.clone());
        }
        let status = TaskStatus {
            state,
            message,
            timestamp: Utc::now().to_rfc3339(),
        };
        self.task.status = status.clone();
        let _ = self
            .events_tx
            .send(A2aStreamEvent::Status(TaskStatusUpdateEvent {
                id: self.task.id.clone(),
                status,
                is_final,
            }));
        self.state_notify.notify_waiters();
    }

    fn push_artifact(&mut self, name: impl Into<String>, parts: Vec<Part>) {
        let index = self.task.artifacts.len();
        let artifact = Artifact {
            name: Some(name.into()),
            description: None,
            parts,
            index,
        };
        self.task.artifacts.push(artifact.clone());
        let _ = self
            .events_tx
            .send(A2aStreamEvent::Artifact(TaskArtifactUpdateEvent {
                id: self.task.id.clone(),
                artifact,
            }));
    }
}

/// `EventSink` bridging `AgentEvent` updates into an A2A task's status stream.
struct A2aEventSink {
    task_id: String,
    tasks: Arc<RwLock<HashMap<String, Arc<Mutex<TaskRecord>>>>>,
}

impl EventSink for A2aEventSink {
    fn on_event(&self, event: &AgentEvent) {
        let summary = match event {
            AgentEvent::StepStarted { step } => format!("Step {} started", step),
            AgentEvent::AssistantResponse {
                content,
                tool_calls,
            } => {
                if tool_calls.is_empty() {
                    return;
                }
                let names: Vec<&str> = tool_calls.iter().map(|c| c.name.as_str()).collect();
                if content.trim().is_empty() {
                    format!("Calling tools: {}", names.join(", "))
                } else {
                    format!("{} (tools: {})", content.trim(), names.join(", "))
                }
            }
            AgentEvent::ToolExecuted { tool_call, output } => {
                let status = if output.is_error { "failed" } else { "ok" };
                format!("Tool `{}` {}", tool_call.name, status)
            }
            AgentEvent::ContextCompacted {
                tokens_before,
                tokens_after,
            } => {
                format!(
                    "Context compacted (~{} -> ~{} tokens)",
                    tokens_before, tokens_after
                )
            }
            AgentEvent::TurnStarted { .. }
            | AgentEvent::ToolApprovalChecked { .. }
            | AgentEvent::TurnCompleted { .. } => return,
        };

        let tasks = self.tasks.clone();
        let task_id = self.task_id.clone();
        tokio::spawn(async move {
            let record_opt = tasks.read().await.get(&task_id).cloned();
            if let Some(record_arc) = record_opt {
                let mut rec = record_arc.lock().await;
                if rec.task.status.state == TaskState::Working {
                    rec.set_status(
                        TaskState::Working,
                        Some(Message::agent_text(summary)),
                        false,
                    );
                }
            }
        });
    }
}

/// `ApprovalHandler` that pauses the A2A task in `TaskState::InputRequired` until
/// the parent agent replies via `tasks/send`.
struct A2aApprovalHandler {
    task_id: String,
    tasks: Arc<RwLock<HashMap<String, Arc<Mutex<TaskRecord>>>>>,
}

#[async_trait]
impl ApprovalHandler for A2aApprovalHandler {
    async fn approve(&self, call: &ToolCall, reason: &str) -> bool {
        let record_arc = match self.tasks.read().await.get(&self.task_id).cloned() {
            Some(r) => r,
            None => return false,
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut rec = record_arc.lock().await;
            rec.pending_approval = Some(tx);
            let preview = format_tool_call_preview(call);
            let prompt_msg = Message {
                role: "agent".to_string(),
                parts: vec![
                    Part::Text {
                        text: format!(
                            "Approval required ({}). Reply with 'approve' or 'deny':\n{}",
                            reason, preview
                        ),
                    },
                    Part::Data {
                        data: serde_json::json!({
                            "type": "approval_request",
                            "reason": reason,
                            "tool_call": call,
                            "preview": preview,
                        }),
                    },
                ],
            };
            rec.set_status(TaskState::InputRequired, Some(prompt_msg), false);
        }

        rx.await.unwrap_or(false)
    }
}

fn parse_approval_reply(msg: &Message) -> bool {
    for part in &msg.parts {
        if let Part::Data { data } = part {
            if let Some(approved) = data.get("approved").and_then(|v| v.as_bool()) {
                return approved;
            }
        }
    }
    let lower = msg.text_content().trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "y" | "yes" | "approve" | "approved" | "allow" | "true" | "ok"
    )
}

// ── A2A Server Core ──────────────────────────────────────────────────────────

/// Shared A2A runtime managing sessions, tasks, and agent execution.
#[derive(Clone)]
pub struct A2aServer {
    pub card: AgentCard,
    agent_config: AgentConfig,
    backend: Arc<dyn ChatBackend>,
    registry: ToolRegistry,
    tool_ctx: ToolContext,
    ws_context: WorkspaceContext,
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<Agent>>>>>,
    tasks: Arc<RwLock<HashMap<String, Arc<Mutex<TaskRecord>>>>>,
}

impl A2aServer {
    pub fn new(
        url: impl Into<String>,
        agent_config: AgentConfig,
        backend: Arc<dyn ChatBackend>,
        registry: ToolRegistry,
        tool_ctx: ToolContext,
        ws_context: WorkspaceContext,
    ) -> Self {
        let card = AgentCard {
            name: "HyperCode".to_string(),
            description: "Provider-agnostic coding agent harness built on HyperInfer".to_string(),
            url: url.into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: AgentCapabilities {
                streaming: true,
                push_notifications: false,
                state_transition_history: true,
            },
            default_input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
            default_output_modes: vec!["text/plain".to_string(), "application/json".to_string()],
            skills: vec![
                AgentSkill {
                    id: "code_editing".to_string(),
                    name: "Code Editing & Refactoring".to_string(),
                    description:
                        "Read, write, and perform exact surgical edits across workspace files."
                            .to_string(),
                    tags: vec!["coding".to_string(), "refactoring".to_string()],
                },
                AgentSkill {
                    id: "repo_search".to_string(),
                    name: "Repository Inspection".to_string(),
                    description: "List workspace structure and search code with regex grep."
                        .to_string(),
                    tags: vec!["search".to_string(), "analysis".to_string()],
                },
                AgentSkill {
                    id: "command_execution".to_string(),
                    name: "Sandboxed Command Execution".to_string(),
                    description:
                        "Run tests, builds, and linters via HostRunner or NVIDIA OpenShell."
                            .to_string(),
                    tags: vec![
                        "bash".to_string(),
                        "testing".to_string(),
                        "sandbox".to_string(),
                    ],
                },
            ],
            metadata: Some(serde_json::json!({
                "model": agent_config.model,
                "workspace": ws_context.workspace_root,
                "sandbox": tool_ctx.command_runner.name(),
                "approvalMode": format!("{:?}", agent_config.approval_mode),
            })),
        };

        Self {
            card,
            agent_config,
            backend,
            registry,
            tool_ctx,
            ws_context,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn get_or_create_session(&self, session_id: &str) -> Arc<Mutex<Agent>> {
        if let Some(existing) = self.sessions.read().await.get(session_id).cloned() {
            return existing;
        }
        let mut sessions = self.sessions.write().await;
        if let Some(existing) = sessions.get(session_id).cloned() {
            return existing;
        }
        let agent = Agent::new(
            self.agent_config.clone(),
            self.backend.clone(),
            self.registry.clone(),
            self.tool_ctx.clone(),
        )
        .with_discovered_context(&self.ws_context);
        let arc = Arc::new(Mutex::new(agent));
        sessions.insert(session_id.to_string(), arc.clone());
        arc
    }

    /// Submit a new task or resume an `InputRequired` task, returning `(record, rx)`.
    async fn start_or_resume_task(
        &self,
        params: TaskSendParams,
    ) -> Result<Arc<Mutex<TaskRecord>>, (i32, String)> {
        let task_id = params
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // Check if this `task_id` already exists and is waiting for approval
        if let Some(existing_arc) = self.tasks.read().await.get(&task_id).cloned() {
            let mut rec = existing_arc.lock().await;
            if rec.task.status.state == TaskState::InputRequired {
                let approved = parse_approval_reply(&params.message);
                rec.task.history.push(params.message);
                if let Some(tx) = rec.pending_approval.take() {
                    let _ = tx.send(approved);
                }
                rec.set_status(
                    TaskState::Working,
                    Some(Message::agent_text(if approved {
                        "Approval granted; resuming execution."
                    } else {
                        "Approval denied; notifying agent."
                    })),
                    false,
                );
                return Ok(existing_arc.clone());
            }
            return Err((
                -32602,
                format!(
                    "Task '{}' already exists in state {:?}",
                    task_id, rec.task.status.state
                ),
            ));
        }

        let prompt = params.message.text_content();
        if prompt.trim().is_empty() {
            return Err((-32602, "Message text cannot be empty".to_string()));
        }

        let session_id = params
            .session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let record_arc = Arc::new(Mutex::new(TaskRecord::new(
            task_id.clone(),
            Some(session_id.clone()),
            params.message,
        )));

        self.tasks
            .write()
            .await
            .insert(task_id.clone(), record_arc.clone());

        let server = self.clone();
        let record_for_task = record_arc.clone();
        let task_id_owned = task_id.clone();

        let join_handle = tokio::spawn(async move {
            let agent_arc = server.get_or_create_session(&session_id).await;
            {
                let mut rec = record_for_task.lock().await;
                rec.set_status(TaskState::Working, None, false);
            }

            let mut agent = agent_arc.lock().await;
            agent.set_event_sink(A2aEventSink {
                task_id: task_id_owned.clone(),
                tasks: server.tasks.clone(),
            });
            agent.set_approval_handler(A2aApprovalHandler {
                task_id: task_id_owned.clone(),
                tasks: server.tasks.clone(),
            });

            let outcome = agent.run_turn(prompt).await;
            drop(agent);

            let mut rec = record_for_task.lock().await;
            rec.pending_approval = None;
            match outcome {
                Ok(res) => {
                    rec.push_artifact(
                        "result",
                        vec![
                            Part::Text {
                                text: res.final_response.clone(),
                            },
                            Part::Data {
                                data: serde_json::json!({
                                    "steps_taken": res.steps_taken,
                                    "tool_calls_executed": res.tool_calls_executed,
                                    "usage": res.total_usage,
                                }),
                            },
                        ],
                    );
                    rec.set_status(
                        TaskState::Completed,
                        Some(Message::agent_text(res.final_response)),
                        true,
                    );
                }
                Err(err) => {
                    rec.set_status(
                        TaskState::Failed,
                        Some(Message::agent_text(err.to_string())),
                        true,
                    );
                }
            }
        });

        {
            let mut rec = record_arc.lock().await;
            rec.abort_handle = Some(join_handle.abort_handle());
        }

        Ok(record_arc)
    }

    /// Dispatch a single non-streaming JSON-RPC 2.0 request.
    pub async fn handle_rpc(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let id = req.id.clone().unwrap_or(serde_json::Value::Null);
        if req.jsonrpc != "2.0" {
            return JsonRpcResponse::err(id, -32600, "Invalid jsonrpc version; expected '2.0'");
        }

        match req.method.as_str() {
            "agent/card" => JsonRpcResponse::ok(id, serde_json::to_value(&self.card).unwrap()),
            "tasks/send" | "message/send" => {
                let params: TaskSendParams = match serde_json::from_value(req.params) {
                    Ok(p) => p,
                    Err(e) => {
                        return JsonRpcResponse::err(id, -32602, format!("Invalid params: {}", e));
                    }
                };
                let record_arc = match self.start_or_resume_task(params).await {
                    Ok(r) => r,
                    Err((code, msg)) => return JsonRpcResponse::err(id, code, msg),
                };

                // Wait until the task reaches a terminal state OR pauses in `InputRequired`.
                loop {
                    let notify = {
                        let rec = record_arc.lock().await;
                        let st = rec.task.status.state;
                        if st.is_terminal() || st == TaskState::InputRequired {
                            return JsonRpcResponse::ok(
                                id,
                                serde_json::to_value(&rec.task).unwrap(),
                            );
                        }
                        rec.state_notify.clone()
                    };
                    notify.notified().await;
                }
            }
            "tasks/get" => {
                let params: TaskIdParams = match serde_json::from_value(req.params) {
                    Ok(p) => p,
                    Err(e) => {
                        return JsonRpcResponse::err(id, -32602, format!("Invalid params: {}", e));
                    }
                };
                let rec_opt = self.tasks.read().await.get(&params.id).cloned();
                match rec_opt {
                    Some(rec_arc) => {
                        let rec = rec_arc.lock().await;
                        JsonRpcResponse::ok(id, serde_json::to_value(&rec.task).unwrap())
                    }
                    None => {
                        JsonRpcResponse::err(id, -32001, format!("Task '{}' not found", params.id))
                    }
                }
            }
            "tasks/cancel" => {
                let params: TaskIdParams = match serde_json::from_value(req.params) {
                    Ok(p) => p,
                    Err(e) => {
                        return JsonRpcResponse::err(id, -32602, format!("Invalid params: {}", e));
                    }
                };
                let rec_opt = self.tasks.read().await.get(&params.id).cloned();
                match rec_opt {
                    Some(rec_arc) => {
                        let mut rec = rec_arc.lock().await;
                        if !rec.task.status.state.is_terminal() {
                            if let Some(handle) = rec.abort_handle.take() {
                                handle.abort();
                            }
                            rec.pending_approval = None;
                            rec.set_status(
                                TaskState::Canceled,
                                Some(Message::agent_text("Task canceled by client")),
                                true,
                            );
                        }
                        JsonRpcResponse::ok(id, serde_json::to_value(&rec.task).unwrap())
                    }
                    None => {
                        JsonRpcResponse::err(id, -32001, format!("Task '{}' not found", params.id))
                    }
                }
            }
            other => JsonRpcResponse::err(id, -32601, format!("Method not found: '{}'", other)),
        }
    }

    /// Build an `axum::Router` serving `GET /.well-known/agent.json` and `POST /`.
    pub fn router(self) -> Router {
        Router::new()
            .route("/.well-known/agent.json", get(http_agent_card))
            .route("/", post(http_rpc_handler))
            .with_state(self)
    }

    /// Run the HTTP/SSE A2A server bound to `bind_addr` (e.g. `127.0.0.1:41241`).
    pub async fn serve_http(self, bind_addr: &str) -> Result<(), String> {
        let listener = tokio::net::TcpListener::bind(bind_addr)
            .await
            .map_err(|e| format!("Failed to bind A2A HTTP server on '{}': {}", bind_addr, e))?;
        axum::serve(listener, self.router())
            .await
            .map_err(|e| format!("A2A HTTP server error: {}", e))
    }

    /// Run a newline-delimited JSON-RPC 2.0 A2A server over `stdin` / `stdout`.
    pub async fn serve_stdio(&self) -> Result<(), String> {
        let stdin = BufReader::new(tokio::io::stdin());
        let mut lines = stdin.lines();
        let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| format!("Failed to read A2A stdio line: {}", e))?
        {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = JsonRpcResponse::err(
                        serde_json::Value::Null,
                        -32700,
                        format!("Parse error: {}", e),
                    );
                    write_json_line(&stdout, &err_resp).await?;
                    continue;
                }
            };

            if req.method == "tasks/sendSubscribe" || req.method == "message/stream" {
                let id = req.id.clone().unwrap_or(serde_json::Value::Null);
                let params: TaskSendParams = match serde_json::from_value(req.params) {
                    Ok(p) => p,
                    Err(e) => {
                        let resp =
                            JsonRpcResponse::err(id, -32602, format!("Invalid params: {}", e));
                        write_json_line(&stdout, &resp).await?;
                        continue;
                    }
                };
                match self.start_or_resume_task(params).await {
                    Ok(rec_arc) => {
                        let mut rx = rec_arc.lock().await.events_tx.subscribe();
                        let out = stdout.clone();
                        tokio::spawn(async move {
                            while let Ok(ev) = rx.recv().await {
                                let is_done = matches!(
                                    &ev,
                                    A2aStreamEvent::Status(s) if s.is_final
                                );
                                let resp = JsonRpcResponse::ok(
                                    id.clone(),
                                    serde_json::to_value(&ev).unwrap(),
                                );
                                let _ = write_json_line(&out, &resp).await;
                                if is_done {
                                    break;
                                }
                            }
                        });
                    }
                    Err((code, msg)) => {
                        let resp = JsonRpcResponse::err(id, code, msg);
                        write_json_line(&stdout, &resp).await?;
                    }
                }
            } else {
                let resp = self.handle_rpc(req).await;
                write_json_line(&stdout, &resp).await?;
            }
        }

        Ok(())
    }
}

async fn write_json_line<T: Serialize>(
    stdout: &Arc<Mutex<tokio::io::Stdout>>,
    value: &T,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|e| format!("Failed to serialize A2A response: {}", e))?;
    bytes.push(b'\n');
    let mut guard = stdout.lock().await;
    guard
        .write_all(&bytes)
        .await
        .map_err(|e| format!("Failed to write A2A stdout: {}", e))?;
    guard
        .flush()
        .await
        .map_err(|e| format!("Failed to flush A2A stdout: {}", e))
}

async fn http_agent_card(State(server): State<A2aServer>) -> Json<AgentCard> {
    Json(server.card)
}

async fn http_rpc_handler(
    State(server): State<A2aServer>,
    Json(req): Json<JsonRpcRequest>,
) -> Response {
    if req.method == "tasks/sendSubscribe" || req.method == "message/stream" {
        let id = req.id.clone().unwrap_or(serde_json::Value::Null);
        let params: TaskSendParams = match serde_json::from_value(req.params) {
            Ok(p) => p,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(JsonRpcResponse::err(
                        id,
                        -32602,
                        format!("Invalid params: {}", e),
                    )),
                )
                    .into_response();
            }
        };
        let record_arc = match server.start_or_resume_task(params).await {
            Ok(r) => r,
            Err((code, msg)) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(JsonRpcResponse::err(id, code, msg)),
                )
                    .into_response();
            }
        };

        let mut rx = record_arc.lock().await.events_tx.subscribe();
        let stream: std::pin::Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
            Box::pin(futures::stream::poll_fn(move |cx| {
                let mut fut = Box::pin(rx.recv());
                match std::future::Future::poll(fut.as_mut(), cx) {
                    std::task::Poll::Ready(Ok(ev)) => {
                        let resp =
                            JsonRpcResponse::ok(id.clone(), serde_json::to_value(&ev).unwrap());
                        let data = serde_json::to_string(&resp).unwrap_or_default();
                        std::task::Poll::Ready(Some(Ok(Event::default().data(data))))
                    }
                    std::task::Poll::Ready(Err(broadcast::error::RecvError::Closed)) => {
                        std::task::Poll::Ready(None)
                    }
                    std::task::Poll::Ready(Err(broadcast::error::RecvError::Lagged(_))) => {
                        cx.waker().wake_by_ref();
                        std::task::Poll::Pending
                    }
                    std::task::Poll::Pending => std::task::Poll::Pending,
                }
            }));

        return Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response();
    }

    Json(server.handle_rpc(req).await).into_response()
}
