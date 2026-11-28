//! Dynamic Model Catalog & Provider Discovery routes (HYP-77).
//!
//! Exposes:
//! - `GET /v1/models` — OpenAI-compatible model listing backed by live `ModelCatalog`
//! - `GET /v1/admin/catalog` — Full model specs (context windows, live pricing, capabilities)
//! - `POST /v1/admin/catalog/refresh` — Force-refresh live catalog from `models.dev` / custom URL
//! - `POST /v1/admin/catalog/discover` — Query a provider's live `list_models()` endpoint

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use hyperinfer_core::{
    default_cache_path, Database, DiscoveredModel, ModelCatalog, ModelPricing, ModelSpec,
    DEFAULT_CACHE_TTL_SECS, DEFAULT_CATALOG_URL,
};
use hyperinfer_providers::{
    anthropic::AnthropicProvider, gemini::GeminiProvider, openai::OpenAiProvider, LlmProvider,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::auth::RequireAdmin;

#[derive(Clone, Default)]
pub struct CatalogState {
    pub catalog: Arc<RwLock<ModelCatalog>>,
}

impl CatalogState {
    pub fn new() -> Self {
        Self {
            catalog: Arc::new(RwLock::new(ModelCatalog::new())),
        }
    }

    pub fn with_catalog(catalog: ModelCatalog) -> Self {
        Self {
            catalog: Arc::new(RwLock::new(catalog)),
        }
    }

    /// Spawn a non-blocking background task to warm the catalog from disk cache or `models.dev`.
    pub fn spawn_background_load(&self) {
        let catalog_arc = self.catalog.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let cache = default_cache_path();
                let loaded = ModelCatalog::load_or_fetch(
                    cache.as_deref(),
                    Duration::from_secs(DEFAULT_CACHE_TTL_SECS),
                )
                .await;
                if !loaded.is_empty() {
                    *catalog_arc.write().await = loaded;
                }
            });
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    pub provider: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CatalogResponse {
    pub source: String,
    pub updated_at_secs: u64,
    pub count: usize,
    pub models: Vec<ModelSpec>,
}

#[derive(Debug, Deserialize)]
pub struct DiscoverModelsRequest {
    pub provider: String,
    /// Optional env-var reference like `$OPENAI_API_KEY` or `env:OPENAI_API_KEY`.
    pub api_key_ref: Option<String>,
    /// Optional custom base URL (validated against SSRF).
    pub base_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnrichedDiscoveredModel {
    #[serde(flatten)]
    pub discovered: DiscoveredModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_spec: Option<ModelSpec>,
}

#[derive(Clone)]
pub struct ModelsRouteState<D> {
    pub db: D,
    pub catalog: CatalogState,
}

/// `GET /v1/models` — OpenAI-compatible model listing of active gateway deployments.
pub async fn list_openai_models<D: Database>(
    State(state): State<ModelsRouteState<D>>,
    Query(query): Query<CatalogQuery>,
) -> impl IntoResponse {
    let deployments = match state.db.list_deployments("", Some(true)).await {
        Ok(list) => list,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to list active models",
            )
                .into_response()
        }
    };

    let provider_filter = query
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let catalog = state.catalog.catalog.read().await;
    let mut seen = HashSet::new();
    let mut data: Vec<serde_json::Value> = Vec::new();

    for d in deployments {
        if let Some(pf) = provider_filter {
            if !d.provider.eq_ignore_ascii_case(pf) {
                continue;
            }
        }
        let dedup_key = (d.provider.to_lowercase(), d.model.clone());
        if !seen.insert(dedup_key) {
            continue;
        }

        let spec = catalog.get(Some(&d.provider), &d.model);
        let pricing = match (d.cost_per_1k_input_tokens, d.cost_per_1k_output_tokens) {
            (Some(inp), Some(out)) => Some(ModelPricing {
                input_per_1m_usd: inp * 1000.0,
                output_per_1m_usd: out * 1000.0,
                cache_read_per_1m_usd: spec
                    .and_then(|s| s.pricing.as_ref())
                    .and_then(|p| p.cache_read_per_1m_usd),
                cache_write_per_1m_usd: spec
                    .and_then(|s| s.pricing.as_ref())
                    .and_then(|p| p.cache_write_per_1m_usd),
                reasoning_per_1m_usd: spec
                    .and_then(|s| s.pricing.as_ref())
                    .and_then(|p| p.reasoning_per_1m_usd),
            }),
            _ => spec.and_then(|s| s.pricing.clone()),
        };

        data.push(serde_json::json!({
            "id": d.model,
            "object": "model",
            "owned_by": d.provider,
            "context_window": spec.and_then(|s| s.context_window),
            "pricing": pricing,
        }));
    }

    Json(serde_json::json!({
        "object": "list",
        "data": data,
    }))
    .into_response()
}

/// `GET /v1/admin/catalog` — Admin model catalog with optional provider/search filter.
pub async fn get_admin_catalog(
    State(state): State<CatalogState>,
    _admin: RequireAdmin,
    Query(query): Query<CatalogQuery>,
) -> impl IntoResponse {
    let catalog = state.catalog.read().await;
    let mut models: Vec<ModelSpec> = match query.provider.as_deref() {
        Some(p) if !p.trim().is_empty() => {
            catalog.list_by_provider(p).into_iter().cloned().collect()
        }
        _ => catalog.all_models().into_iter().cloned().collect(),
    };

    if let Some(q) = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let q_lower = q.to_lowercase();
        models.retain(|m| {
            m.id.to_lowercase().contains(&q_lower)
                || m.name.to_lowercase().contains(&q_lower)
                || m.provider.to_lowercase().contains(&q_lower)
        });
    }

    let count = models.len();
    Json(CatalogResponse {
        source: catalog.source.clone(),
        updated_at_secs: catalog.updated_at_secs,
        count,
        models,
    })
}

/// `POST /v1/admin/catalog/refresh` — Force-fetch latest catalog from `models.dev` or `HYPERINFER_CATALOG_URL`.
pub async fn refresh_admin_catalog(
    State(state): State<CatalogState>,
    _admin: RequireAdmin,
) -> impl IntoResponse {
    let url = std::env::var("HYPERINFER_CATALOG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CATALOG_URL.to_string());

    let fetched = if url.starts_with("http://") || url.starts_with("https://") {
        ModelCatalog::fetch_from_url(&url).await
    } else {
        tokio::fs::read_to_string(&url)
            .await
            .map_err(|e| format!("Failed to read local catalog '{url}': {e}"))
            .and_then(|raw| ModelCatalog::from_json_auto(&raw))
    };

    match fetched {
        Ok(cat) => {
            if let Some(path) = default_cache_path() {
                if let Some(parent) = path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                if let Ok(serialized) = serde_json::to_string(&cat) {
                    let _ = tokio::fs::write(path, serialized).await;
                }
            }
            let count = cat.len();
            let source = cat.source.clone();
            let updated_at_secs = cat.updated_at_secs;
            *state.catalog.write().await = cat;
            Json(serde_json::json!({
                "status": "refreshed",
                "source": source,
                "updated_at_secs": updated_at_secs,
                "count": count,
            }))
            .into_response()
        }
        Err(err) => (StatusCode::BAD_GATEWAY, err).into_response(),
    }
}

fn resolve_env_key(provider: &str, api_key_ref: Option<&str>) -> Option<String> {
    if let Some(raw) = api_key_ref.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(var) = raw.strip_prefix('$').or_else(|| raw.strip_prefix("env:")) {
            return std::env::var(var).ok().filter(|v| !v.trim().is_empty());
        }
        return Some(raw.to_string());
    }

    let default_env = match provider.to_lowercase().as_str() {
        "openai" => "OPENAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        "gemini" | "google" => "GEMINI_API_KEY",
        _ => return None,
    };
    std::env::var(default_env)
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// `POST /v1/admin/catalog/discover` — Query a provider's live `list_models()` endpoint and enrich with catalog pricing.
pub async fn discover_provider_models(
    State(state): State<CatalogState>,
    _admin: RequireAdmin,
    Json(req): Json<DiscoverModelsRequest>,
) -> impl IntoResponse {
    let provider_name = req.provider.trim().to_lowercase();
    let Some(api_key) = resolve_env_key(&provider_name, req.api_key_ref.as_deref()) else {
        return (
            StatusCode::BAD_REQUEST,
            "No API key available for provider (set provider env var or pass api_key_ref)",
        )
            .into_response();
    };

    if let Some(ref base_url) = req.base_url {
        let allow_localhost = std::env::var("ALLOW_LOCALHOST_PROVIDERS")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        if crate::proxy::validate_base_url(base_url, allow_localhost).is_err() {
            return (StatusCode::BAD_REQUEST, "Invalid or blocked base_url").into_response();
        }
    }

    let discovered: Result<Vec<DiscoveredModel>, _> = match provider_name.as_str() {
        "openai" => {
            let p = match req.base_url.as_deref() {
                Some(url) => OpenAiProvider::with_base_url(url),
                None => OpenAiProvider::new(),
            };
            match p {
                Ok(prov) => prov.list_models(&api_key).await,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            }
        }
        "anthropic" => {
            let p = match req.base_url.as_deref() {
                Some(url) => AnthropicProvider::with_base_url(url),
                None => AnthropicProvider::new(),
            };
            match p {
                Ok(prov) => prov.list_models(&api_key).await,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            }
        }
        "gemini" | "google" => {
            let p = match req.base_url.as_deref() {
                Some(url) => GeminiProvider::with_base_url(url),
                None => GeminiProvider::new(),
            };
            match p {
                Ok(prov) => prov.list_models(&api_key).await,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            }
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "Unsupported provider for live discovery (use openai, anthropic, or gemini)",
            )
                .into_response();
        }
    };

    match discovered {
        Ok(models) => {
            let catalog = state.catalog.read().await;
            let enriched: Vec<EnrichedDiscoveredModel> = models
                .into_iter()
                .map(|m| {
                    let spec = catalog.get(Some(&m.provider), &m.id).cloned();
                    EnrichedDiscoveredModel {
                        discovered: m,
                        catalog_spec: spec,
                    }
                })
                .collect();
            Json(enriched).into_response()
        }
        Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{auth_middleware, create_auth_token};
    use crate::standalone::InMemoryDb;
    use axum::{middleware, routing::get, Router};
    use axum_test::TestServer;
    use hyperinfer_core::{CreateDeploymentRequest, User};

    #[tokio::test]
    async fn test_catalog_routes_filter_and_return_openai_list() {
        let raw = r#"{
            "openai": {
                "models": {
                    "gpt-6": {
                        "id": "gpt-6",
                        "name": "GPT-6",
                        "limit": { "context": 1000000, "output": 128000 },
                        "cost": { "input": 5.0, "output": 25.0 }
                    }
                }
            },
            "anthropic": {
                "models": {
                    "claude-fable-5": {
                        "id": "claude-fable-5",
                        "name": "Claude Fable 5",
                        "limit": { "context": 1000000, "output": 128000 },
                        "cost": { "input": 10.0, "output": 50.0 }
                    }
                }
            }
        }"#;
        let catalog = ModelCatalog::from_models_dev_json(raw).unwrap();
        let state = CatalogState::with_catalog(catalog);
        let db = InMemoryDb::new();
        db.create_deployment(CreateDeploymentRequest {
            name: "openai-gpt6".to_string(),
            provider: "openai".to_string(),
            model: "gpt-6".to_string(),
            api_key_ref: Some("sk-test".to_string()),
            base_url: "https://api.openai.com/v1".to_string(),
            is_active: true,
            weight: 1,
            priority: 1,
            max_tpm: None,
            max_rpm: None,
            cost_per_1k_input_tokens: None,
            cost_per_1k_output_tokens: None,
            metadata: None,
            sort_order: Some(0),
        })
        .await
        .unwrap();
        db.create_deployment(CreateDeploymentRequest {
            name: "local-qwen".to_string(),
            provider: "openai".to_string(),
            model: "qwen3-local-gguf".to_string(),
            api_key_ref: Some("local".to_string()),
            base_url: "http://127.0.0.1:9931".to_string(),
            is_active: true,
            weight: 1,
            priority: 2,
            max_tpm: None,
            max_rpm: None,
            cost_per_1k_input_tokens: None,
            cost_per_1k_output_tokens: None,
            metadata: None,
            sort_order: Some(0),
        })
        .await
        .unwrap();
        db.create_deployment(CreateDeploymentRequest {
            name: "inactive-claude".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-fable-5".to_string(),
            api_key_ref: Some("sk-ant".to_string()),
            base_url: "https://api.anthropic.com".to_string(),
            is_active: false,
            weight: 1,
            priority: 1,
            max_tpm: None,
            max_rpm: None,
            cost_per_1k_input_tokens: None,
            cost_per_1k_output_tokens: None,
            metadata: None,
            sort_order: Some(0),
        })
        .await
        .unwrap();

        let secret = "catalog-test-secret";

        let public_router = Router::new()
            .route("/v1/models", get(list_openai_models::<InMemoryDb>))
            .with_state(ModelsRouteState {
                db,
                catalog: state.clone(),
            });
        let admin_router = Router::new()
            .route("/v1/admin/catalog", get(get_admin_catalog))
            .layer(middleware::from_fn_with_state(
                Arc::new(secret.to_string()),
                auth_middleware,
            ))
            .with_state(state);

        let server = TestServer::new(public_router.merge(admin_router));

        // OpenAI-compatible /v1/models only lists active deployments (including custom models),
        // enriched with catalog specs when available.
        let models_resp = server.get("/v1/models").await;
        models_resp.assert_status_ok();
        let body: serde_json::Value = models_resp.json();
        assert_eq!(body["object"], "list");
        let items = body["data"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], "gpt-6");
        assert_eq!(items[0]["context_window"], 1000000);
        assert_eq!(items[1]["id"], "qwen3-local-gguf");

        // Admin catalog still returns full upstream catalog with provider filter
        let admin_user = User {
            id: "u1".to_string(),
            team_id: "t1".to_string(),
            email: "admin@test.local".to_string(),
            role: "admin".to_string(),
            password_hash: None,
            created_at: chrono::Utc::now(),
        };
        let token = create_auth_token(&admin_user, secret, 3600).unwrap();

        let admin_resp = server
            .get("/v1/admin/catalog?provider=anthropic")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        admin_resp.assert_status_ok();
        let admin_body: serde_json::Value = admin_resp.json();
        assert_eq!(admin_body["count"], 1);
        assert_eq!(admin_body["models"][0]["id"], "claude-fable-5");
    }
}
