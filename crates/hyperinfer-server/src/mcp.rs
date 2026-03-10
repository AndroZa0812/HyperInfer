//! MCP (Model Context Protocol) host implementation.
//!
//! Implements the SSE-based bidirectional transport described in the project spec:
//!
//! * `GET /mcp/sse` — establishes a long-lived SSE stream for the client session.
//! * `POST /mcp/message` — accepts JSON-RPC 2.0 messages and fans them out to the
//!   correct session via the in-memory session registry.
//!
//! Both endpoints are protected by the [`JwtAuthLayer`] tower middleware which
//! validates `Authorization: Bearer <token>` JWTs signed with HS256.
//!
//! # Session lifecycle
//!
//! 1. Client opens `GET /mcp/sse`.  The server creates a `McpSession` with a
//!    random UUID, inserts it into the `SessionRegistry`, and starts streaming
//!    SSE events.  The first event is `endpoint` which tells the client the URL
//!    it must use for `POST /mcp/message?session_id=<uuid>`.
//! 2. Client POSTs JSON-RPC commands to `POST /mcp/message?session_id=<uuid>`.
//!    The handler looks up the session, dispatches the command, and sends the
//!    reply through the session's mpsc channel so it appears on the SSE stream.
//! 3. When the client disconnects the SSE stream is dropped; the cleanup task
//!    removes the session from the registry.

use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{sse::Event, IntoResponse, Response, Sse},
    Extension, Json,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::{mpsc, RwLock};
use tracing::warn;
use uuid::Uuid;

// ── JWT ─────────────────────────────────────────────────────────────────────

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

/// Claims expected inside an MCP Bearer JWT.
#[derive(Debug, Deserialize, Clone)]
pub struct McpClaims {
    /// Subject (agent / virtual key identifier).
    pub sub: String,
    /// Expiry (Unix timestamp seconds).  Validated automatically by
    /// `jsonwebtoken` when present.
    #[allow(dead_code)]
    pub exp: Option<u64>,
}

/// Axum middleware that validates `Authorization: Bearer <jwt>`.
///
/// Requires the JWT secret to be present in `McpState`.
/// On failure returns 401; on success the claims are added to request extensions
/// and the request is forwarded.
pub async fn jwt_auth_middleware(
    State(state): State<McpState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let token = match extract_bearer(req.headers()) {
        Some(t) => t,
        None => {
            return (StatusCode::UNAUTHORIZED, "Missing Authorization header").into_response();
        }
    };

    let claims = match validate_jwt(&token, &state.jwt_secret, state.allow_insecure_exp) {
        Ok(c) => c,
        Err(_) => {
            return (StatusCode::UNAUTHORIZED, "Invalid or expired JWT").into_response();
        }
    };

    req.extensions_mut().insert(claims);
    next.run(req).await
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?;
    let s = value.to_str().ok()?;
    let mut parts = s.splitn(2, char::is_whitespace);
    let scheme = parts.next()?;
    if scheme.eq_ignore_ascii_case("bearer") {
        return Some(parts.next()?.to_owned());
    }
    None
}

pub fn validate_jwt(
    token: &str,
    secret: &str,
    allow_insecure_exp: bool,
) -> Result<McpClaims, jsonwebtoken::errors::Error> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    if allow_insecure_exp {
        validation.required_spec_claims.remove("exp");
    }
    let data = decode::<McpClaims>(token, &key, &validation)?;
    Ok(data.claims)
}

/// Create a signed HS256 JWT for testing / internal use.
pub fn create_jwt(sub: &str, secret: &str) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};

    #[derive(Serialize)]
    struct Claims<'a> {
        sub: &'a str,
        exp: u64,
    }

    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + 3600;

    encode(
        &Header::default(),
        &Claims { sub, exp },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("JWT encoding should never fail for HS256")
}

// ── Session registry ─────────────────────────────────────────────────────────

const MAX_SESSIONS_GLOBAL: usize = 1000;
const MAX_SESSIONS_PER_OWNER: usize = 50;

struct SessionRemovalGuard {
    sessions: SessionRegistry,
    sid: String,
}

impl Drop for SessionRemovalGuard {
    fn drop(&mut self) {
        let sessions = self.sessions.clone();
        let sid = self.sid.clone();
        tokio::spawn(async move {
            let mut sessions = sessions.write().await;
            sessions.remove(&sid);
            tracing::debug!("MCP SSE session {} removed by Drop guard", sid);
        });
    }
}

/// A single SSE event frame sent through the session channel.
#[derive(Debug, Clone)]
pub struct SseFrame {
    pub event: String,
    pub data: String,
}

/// A live MCP client session.
#[derive(Clone)]
pub struct McpSession {
    pub id: String,
    /// Owner/subject from the JWT that created this session.
    pub owner: String,
    /// Sender side of the event channel.  Cloned into the SSE stream and into
    /// the message handler so both can push frames to the client.
    pub tx: mpsc::Sender<SseFrame>,
}

/// Thread-safe map of session_id → session.
pub type SessionRegistry = Arc<RwLock<HashMap<String, McpSession>>>;

// ── Shared state ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct McpState {
    pub sessions: SessionRegistry,
    pub jwt_secret: Arc<String>,
    /// If true, JWTs without the "exp" claim are accepted (insecure, for dev only).
    pub allow_insecure_exp: bool,
}

impl McpState {
    pub fn new(jwt_secret: impl Into<String>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret: Arc::new(jwt_secret.into()),
            allow_insecure_exp: false,
        }
    }

    pub fn new_with_insecure_exp(jwt_secret: impl Into<String>, allow_insecure: bool) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret: Arc::new(jwt_secret.into()),
            allow_insecure_exp: allow_insecure,
        }
    }
}

// ── JSON-RPC types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Option<Value>>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

/// Dispatch a JSON-RPC method to its handler and return the response value.
///
/// Extend this function to add new MCP tool methods.
pub fn dispatch_method(req: &JsonRpcRequest) -> JsonRpcResponse {
    let id = req.id.clone().flatten();
    match req.method.as_str() {
        "ping" => JsonRpcResponse::ok(id, serde_json::json!("pong")),
        "tools/list" => JsonRpcResponse::ok(id, serde_json::json!({ "tools": [] })),
        "initialize" => JsonRpcResponse::ok(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "hyperinfer-mcp", "version": env!("CARGO_PKG_VERSION") }
            }),
        ),
        unknown => JsonRpcResponse::err(id, -32601, format!("Method not found: {}", unknown)),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /mcp/sse`
///
/// Opens a long-lived SSE connection for the calling agent.  The first event
/// sent to the client is `endpoint` containing the POST URL the client should
/// use for all subsequent JSON-RPC messages.
pub async fn mcp_sse_handler(
    State(state): State<McpState>,
    Extension(claims): Extension<McpClaims>,
) -> impl IntoResponse {
    let session_id = Uuid::new_v4().to_string();
    let owner = claims.sub.clone();
    let (tx, mut rx) = mpsc::channel::<SseFrame>(64);

    // Register session with owner and enforce limits.
    {
        let mut sessions = state.sessions.write().await;

        // Enforce global cap
        if sessions.len() >= MAX_SESSIONS_GLOBAL {
            warn!(
                "MCP SSE: Global session limit reached ({})",
                MAX_SESSIONS_GLOBAL
            );
            return (StatusCode::SERVICE_UNAVAILABLE, "Server at capacity").into_response();
        }

        // Enforce per-owner cap
        let owner_count = sessions.values().filter(|s| s.owner == owner).count();
        if owner_count >= MAX_SESSIONS_PER_OWNER {
            warn!(
                "MCP SSE: Per-owner session limit reached ({}) for owner: {}",
                MAX_SESSIONS_PER_OWNER, owner
            );
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many concurrent sessions",
            )
                .into_response();
        }

        sessions.insert(
            session_id.clone(),
            McpSession {
                id: session_id.clone(),
                owner,
                tx: tx.clone(),
            },
        );
    }

    // RAII guard for cleanup on stream drop.
    let _guard = SessionRemovalGuard {
        sessions: state.sessions.clone(),
        sid: session_id.clone(),
    };

    // Send the `endpoint` event immediately so the client knows where to POST.
    if tx
        .send(SseFrame {
            event: "endpoint".to_string(),
            data: format!("/mcp/message?session_id={}", session_id),
        })
        .await
        .is_err()
    {
        warn!(
            "MCP SSE: failed to send endpoint event for session {}; client disconnected immediately",
            session_id
        );
        // Requirement (1): remove the entry if initial send fails
        let mut sessions = state.sessions.write().await;
        sessions.remove(&session_id);
    }

    // Build the SSE stream from the channel: convert SseFrame → axum Event.
    let stream = async_stream::stream! {
        let _guard = _guard; // Move guard into stream to ensure it lives as long as the stream

        // Disconnect idle sessions after 30 minutes.
        let idle_timeout = Duration::from_secs(30 * 60);

        loop {
            match tokio::time::timeout(idle_timeout, rx.recv()).await {
                Ok(Some(frame)) => {
                    let ev: Result<Event, Infallible> = Ok(
                        Event::default().event(frame.event).data(frame.data)
                    );
                    yield ev;
                }
                Ok(None) => {
                    // Channel closed (e.g. server shutdown)
                    break;
                }
                Err(_) => {
                    // Timeout elapsed
                    tracing::info!("MCP SSE session {} timed out due to inactivity", _guard.sid);
                    break;
                }
            }
        }

        // Guard is dropped here and removes the session.
    };

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}

/// Query parameters for `POST /mcp/message`.
#[derive(Deserialize)]
pub struct MessageQuery {
    pub session_id: String,
}

/// `POST /mcp/message?session_id=<uuid>`
///
/// Accepts a JSON-RPC 2.0 message body, dispatches it, and sends the response
/// back through the SSE channel of the identified session.
///
/// Returns 202 Accepted on success, 404 if the session does not exist,
/// 403 if the caller does not own the session.
pub async fn mcp_message_handler(
    State(state): State<McpState>,
    Extension(claims): Extension<McpClaims>,
    Query(query): Query<MessageQuery>,
    Json(rpc_req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    if rpc_req.jsonrpc != "2.0" {
        return (
            StatusCode::BAD_REQUEST,
            Json(JsonRpcResponse::err(
                None,
                -32600,
                "Invalid JSON-RPC version",
            )),
        )
            .into_response();
    }

    let sessions = state.sessions.read().await;
    let session = match sessions.get(&query.session_id) {
        Some(s) => s.clone(),
        None => {
            warn!("POST /mcp/message: unknown session_id {}", query.session_id);
            return (StatusCode::NOT_FOUND, "Session not found").into_response();
        }
    };

    if session.owner != claims.sub {
        warn!(
            "POST /mcp/message: unauthorized access attempt by {} to session owned by {}",
            claims.sub, session.owner
        );
        return (StatusCode::FORBIDDEN, "Not the session owner").into_response();
    }
    drop(sessions);

    let rpc_response = dispatch_method(&rpc_req);

    // If it's a notification (rpc_req.id is None), do not respond.
    if rpc_req.id.is_none() {
        return StatusCode::ACCEPTED.into_response();
    }

    let data = serde_json::to_string(&rpc_response).unwrap_or_else(|e| {
        warn!("Failed to serialize JSON-RPC response: {}", e);
        let id_val = rpc_req.id.flatten();
        let id_str = id_val
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok())
            .unwrap_or_else(|| "null".to_string());
        format!(
            r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32603,"message":"Internal error"}}}}"#,
            id_str
        )
    });
    let frame = SseFrame {
        event: "message".to_string(),
        data,
    };

    match session.tx.try_send(frame) {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(mpsc::error::TrySendError::Full(_)) => {
            warn!(
                "POST /mcp/message: session {} SSE channel full",
                query.session_id
            );
            (StatusCode::SERVICE_UNAVAILABLE, "Session stream full").into_response()
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            warn!(
                "POST /mcp/message: session {} SSE channel closed",
                query.session_id
            );
            (StatusCode::GONE, "Session stream closed").into_response()
        }
    }
}

// ── Stream helper (used by tests) ─────────────────────────────────────────────

/// Drain up to `limit` frames from a session channel (for testing).
pub async fn collect_frames(rx: &mut mpsc::Receiver<SseFrame>, limit: usize) -> Vec<SseFrame> {
    let mut frames = Vec::new();
    while frames.len() < limit {
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Some(frame)) => frames.push(frame),
            _ => break,
        }
    }
    frames
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        middleware,
        routing::{get, post},
        Router,
    };
    use axum_test::TestServer;
    use serde_json::json;

    /// Simple 200 GET handler used only in tests to validate JWT middleware.
    async fn health_ok() -> StatusCode {
        StatusCode::OK
    }

    fn build_app(state: McpState) -> Router {
        let mcp_routes = Router::new()
            .route("/mcp/message", post(mcp_message_handler))
            // `/mcp/health` is a test-only non-streaming endpoint so we can
            // check JWT middleware without hanging on an SSE stream.
            .route("/mcp/health", get(health_ok))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                jwt_auth_middleware,
            ))
            .with_state(state.clone());

        // SSE route added separately (not tested via TestServer since SSE
        // responses are infinite streams and block axum-test's response reader).
        let sse_route = Router::new()
            .route("/mcp/sse", get(mcp_sse_handler))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                jwt_auth_middleware,
            ))
            .with_state(state);

        mcp_routes.merge(sse_route)
    }

    fn make_jwt(secret: &str) -> String {
        create_jwt("test-agent", secret)
    }

    // ── JWT middleware ──────────────────────────────────────────────────────
    //
    // NOTE: We test the JWT middleware via `/mcp/health` (a plain 200 endpoint)
    // rather than `/mcp/sse` because SSE responses are infinite streams and
    // `axum-test` would block indefinitely waiting for the response body.

    #[tokio::test]
    async fn test_jwt_missing_header_returns_401() {
        let state = McpState::new("secret");
        let app = build_app(state);
        let server = TestServer::new(app);

        let resp: axum_test::TestResponse = server.get("/mcp/health").await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jwt_invalid_token_returns_401() {
        let state = McpState::new("secret");
        let app = build_app(state);
        let server = TestServer::new(app);

        let resp: axum_test::TestResponse = server
            .get("/mcp/health")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_static("Bearer not-a-valid-jwt"),
            )
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jwt_wrong_secret_returns_401() {
        let state = McpState::new("correct-secret");
        let app = build_app(state);
        let server = TestServer::new(app);

        let bad_token = create_jwt("agent", "wrong-secret");
        let auth_value = format!("Bearer {}", bad_token);
        let resp: axum_test::TestResponse = server
            .get("/mcp/health")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_str(&auth_value).unwrap(),
            )
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jwt_valid_case_insensitive_bearer_passes() {
        let secret = "test-secret";
        let state = McpState::new(secret);
        let app = build_app(state);
        let server = TestServer::new(app);

        // A valid token with a lowercase "bearer" should allow access.
        let token = make_jwt(secret);
        let auth_value = format!("bearer {}", token);
        let resp: axum_test::TestResponse = server
            .get("/mcp/health")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_str(&auth_value).unwrap(),
            )
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    // ── validate_jwt unit tests ─────────────────────────────────────────────

    #[test]
    fn test_validate_jwt_valid() {
        let secret = "my-secret";
        let token = create_jwt("alice", secret);
        let claims = validate_jwt(&token, secret, false).unwrap();
        assert_eq!(claims.sub, "alice");
    }

    #[test]
    fn test_validate_jwt_wrong_secret() {
        let token = create_jwt("alice", "correct");
        assert!(validate_jwt(&token, "wrong", false).is_err());
    }

    #[test]
    fn test_validate_jwt_malformed() {
        assert!(validate_jwt("not.a.jwt", "secret", false).is_err());
    }

    // ── dispatch_method ─────────────────────────────────────────────────────

    #[test]
    fn test_dispatch_ping() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Some(json!(1))),
            method: "ping".into(),
            params: None,
        };
        let resp = dispatch_method(&req);
        assert_eq!(resp.result, Some(json!("pong")));
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_dispatch_tools_list() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Some(json!(2))),
            method: "tools/list".into(),
            params: None,
        };
        let resp = dispatch_method(&req);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        let tools = &resp.result.unwrap()["tools"];
        assert!(tools.is_array());
    }

    #[test]
    fn test_dispatch_initialize() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Some(json!(3))),
            method: "initialize".into(),
            params: None,
        };
        let resp = dispatch_method(&req);
        assert!(resp.result.is_some());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn test_dispatch_unknown_method() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Some(json!(4))),
            method: "nonexistent".into(),
            params: None,
        };
        let resp = dispatch_method(&req);
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
    }

    // ── POST /mcp/message ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_message_unknown_session() {
        let secret = "s";
        let state = McpState::new(secret);
        let app = build_app(state);
        let server = TestServer::new(app);
        let token = make_jwt(secret);
        let auth_value = format!("Bearer {}", token);

        let resp: axum_test::TestResponse = server
            .post("/mcp/message?session_id=does-not-exist")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_str(&auth_value).unwrap(),
            )
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "ping"
            }))
            .await;
        assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_mcp_message_routes_to_session() {
        let secret = "s2";
        let state = McpState::new(secret);

        // Inject a fake session directly into the registry.
        let (tx, mut rx) = mpsc::channel::<SseFrame>(8);
        let session_id = "fake-session-123".to_string();
        {
            let mut sessions = state.sessions.write().await;
            sessions.insert(
                session_id.clone(),
                McpSession {
                    id: session_id.clone(),
                    owner: "test-agent".to_string(),
                    tx,
                },
            );
        }

        let app = build_app(state);
        let server = TestServer::new(app);
        let token = make_jwt(secret);
        let auth_value = format!("Bearer {}", token);

        let resp: axum_test::TestResponse = server
            .post(&format!("/mcp/message?session_id={}", session_id))
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_str(&auth_value).unwrap(),
            )
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "ping"
            }))
            .await;

        assert_eq!(resp.status_code(), StatusCode::ACCEPTED);

        // The response should have been sent through the channel as an SseFrame.
        let frame = rx.recv().await.unwrap();
        assert_eq!(frame.event, "message");
        assert!(
            frame.data.contains("pong"),
            "Expected 'pong' in frame data: {}",
            frame.data
        );
    }
}
