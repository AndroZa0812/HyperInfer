//! HyperCode Agent Sessions & Trajectory Observability API routes (HYP-76).
//!
//! Provides fleet-wide **read-only observability** for HyperCode agent runs
//! attributed by team, user, and virtual API key, plus a redacted ingest
//! endpoint. Automatically applies `RedactionPlugin` (HYP-35 / HYP-67) on
//! ingest and read, calculates weighted cost (HYP-68) and cache hit savings
//! (HYP-69), and never executes agent tools on the gateway server.

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use hyperinfer_core::{
    ConfigStore, CostCalculator, Database, DbError, ListAgentSessionsFilter, McpToolCallSummary,
    RedactionPlugin, TrajectoryEvent, UpsertAgentSessionRequest, WeightedUsage,
};
use serde::Deserialize;

use crate::auth::AuthClaims;
use crate::state::AppState;

const ALLOWED_STATUSES: &[&str] = &["running", "completed", "failed", "cancelled"];

#[derive(Debug, Deserialize)]
pub struct AgentSessionsQuery {
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub status: Option<String>,
    pub model: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct IngestAgentSessionBody {
    pub session_key: String,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    #[serde(default = "default_agent_name")]
    pub agent_name: String,
    pub model: String,
    #[serde(default = "default_status")]
    pub status: String,
    pub title: Option<String>,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub reasoning_tokens: i64,
    #[serde(default)]
    pub cost_cents: f64,
    #[serde(default)]
    pub cache_savings_cents: f64,
    #[serde(default)]
    pub cache_hits: i32,
    #[serde(default)]
    pub total_latency_ms: i64,
    #[serde(default)]
    pub mcp_tool_calls: Vec<McpToolCallSummary>,
    #[serde(default)]
    pub trajectory: Vec<TrajectoryEvent>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

fn default_agent_name() -> String {
    "hypercode".to_string()
}

fn default_status() -> String {
    "completed".to_string()
}

fn redact_session_fields(
    redactor: &RedactionPlugin,
    agent_name: &mut String,
    title: &mut Option<String>,
    mcp_tool_calls: &mut [McpToolCallSummary],
    trajectory: &mut [TrajectoryEvent],
) {
    *agent_name = redactor.redact_str(agent_name);
    if let Some(ref mut t) = title {
        *t = redactor.redact_str(t);
    }
    for call in mcp_tool_calls.iter_mut() {
        call.server = redactor.redact_str(&call.server);
        call.tool = redactor.redact_str(&call.tool);
    }
    for ev in trajectory.iter_mut() {
        ev.content = redactor.redact_str(&ev.content);
        if let Some(ref mut name) = ev.tool_name {
            *name = redactor.redact_str(name);
        }
        if let Some(ref mut server) = ev.mcp_server {
            *server = redactor.redact_str(server);
        }
    }
}

fn compute_cost_and_savings(
    model: &str,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    provided_cost_cents: f64,
    provided_savings_cents: f64,
) -> (f64, f64) {
    let calc = CostCalculator::with_defaults();
    let in_tok = input_tokens.max(0) as u64;
    let cached_tok = cached_input_tokens.max(0) as u64;
    let out_tok = output_tokens.max(0) as u64;
    let reason_tok = reasoning_tokens.max(0) as u64;

    let cost_cents = if provided_cost_cents.is_finite() && provided_cost_cents > 0.0 {
        provided_cost_cents
    } else if in_tok > 0 || cached_tok > 0 || out_tok > 0 || reason_tok > 0 {
        calc.calculate_cost_cents(
            model,
            &WeightedUsage {
                input_tokens: in_tok,
                cached_input_tokens: cached_tok,
                output_tokens: out_tok,
                reasoning_tokens: reason_tok,
            },
        )
    } else {
        0.0
    };

    let cache_savings_cents = if provided_savings_cents.is_finite() && provided_savings_cents > 0.0
    {
        provided_savings_cents
    } else if cached_tok > 0 {
        let without_cache = calc.calculate_cost_cents(
            model,
            &WeightedUsage {
                input_tokens: in_tok.saturating_add(cached_tok),
                cached_input_tokens: 0,
                output_tokens: out_tok,
                reasoning_tokens: reason_tok,
            },
        );
        let with_cache = calc.calculate_cost_cents(
            model,
            &WeightedUsage {
                input_tokens: in_tok,
                cached_input_tokens: cached_tok,
                output_tokens: out_tok,
                reasoning_tokens: reason_tok,
            },
        );
        (without_cache - with_cache).max(0.0)
    } else {
        0.0
    };

    (cost_cents, cache_savings_cents)
}

/// `POST /v1/agent/sessions` — Ingest or update a HyperCode agent session with mandatory PII redaction.
pub async fn upsert_agent_session<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Json(mut body): Json<IngestAgentSessionBody>,
) -> impl IntoResponse {
    if body.session_key.trim().is_empty() || body.model.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "session_key and model are required",
        )
            .into_response();
    }
    if body.session_key.trim().len() > 255
        || body.model.trim().len() > 255
        || body.agent_name.len() > 128
        || body.title.as_ref().is_some_and(|t| t.len() > 512)
    {
        return (
            StatusCode::BAD_REQUEST,
            "Field exceeds maximum allowed length (session_key/model <= 255, agent_name <= 128, title <= 512)",
        )
            .into_response();
    }
    if !ALLOWED_STATUSES.contains(&body.status.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            "status must be one of: running, completed, failed, cancelled",
        )
            .into_response();
    }

    let team_id = if claims.role == "admin" {
        body.team_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| claims.team_id.clone())
    } else {
        claims.team_id.clone()
    };
    let user_id = if claims.role == "admin" {
        body.user_id
            .filter(|s| !s.trim().is_empty())
            .or_else(|| Some(claims.sub.clone()))
    } else {
        Some(claims.sub.clone())
    };

    let api_key_id = body.api_key_id.filter(|s| !s.trim().is_empty());
    if claims.role != "admin" {
        if let Some(ref kid) = api_key_id {
            match state.db.get_api_key(kid).await {
                Ok(Some(key)) if key.team_id == claims.team_id => {}
                Ok(_) => {
                    return (
                        StatusCode::FORBIDDEN,
                        "API key does not belong to caller team",
                    )
                        .into_response();
                }
                Err(DbError::InvalidUuid) => {
                    return (StatusCode::BAD_REQUEST, "Invalid API key UUID").into_response();
                }
                Err(_) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to validate API key ownership",
                    )
                        .into_response();
                }
            }
        }
    }

    let redactor = state
        .plugins
        .redaction()
        .map(|arc| (*arc).clone())
        .unwrap_or_else(RedactionPlugin::with_defaults);
    redact_session_fields(
        &redactor,
        &mut body.agent_name,
        &mut body.title,
        &mut body.mcp_tool_calls,
        &mut body.trajectory,
    );

    let (cost_cents, cache_savings_cents) = compute_cost_and_savings(
        &body.model,
        body.input_tokens,
        body.cached_input_tokens,
        body.output_tokens,
        body.reasoning_tokens,
        body.cost_cents,
        body.cache_savings_cents,
    );

    let req = UpsertAgentSessionRequest {
        session_key: body.session_key.trim().to_string(),
        team_id,
        user_id,
        api_key_id,
        agent_name: body.agent_name,
        model: body.model.trim().to_string(),
        status: body.status,
        title: body.title,
        input_tokens: body.input_tokens.max(0),
        cached_input_tokens: body.cached_input_tokens.max(0),
        output_tokens: body.output_tokens.max(0),
        reasoning_tokens: body.reasoning_tokens.max(0),
        cost_cents,
        cache_savings_cents,
        cache_hits: body.cache_hits.max(0),
        total_latency_ms: body.total_latency_ms.max(0),
        mcp_tool_calls: body.mcp_tool_calls,
        trajectory: body.trajectory,
        started_at: body.started_at,
        ended_at: body.ended_at,
    };

    match state.db.upsert_agent_session(req).await {
        Ok(session) => (StatusCode::CREATED, Json(session)).into_response(),
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to record agent session",
        )
            .into_response(),
    }
}

/// `GET /v1/agent/sessions` — Paginated HyperCode agent sessions with tenant isolation.
pub async fn list_agent_sessions<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<AgentSessionsQuery>,
) -> impl IntoResponse {
    let team_id = if claims.role == "admin" {
        query.team_id.filter(|s| !s.trim().is_empty())
    } else {
        Some(claims.team_id.clone())
    };

    let filter = ListAgentSessionsFilter {
        team_id,
        user_id: query.user_id.filter(|s| !s.trim().is_empty()),
        api_key_id: query.api_key_id.filter(|s| !s.trim().is_empty()),
        status: query.status.filter(|s| !s.trim().is_empty()),
        model: query.model.filter(|s| !s.trim().is_empty()),
        limit: query.limit.unwrap_or(25).clamp(1, 200),
        offset: query.offset.unwrap_or(0).max(0),
    };

    match state.db.list_agent_sessions(filter).await {
        Ok(mut page) => {
            let redactor = state
                .plugins
                .redaction()
                .map(|arc| (*arc).clone())
                .unwrap_or_else(RedactionPlugin::with_defaults);
            for s in &mut page.sessions {
                redact_session_fields(
                    &redactor,
                    &mut s.agent_name,
                    &mut s.title,
                    &mut s.mcp_tool_calls,
                    &mut s.trajectory,
                );
            }
            Json(page).into_response()
        }
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to list agent sessions",
        )
            .into_response(),
    }
}

/// `GET /v1/agent/sessions/{id}` — Inspect a single redacted HyperCode session trajectory (read-only).
pub async fn get_agent_session<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_agent_session(&id).await {
        Ok(Some(mut session)) => {
            if claims.role != "admin" && session.team_id != claims.team_id {
                return (StatusCode::FORBIDDEN, "Forbidden").into_response();
            }
            // Defense-in-depth redaction on read (HYP-35 / HYP-67)
            let redactor = state
                .plugins
                .redaction()
                .map(|arc| (*arc).clone())
                .unwrap_or_else(RedactionPlugin::with_defaults);
            redact_session_fields(
                &redactor,
                &mut session.agent_name,
                &mut session.title,
                &mut session.mcp_tool_calls,
                &mut session.trajectory,
            );
            Json(session).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Agent session not found").into_response(),
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load agent session",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{auth_middleware, create_auth_token};
    use crate::standalone::{InMemoryConfigStore, InMemoryDb};
    use axum::{middleware, routing::get, Router};
    use axum_test::TestServer;
    use hyperinfer_core::{AgentSession, Config, PluginPipeline};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    async fn setup_test_server() -> (TestServer, String, String, String, String) {
        let db = InMemoryDb::new();
        let t1 = db.create_team("Team Alpha", 10_000).await.unwrap();
        let t2 = db.create_team("Team Beta", 10_000).await.unwrap();
        let admin = db
            .create_user(&t1.id, "admin@test.local", "admin", None)
            .await
            .unwrap();
        let member = db
            .create_user(&t2.id, "member@test.local", "member", None)
            .await
            .unwrap();
        let k1 = db
            .create_api_key("hash1", &admin.id, &t1.id, Some("hc-key".into()))
            .await
            .unwrap();

        let secret = "agent-sessions-secret";
        let config = Arc::new(RwLock::new(Config {
            api_keys: HashMap::new(),
            routing_rules: Vec::new(),
            quotas: HashMap::new(),
            model_aliases: HashMap::new(),
            default_provider: None,
        }));
        let state = AppState {
            config: config.clone(),
            db,
            config_manager: InMemoryConfigStore::new(config),
            admin_token: Arc::new("admin-tok".to_string()),
            jwt_secret: Arc::new(secret.to_string()),
            proxy_router: None,
            rate_limiter: None,
            plugins: PluginPipeline::new(),
        };

        let app = Router::new()
            .route(
                "/v1/agent/sessions",
                get(list_agent_sessions).post(upsert_agent_session),
            )
            .route("/v1/agent/sessions/{id}", get(get_agent_session))
            .layer(middleware::from_fn_with_state(
                state.jwt_secret.clone(),
                auth_middleware,
            ))
            .with_state(state);

        let admin_jwt = create_auth_token(&admin, secret, 3600).unwrap();
        let member_jwt = create_auth_token(&member, secret, 3600).unwrap();

        (TestServer::new(app), admin_jwt, member_jwt, t1.id, k1.id)
    }

    #[tokio::test]
    async fn test_agent_session_redaction_cost_and_tenant_isolation() {
        let (server, admin_jwt, member_jwt, t1_id, k1_id) = setup_test_server().await;

        let create = server
            .post("/v1/agent/sessions")
            .add_header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "session_key": "hc-run-001",
                "team_id": t1_id,
                "api_key_id": k1_id,
                "model": "gpt-4o",
                "status": "completed",
                "title": "Fix auth for alice@example.com with sk-proj-abcdefghijklmnop",
                "input_tokens": 2000,
                "cached_input_tokens": 1000,
                "output_tokens": 500,
                "cache_hits": 2,
                "total_latency_ms": 1450,
                "mcp_tool_calls": [
                    { "server": "github", "tool": "search_code", "count": 3, "avg_latency_ms": 110, "errors": 0 }
                ],
                "trajectory": [
                    {
                        "step": 1,
                        "timestamp": "2026-11-26T12:00:00Z",
                        "kind": "prompt",
                        "role": "user",
                        "content": "Contact alice@example.com with key sk-proj-abcdefghijklmnop",
                        "cache_hit": false
                    },
                    {
                        "step": 2,
                        "timestamp": "2026-11-26T12:00:01Z",
                        "kind": "mcp_tool_call",
                        "role": "assistant",
                        "content": "Calling github.search_code",
                        "tool_name": "search_code",
                        "mcp_server": "github",
                        "tokens": 250,
                        "latency_ms": 110,
                        "cache_hit": true
                    }
                ]
            }))
            .await;
        create.assert_status(StatusCode::CREATED);
        let created: AgentSession = create.json();

        // Title and trajectory are redacted on ingest
        assert_eq!(
            created.title.as_deref(),
            Some("Fix auth for [REDACTED_EMAIL] with [REDACTED_API_KEY]")
        );
        assert!(created.cost_cents > 0.0);
        assert!(created.cache_savings_cents > 0.0);
        assert_eq!(created.mcp_tool_calls.len(), 1);
        assert_eq!(
            created.trajectory[0].content,
            "Contact [REDACTED_EMAIL] with key [REDACTED_API_KEY]"
        );

        // Admin can inspect full redacted trajectory
        let detail = server
            .get(&format!("/v1/agent/sessions/{}", created.id))
            .add_header("Authorization", format!("Bearer {admin_jwt}"))
            .await;
        detail.assert_status_ok();
        let inspected: AgentSession = detail.json();
        assert_eq!(inspected.trajectory.len(), 2);

        // Non-admin from another team cannot list or inspect Team Alpha's session
        let member_list = server
            .get("/v1/agent/sessions")
            .add_header("Authorization", format!("Bearer {member_jwt}"))
            .await;
        member_list.assert_status_ok();
        let page: hyperinfer_core::AgentSessionsPage = member_list.json();
        assert_eq!(page.total, 0);

        let member_detail = server
            .get(&format!("/v1/agent/sessions/{}", created.id))
            .add_header("Authorization", format!("Bearer {member_jwt}"))
            .await;
        member_detail.assert_status(StatusCode::FORBIDDEN);

        // Non-admin cannot attribute a session to another team's API key
        let spoofed_key = server
            .post("/v1/agent/sessions")
            .add_header("Authorization", format!("Bearer {member_jwt}"))
            .json(&serde_json::json!({
                "session_key": "hc-spoof-01",
                "api_key_id": k1_id,
                "model": "gpt-4o",
                "status": "completed"
            }))
            .await;
        spoofed_key.assert_status(StatusCode::FORBIDDEN);

        // Non-admin user_id is always forced to authenticated subject and MCP tool metadata is redacted
        let member_own = server
            .post("/v1/agent/sessions")
            .add_header("Authorization", format!("Bearer {member_jwt}"))
            .json(&serde_json::json!({
                "session_key": "hc-member-own",
                "user_id": "00000000-0000-0000-0000-000000000000",
                "model": "gpt-4o",
                "status": "completed",
                "mcp_tool_calls": [
                    { "server": "srv-alice@example.com", "tool": "run-sk-proj-abcdefghijklmnop", "count": 1 }
                ]
            }))
            .await;
        member_own.assert_status(StatusCode::CREATED);
        let own_session: AgentSession = member_own.json();
        assert_ne!(
            own_session.user_id.as_deref(),
            Some("00000000-0000-0000-0000-000000000000")
        );
        assert_eq!(own_session.mcp_tool_calls[0].server, "[REDACTED_EMAIL]");
        assert_eq!(own_session.mcp_tool_calls[0].tool, "run-[REDACTED_API_KEY]");

        // Reject oversized session_key (> 255 chars) with 400 Bad Request
        let too_long = server
            .post("/v1/agent/sessions")
            .add_header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "session_key": "x".repeat(256),
                "model": "gpt-4o",
                "status": "completed"
            }))
            .await;
        too_long.assert_status(StatusCode::BAD_REQUEST);
    }
}
