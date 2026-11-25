//! Admin API routes for managing the multiplexing MCP Gateway (HYP-66 / HYP-70).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::RequireAdmin;
use crate::mcp::{McpState, McpToolDefinition};
use crate::mcp_gateway::{McpTeamPolicy, UpstreamMcpServer, UpstreamTransport};

#[derive(Debug, Deserialize)]
pub struct RegisterUpstreamRequest {
    pub name: String,
    /// `"streamable_http"` (default) or `"sse"`.
    #[serde(default = "default_transport_kind")]
    pub transport: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_transport_kind() -> String {
    "streamable_http".to_string()
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct UpstreamSummary {
    pub name: String,
    pub transport: String,
    pub url: String,
    pub enabled: bool,
    pub header_keys: Vec<String>,
}

impl From<UpstreamMcpServer> for UpstreamSummary {
    fn from(s: UpstreamMcpServer) -> Self {
        let (transport, url) = match s.transport {
            UpstreamTransport::StreamableHttp { url } => ("streamable_http".to_string(), url),
            UpstreamTransport::Sse { url } => ("sse".to_string(), url),
        };
        let mut header_keys: Vec<String> = s.headers.keys().cloned().collect();
        header_keys.sort();
        Self {
            name: s.name,
            transport,
            url,
            enabled: s.enabled,
            header_keys,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct McpToolsResponse {
    pub tools: Vec<McpToolDefinition>,
    pub catalog_meta_tools: Vec<McpToolDefinition>,
}

pub async fn list_upstreams(
    State(state): State<McpState>,
    _admin: RequireAdmin,
) -> impl IntoResponse {
    let mut upstreams: Vec<UpstreamSummary> = state
        .gateway
        .list_upstreams()
        .await
        .into_iter()
        .map(UpstreamSummary::from)
        .collect();
    upstreams.sort_by(|a, b| a.name.cmp(&b.name));
    Json(upstreams)
}

pub async fn register_upstream(
    State(state): State<McpState>,
    _admin: RequireAdmin,
    Json(req): Json<RegisterUpstreamRequest>,
) -> impl IntoResponse {
    let name = req.name.trim().to_string();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return (
            StatusCode::BAD_REQUEST,
            "Upstream name must be non-empty and contain only alphanumeric, '-', or '_' characters",
        )
            .into_response();
    }

    let url = req.url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return (
            StatusCode::BAD_REQUEST,
            "Upstream URL must start with http:// or https://",
        )
            .into_response();
    }

    let transport = match req.transport.as_str() {
        "streamable_http" => UpstreamTransport::StreamableHttp { url },
        "sse" => UpstreamTransport::Sse { url },
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "Invalid transport: expected 'streamable_http' or 'sse'",
            )
                .into_response();
        }
    };

    let server = UpstreamMcpServer {
        name,
        transport,
        headers: req.headers,
        enabled: req.enabled,
    };
    let summary = UpstreamSummary::from(server.clone());
    state.gateway.register_upstream(server).await;

    (StatusCode::CREATED, Json(summary)).into_response()
}

pub async fn remove_upstream(
    State(state): State<McpState>,
    _admin: RequireAdmin,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if state.gateway.remove_upstream(&name).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (StatusCode::NOT_FOUND, "Upstream server not found").into_response()
    }
}

pub async fn list_tools(State(state): State<McpState>, _admin: RequireAdmin) -> impl IntoResponse {
    let local_tools = state.tools.read().await;
    let mut tools: Vec<McpToolDefinition> =
        local_tools.values().map(|t| t.definition.clone()).collect();
    drop(local_tools);

    tools.extend(state.gateway.list_upstream_tools().await);
    tools.sort_by(|a, b| a.name.cmp(&b.name));

    Json(McpToolsResponse {
        tools,
        catalog_meta_tools: crate::mcp_gateway::McpGateway::catalog_meta_tools(),
    })
}

pub async fn get_team_policy(
    State(state): State<McpState>,
    _admin: RequireAdmin,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    let policy = state.gateway.get_team_policy(&team_id).await;
    Json(policy)
}

pub async fn update_team_policy(
    State(state): State<McpState>,
    _admin: RequireAdmin,
    Path(team_id): Path<String>,
    Json(policy): Json<McpTeamPolicy>,
) -> impl IntoResponse {
    for pattern in policy
        .allow_patterns
        .iter()
        .chain(policy.deny_patterns.iter())
    {
        if let Err(err) = regex::Regex::new(pattern) {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid regex pattern '{}': {}", pattern, err),
            )
                .into_response();
        }
    }

    state
        .gateway
        .set_team_policy(&team_id, policy.clone())
        .await;
    Json(policy).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{auth_middleware, create_auth_token};
    use axum::{
        middleware,
        routing::{delete, get},
        Router,
    };
    use axum_test::TestServer;
    use hyperinfer_core::User;
    use std::sync::Arc;

    fn test_app() -> (TestServer, String) {
        let secret = "test-mcp-admin-secret";
        let state = McpState::new(secret);
        let app = Router::new()
            .route(
                "/v1/admin/mcp/upstreams",
                get(list_upstreams).post(register_upstream),
            )
            .route("/v1/admin/mcp/upstreams/{name}", delete(remove_upstream))
            .route("/v1/admin/mcp/tools", get(list_tools))
            .route(
                "/v1/admin/mcp/policies/{team_id}",
                get(get_team_policy).put(update_team_policy),
            )
            .layer(middleware::from_fn_with_state(
                Arc::new(secret.to_string()),
                auth_middleware,
            ))
            .with_state(state);

        let admin_user = User {
            id: "u1".to_string(),
            team_id: "t1".to_string(),
            email: "admin@test.local".to_string(),
            role: "admin".to_string(),
            password_hash: None,
            created_at: chrono::Utc::now(),
        };
        let token = create_auth_token(&admin_user, secret, 3600).unwrap();

        (TestServer::new(app), token)
    }

    #[tokio::test]
    async fn test_upstream_crud_hides_secret_header_values() {
        let (server, token) = test_app();

        let create_resp = server
            .post("/v1/admin/mcp/upstreams")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "name": "github",
                "transport": "streamable_http",
                "url": "https://mcp.github.example/mcp",
                "headers": { "Authorization": "Bearer super-secret-value" },
                "enabled": true
            }))
            .await;
        create_resp.assert_status(StatusCode::CREATED);
        let body = create_resp.text();
        assert!(!body.contains("super-secret-value"));
        assert!(body.contains("Authorization"));

        let list_resp = server
            .get("/v1/admin/mcp/upstreams")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        list_resp.assert_status_ok();
        let list_body = list_resp.text();
        assert!(list_body.contains("github"));
        assert!(!list_body.contains("super-secret-value"));

        let del_resp = server
            .delete("/v1/admin/mcp/upstreams/github")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        del_resp.assert_status(StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_policy_validation_and_persistence() {
        let (server, token) = test_app();

        let bad_resp = server
            .put("/v1/admin/mcp/policies/team-1")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "allow_patterns": ["[unclosed"],
                "deny_patterns": [],
                "catalog_mode": true
            }))
            .await;
        bad_resp.assert_status(StatusCode::BAD_REQUEST);

        let ok_resp = server
            .put("/v1/admin/mcp/policies/team-1")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "allow_patterns": ["^github__.*"],
                "deny_patterns": ["github__delete_.*"],
                "catalog_mode": true
            }))
            .await;
        ok_resp.assert_status_ok();

        let get_resp = server
            .get("/v1/admin/mcp/policies/team-1")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        get_resp.assert_status_ok();
        let policy: McpTeamPolicy = get_resp.json();
        assert!(policy.catalog_mode);
        assert_eq!(policy.allow_patterns, vec!["^github__.*"]);
        assert_eq!(policy.deny_patterns, vec!["github__delete_.*"]);
    }
}
