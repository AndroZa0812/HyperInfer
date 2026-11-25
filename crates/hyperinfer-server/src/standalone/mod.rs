pub mod config_store;
pub mod detect;
pub mod memory_db;

pub use config_store::InMemoryConfigStore;
pub use detect::{
    auto_detect_providers, auto_detect_providers_from_env, detected_to_deployments, probe_ollama,
    DetectedProvider,
};
pub use memory_db::InMemoryDb;

use std::sync::Arc;

use hyperinfer_core::{Config, Database, Provider, RateLimiter};
use tokio::sync::RwLock;
use tracing::info;

use crate::proxy::ProxyRouter;
use crate::state::AppState;

pub type StandaloneState = AppState<InMemoryDb, InMemoryConfigStore>;

fn provider_from_string(s: &str) -> Provider {
    match s {
        "openai" => Provider::OpenAI,
        "anthropic" => Provider::Anthropic,
        "gemini" => Provider::Gemini,
        "antigravity" => Provider::Antigravity,
        _ => Provider::Other,
    }
}

pub struct StandaloneBootstrap {
    pub state: StandaloneState,
    pub admin_token: String,
    pub jwt_secret: String,
    pub api_key: String,
    pub admin_email: String,
    pub admin_password: String,
    pub detected_providers: Vec<DetectedProvider>,
}

pub async fn bootstrap_standalone(
    probe_local_ollama: bool,
) -> Result<StandaloneBootstrap, Box<dyn std::error::Error + Send + Sync>> {
    let db = InMemoryDb::new();

    let config = Arc::new(RwLock::new(Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    }));
    let config_manager = InMemoryConfigStore::new(config.clone());

    let team = db
        .create_team("Local Standalone", 100_000)
        .await
        .map_err(|e| format!("Failed to create default team: {e}"))?;
    info!("Created default team: {}", team.name);

    let admin_email =
        std::env::var("INITIAL_ADMIN_EMAIL").unwrap_or_else(|_| "admin@localhost".to_string());
    let admin_password = std::env::var("INITIAL_ADMIN_PASSWORD")
        .ok()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| format!("hi-pass-{}", uuid::Uuid::new_v4().simple()));
    let password_hash = crate::db::hash_password(&admin_password)?;
    let user = db
        .create_user(&team.id, &admin_email, "admin", Some(password_hash))
        .await
        .map_err(|e| format!("Failed to create admin user: {e}"))?;
    info!("Created admin user: {}", user.email);

    let api_key = std::env::var("HYPERINFER_STANDALONE_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| format!("hi-local-{}", uuid::Uuid::new_v4().simple()));
    let key_hash = crate::state::hash_key(&api_key);
    db.create_api_key(
        &key_hash,
        &user.id,
        &team.id,
        Some("standalone-default".to_string()),
    )
    .await
    .map_err(|e| format!("Failed to create API key: {e}"))?;
    info!(
        "Created default API key (id: {})",
        crate::state::key_id(&api_key)
    );

    let detected_providers = auto_detect_providers(probe_local_ollama).await;
    let deployments = detected_to_deployments(&detected_providers);
    for req in deployments {
        db.create_deployment(req)
            .await
            .map_err(|e| format!("Failed to create deployment: {e}"))?;
    }
    if !detected_providers.is_empty() {
        let provider_names: Vec<String> =
            detected_providers.iter().map(|p| p.name.clone()).collect();
        info!("Detected providers: {}", provider_names.join(", "));
        let mut cfg = config.write().await;
        cfg.default_provider = Some(provider_from_string(&detected_providers[0].provider));
    }

    let admin_token = std::env::var("ADMIN_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| format!("hi-admin-{}", uuid::Uuid::new_v4().simple()));
    let jwt_secret = std::env::var("MCP_JWT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("hi-jwt-{}", uuid::Uuid::new_v4().simple()));

    let proxy_router = Arc::new(ProxyRouter::new_standalone().await);
    let rate_limiter = RateLimiter::new(None).await.ok().map(Arc::new);

    let state: StandaloneState = AppState {
        config,
        db,
        config_manager,
        admin_token: Arc::new(admin_token.clone()),
        jwt_secret: Arc::new(jwt_secret.clone()),
        proxy_router: Some(proxy_router),
        rate_limiter,
    };

    Ok(StandaloneBootstrap {
        state,
        admin_token,
        jwt_secret,
        api_key,
        admin_email,
        admin_password,
        detected_providers,
    })
}
