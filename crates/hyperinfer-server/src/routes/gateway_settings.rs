//! Admin API routes for Gateway Plugins, PII Redaction, and Semantic Cache settings (HYP-73).

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use hyperinfer_core::{RedactionConfig, RedactionPlugin};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::auth::RequireAdmin;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SemanticCacheConfig {
    pub enabled: bool,
    pub similarity_threshold: f64,
    pub ttl_secs: u64,
    pub embedding_provider: String,
    pub embedding_model: String,
    pub sse_replay: bool,
}

impl Default for SemanticCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            similarity_threshold: 0.92,
            ttl_secs: 3600,
            embedding_provider: "openai".to_string(),
            embedding_model: "text-embedding-3-small".to_string(),
            sse_replay: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySettings {
    pub pipeline_plugins: Vec<String>,
    pub redaction: RedactionConfig,
    pub cache: SemanticCacheConfig,
}

#[derive(Clone)]
pub struct GatewaySettingsState {
    pub settings: Arc<RwLock<GatewaySettings>>,
}

impl GatewaySettingsState {
    pub fn new(pipeline_plugins: Vec<String>) -> Self {
        Self {
            settings: Arc::new(RwLock::new(GatewaySettings {
                pipeline_plugins,
                redaction: RedactionConfig::default(),
                cache: SemanticCacheConfig::default(),
            })),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TestRedactionRequest {
    pub sample: String,
}

#[derive(Debug, Serialize)]
pub struct TestRedactionResponse {
    pub redacted: String,
}

pub async fn get_gateway_settings(
    State(state): State<GatewaySettingsState>,
    _admin: RequireAdmin,
) -> impl IntoResponse {
    let settings = state.settings.read().await.clone();
    Json(settings)
}

pub async fn update_redaction_config(
    State(state): State<GatewaySettingsState>,
    _admin: RequireAdmin,
    Json(config): Json<RedactionConfig>,
) -> impl IntoResponse {
    for (pattern, _replacement) in &config.custom_patterns {
        if let Err(err) = regex::Regex::new(pattern) {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid custom redaction regex '{}': {}", pattern, err),
            )
                .into_response();
        }
    }

    let mut guard = state.settings.write().await;
    guard.redaction = config.clone();
    Json(config).into_response()
}

pub async fn test_redaction(
    State(state): State<GatewaySettingsState>,
    _admin: RequireAdmin,
    Json(req): Json<TestRedactionRequest>,
) -> impl IntoResponse {
    let config = state.settings.read().await.redaction.clone();
    let plugin = RedactionPlugin::new(config);
    let redacted = plugin.redact_str(&req.sample);
    Json(TestRedactionResponse { redacted })
}

pub async fn update_cache_config(
    State(state): State<GatewaySettingsState>,
    _admin: RequireAdmin,
    Json(config): Json<SemanticCacheConfig>,
) -> impl IntoResponse {
    if !(0.5..=1.0).contains(&config.similarity_threshold)
        || !config.similarity_threshold.is_finite()
    {
        return (
            StatusCode::BAD_REQUEST,
            "similarity_threshold must be between 0.5 and 1.0",
        )
            .into_response();
    }
    if config.ttl_secs == 0 {
        return (StatusCode::BAD_REQUEST, "ttl_secs must be greater than 0").into_response();
    }

    let mut guard = state.settings.write().await;
    guard.cache = config.clone();
    Json(config).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{auth_middleware, create_auth_token};
    use axum::{
        middleware,
        routing::{get, post, put},
        Router,
    };
    use axum_test::TestServer;
    use hyperinfer_core::User;

    fn test_server() -> (TestServer, String) {
        let secret = "gw-settings-secret";
        let state = GatewaySettingsState::new(vec!["redaction".to_string()]);
        let app = Router::new()
            .route("/v1/admin/gateway/settings", get(get_gateway_settings))
            .route("/v1/admin/gateway/redaction", put(update_redaction_config))
            .route("/v1/admin/gateway/redaction/test", post(test_redaction))
            .route("/v1/admin/gateway/cache", put(update_cache_config))
            .layer(middleware::from_fn_with_state(
                Arc::new(secret.to_string()),
                auth_middleware,
            ))
            .with_state(state);

        let admin = User {
            id: "u1".into(),
            team_id: "t1".into(),
            email: "admin@test.local".into(),
            role: "admin".into(),
            password_hash: None,
            created_at: chrono::Utc::now(),
        };
        let token = create_auth_token(&admin, secret, 3600).unwrap();
        (TestServer::new(app), token)
    }

    #[tokio::test]
    async fn test_redaction_validation_and_live_preview() {
        let (server, token) = test_server();

        // Reject invalid custom regex
        let bad = server
            .put("/v1/admin/gateway/redaction")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "redact_api_keys": true,
                "custom_patterns": [["[unclosed", "[X]"]]
            }))
            .await;
        bad.assert_status(StatusCode::BAD_REQUEST);

        // Save valid custom pattern and test preview
        let ok = server
            .put("/v1/admin/gateway/redaction")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "redact_api_keys": true,
                "redact_emails": true,
                "custom_patterns": [["SECRET-[0-9]+", "[CUSTOM_SECRET]"]]
            }))
            .await;
        ok.assert_status_ok();

        let preview = server
            .post("/v1/admin/gateway/redaction/test")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "sample": "Contact alice@example.com with SECRET-9999"
            }))
            .await;
        preview.assert_status_ok();
        let body: serde_json::Value = preview.json();
        assert_eq!(
            body["redacted"],
            "Contact [REDACTED_EMAIL] with [CUSTOM_SECRET]"
        );
    }

    #[tokio::test]
    async fn test_cache_config_validation_and_update() {
        let (server, token) = test_server();

        let bad = server
            .put("/v1/admin/gateway/cache")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "enabled": true,
                "similarity_threshold": 0.2,
                "ttl_secs": 3600
            }))
            .await;
        bad.assert_status(StatusCode::BAD_REQUEST);

        let ok = server
            .put("/v1/admin/gateway/cache")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "enabled": true,
                "similarity_threshold": 0.95,
                "ttl_secs": 1800,
                "embedding_provider": "openai",
                "embedding_model": "text-embedding-3-large",
                "sse_replay": true
            }))
            .await;
        ok.assert_status_ok();

        let get_resp = server
            .get("/v1/admin/gateway/settings")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        get_resp.assert_status_ok();
        let settings: GatewaySettings = get_resp.json();
        assert!(settings.cache.enabled);
        assert_eq!(settings.cache.ttl_secs, 1800);
        assert_eq!(settings.pipeline_plugins, vec!["redaction"]);
    }
}
