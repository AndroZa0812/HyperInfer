//! Admin API routes for Gateway Plugins, PII Redaction, and Semantic Cache settings (HYP-73).

use std::collections::HashMap;
use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use hyperinfer_core::{RedactionConfig, RedactionPlugin, RedactionVault, SmartRedactionBackend};
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
    pub live_redaction: Option<Arc<RedactionPlugin>>,
}

impl GatewaySettingsState {
    pub fn new(pipeline_plugins: Vec<String>) -> Self {
        Self::with_live_redaction(pipeline_plugins, None)
    }

    pub fn with_live_redaction(
        pipeline_plugins: Vec<String>,
        live_redaction: Option<Arc<RedactionPlugin>>,
    ) -> Self {
        let initial_redaction = live_redaction
            .as_ref()
            .map(|p| p.config())
            .unwrap_or_default();
        Self {
            settings: Arc::new(RwLock::new(GatewaySettings {
                pipeline_plugins,
                redaction: initial_redaction,
                cache: SemanticCacheConfig::default(),
            })),
            live_redaction,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_entries: Option<HashMap<String, String>>,
}

fn is_http_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

pub async fn get_gateway_settings(
    State(state): State<GatewaySettingsState>,
    _admin: RequireAdmin,
) -> impl IntoResponse {
    let mut settings = state.settings.read().await.clone();
    // Never echo raw smart-redaction LLM API keys back in GET responses.
    if settings.redaction.smart.llm_api_key.is_some() {
        settings.redaction.smart.llm_api_key = None;
    }
    Json(settings)
}

pub async fn update_redaction_config(
    State(state): State<GatewaySettingsState>,
    _admin: RequireAdmin,
    Json(mut config): Json<RedactionConfig>,
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

    if config.smart.enabled {
        if config.smart.timeout_ms == 0 {
            return (
                StatusCode::BAD_REQUEST,
                "smart.timeout_ms must be greater than 0",
            )
                .into_response();
        }
        if !config.smart.presidio_score_threshold.is_finite()
            || !(0.0..=1.0).contains(&config.smart.presidio_score_threshold)
        {
            return (
                StatusCode::BAD_REQUEST,
                "smart.presidio_score_threshold must be between 0.0 and 1.0",
            )
                .into_response();
        }
        match config.smart.backend {
            SmartRedactionBackend::Presidio => {
                if !is_http_url(&config.smart.presidio_url) {
                    return (
                        StatusCode::BAD_REQUEST,
                        "smart.presidio_url must start with http:// or https://",
                    )
                        .into_response();
                }
            }
            SmartRedactionBackend::Llm => {
                if !is_http_url(&config.smart.llm_base_url) {
                    return (
                        StatusCode::BAD_REQUEST,
                        "smart.llm_base_url must start with http:// or https://",
                    )
                        .into_response();
                }
                if config.smart.llm_model.trim().is_empty() {
                    return (StatusCode::BAD_REQUEST, "smart.llm_model must not be empty")
                        .into_response();
                }
            }
        }
    }

    // Preserve existing stored llm_api_key if the admin omitted it on update.
    {
        let existing = state.settings.read().await;
        if config
            .smart
            .llm_api_key
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            config.smart.llm_api_key = existing.redaction.smart.llm_api_key.clone();
        }
    }

    if let Some(plugin) = &state.live_redaction {
        plugin.update_config(config.clone());
    }

    let mut guard = state.settings.write().await;
    guard.redaction = config.clone();

    let mut sanitized = config;
    sanitized.smart.llm_api_key = None;
    Json(sanitized).into_response()
}

pub async fn test_redaction(
    State(state): State<GatewaySettingsState>,
    _admin: RequireAdmin,
    Json(req): Json<TestRedactionRequest>,
) -> impl IntoResponse {
    let plugin = if let Some(plugin) = &state.live_redaction {
        plugin.as_ref().clone()
    } else {
        let config = state.settings.read().await.redaction.clone();
        RedactionPlugin::new(config)
    };

    let reversible = plugin.config().reversible;
    let mut vault = if reversible {
        Some(RedactionVault::new())
    } else {
        None
    };

    match plugin.redact_str_async(&req.sample, vault.as_mut()).await {
        Ok(redacted) => {
            let vault_entries = vault
                .filter(|v| !v.is_empty())
                .map(|v| v.mappings().clone());
            Json(TestRedactionResponse {
                redacted,
                vault_entries,
            })
            .into_response()
        }
        Err(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Smart redaction failed: {}", err),
        )
            .into_response(),
    }
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

    fn test_server() -> (TestServer, String, Arc<RedactionPlugin>) {
        let secret = "gw-settings-secret";
        let live_redaction = Arc::new(RedactionPlugin::with_defaults());
        let state = GatewaySettingsState::with_live_redaction(
            vec!["redaction".to_string()],
            Some(live_redaction.clone()),
        );
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
        (TestServer::new(app), token, live_redaction)
    }

    #[tokio::test]
    async fn test_redaction_validation_and_live_preview() {
        let (server, token, live_redaction) = test_server();

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

        // Save valid custom pattern and test preview + live plugin hot-reload
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

        // Live pipeline plugin immediately applies the updated rules
        assert_eq!(
            live_redaction.redact_str("Contact alice@example.com with SECRET-9999"),
            "Contact [REDACTED_EMAIL] with [CUSTOM_SECRET]"
        );

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
        let (server, token, _) = test_server();

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

    #[tokio::test]
    async fn test_smart_redaction_validation_and_reversible_preview() {
        let (server, token, _) = test_server();

        // Reject invalid Presidio score threshold
        let bad_score = server
            .put("/v1/admin/gateway/redaction")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "smart": {
                    "enabled": true,
                    "backend": "presidio",
                    "presidio_url": "http://127.0.0.1:5001",
                    "presidio_score_threshold": 1.5
                }
            }))
            .await;
        bad_score.assert_status(StatusCode::BAD_REQUEST);

        // Reject non-HTTP Presidio URL
        let bad_url = server
            .put("/v1/admin/gateway/redaction")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "smart": {
                    "enabled": true,
                    "backend": "presidio",
                    "presidio_url": "file:///etc/passwd"
                }
            }))
            .await;
        bad_url.assert_status(StatusCode::BAD_REQUEST);

        // Reject empty LLM model
        let bad_model = server
            .put("/v1/admin/gateway/redaction")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "smart": {
                    "enabled": true,
                    "backend": "llm",
                    "llm_base_url": "http://127.0.0.1:11434/v1",
                    "llm_model": "   "
                }
            }))
            .await;
        bad_model.assert_status(StatusCode::BAD_REQUEST);

        // Enable reversible mode + LLM key (should not be echoed in responses)
        let ok = server
            .put("/v1/admin/gateway/redaction")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "redact_emails": true,
                "reversible": true,
                "placeholder_format": "indexed",
                "smart": {
                    "enabled": false,
                    "backend": "llm",
                    "llm_base_url": "http://127.0.0.1:11434/v1",
                    "llm_model": "qwen2.5:3b",
                    "llm_api_key": "super-secret-llm-key"
                }
            }))
            .await;
        ok.assert_status_ok();
        let saved: serde_json::Value = ok.json();
        assert!(saved["smart"].get("llm_api_key").is_none());

        let preview = server
            .post("/v1/admin/gateway/redaction/test")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "sample": "Reach alice@example.com"
            }))
            .await;
        preview.assert_status_ok();
        let body: serde_json::Value = preview.json();
        assert_eq!(body["redacted"], "Reach [REDACTED_EMAIL_1]");
        assert_eq!(
            body["vault_entries"]["[REDACTED_EMAIL_1]"],
            "alice@example.com"
        );
    }
}
