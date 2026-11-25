//! HyperInfer Server (Control Plane)

use hyperinfer_core::{Config, ConfigStore, Database, RateLimiter, TelemetryConsumer, UsageRecord};
use hyperinfer_server::{
    build_router,
    mcp::McpState,
    proxy,
    state::{build_cors_layer_from_string, hash_key, key_id, resolve_api_key, AppState, ProdState},
    RedisConfigStore, SqlxDb,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing with log level from RUST_LOG env var (defaults to INFO)
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/hyperinfer".to_string());

    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let max_connections: u32 = std::env::var("DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(20);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let db = SqlxDb::new(pool);

    // Run seeding logic - fail fast if seeding fails
    hyperinfer_server::seeding::run_seeding(&db).await?;

    let config_manager = RedisConfigStore::new(&redis_url).await?;
    let config = config_manager.fetch_config().await.unwrap_or_else(|e| {
        tracing::warn!(
            "Failed to fetch config from Redis, starting with empty config: {:?}",
            e
        );
        Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        }
    });

    let config = Arc::new(RwLock::new(config));
    let _config_subscriber = config_manager
        .subscribe_to_config_updates(config.clone())
        .await?;

    let db_clone = db.clone();
    let telemetry_consumer = TelemetryConsumer::new(&redis_url).await?;
    let cancellation_token = CancellationToken::new();

    // Key-hash cache: hash -> (team_id, api_key_id), bounded at 10,000 entries
    let key_cache: Arc<RwLock<HashMap<String, (String, String)>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let key_cache_clone = key_cache.clone();

    let _telemetry_handle = telemetry_consumer
        .start_consuming_batches(
            move |records: Vec<UsageRecord>| {
                let db = db_clone.clone();
                let cache = key_cache_clone.clone();
                async move {
                    let mut batch = Vec::with_capacity(records.len());
                    let mut cache_w = cache.write().await;

                    for record in records {
                        let key_hash = hash_key(&record.key);
                        let (team_id, api_key_id) = match cache_w.get(&key_hash) {
                            Some(v) => v.clone(),
                            None => match resolve_api_key(&db, &record.key).await {
                                Ok(Some(v)) => {
                                    if cache_w.len() >= 10_000 {
                                        cache_w.clear();
                                    }
                                    cache_w.insert(key_hash, v.clone());
                                    v
                                }
                                Ok(None) => {
                                    tracing::debug!(
                                        "API key not found for key_id: {}, skipping usage record",
                                        key_id(&record.key)
                                    );
                                    continue;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to resolve API key for key_id {}: {:?}",
                                        key_id(&record.key),
                                        e
                                    );
                                    return Err(e.into());
                                }
                            },
                        };

                        batch.push(hyperinfer_core::NewUsageLog {
                            team_id,
                            api_key_id,
                            model: record.model,
                            input_tokens: i32::try_from(record.input_tokens).unwrap_or_else(|_| {
                                tracing::warn!("input_tokens overflow: {}", record.input_tokens);
                                i32::MAX
                            }),
                            output_tokens: i32::try_from(record.output_tokens).unwrap_or_else(
                                |_| {
                                    tracing::warn!(
                                        "output_tokens overflow: {}",
                                        record.output_tokens
                                    );
                                    i32::MAX
                                },
                            ),
                            response_time_ms: i64::try_from(record.response_time_ms)
                                .unwrap_or_else(|_| {
                                    tracing::warn!(
                                        "response_time_ms overflow: {}",
                                        record.response_time_ms
                                    );
                                    i64::MAX
                                }),
                        });
                    }

                    if !batch.is_empty() {
                        match db.record_usage_batch(&batch).await {
                            Ok(_) => {
                                tracing::debug!("Recorded {} usage entries in batch", batch.len());
                            }
                            Err(e) => {
                                tracing::error!("Failed to record usage batch: {:?}", e);
                                return Err(e.into());
                            }
                        }
                    }

                    Ok(())
                }
            },
            cancellation_token,
        )
        .await?;

    let admin_token = match std::env::var("ADMIN_TOKEN") {
        Ok(s) if !s.is_empty() => s,
        _ => return Err("ADMIN_TOKEN must be set to a non-empty value.".into()),
    };

    // JWT secret must be set explicitly.
    let jwt_secret = match std::env::var("MCP_JWT_SECRET") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            return Err("MCP_JWT_SECRET must be set to a non-empty value.".into());
        }
    };
    let jwt_secret_arc = Arc::new(jwt_secret.clone());

    let proxy_router = Arc::new(proxy::ProxyRouter::with_redis(&redis_url).await);
    let rate_limiter = match RateLimiter::new(Some(&redis_url)).await {
        Ok(rl) => Some(Arc::new(rl)),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to initialize Redis RateLimiter; quota enforcement disabled");
            None
        }
    };

    let enable_pii_redaction = std::env::var("ENABLE_PII_REDACTION")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let mut plugins = hyperinfer_core::PluginPipeline::new();
    if enable_pii_redaction {
        plugins.add(Arc::new(hyperinfer_core::RedactionPlugin::with_defaults()));
    }

    let state: ProdState = AppState {
        config,
        db: db.clone(),
        config_manager,
        admin_token: Arc::new(admin_token),
        jwt_secret: jwt_secret_arc.clone(),
        proxy_router: Some(proxy_router),
        rate_limiter,
        plugins,
    };

    let mcp_state = McpState::new(jwt_secret);

    let cors = build_cors_layer_from_string(std::env::var("ALLOWED_ORIGINS").ok().as_deref())?;

    // Swagger UI - disabled by default, set ENABLE_DOCS=true to enable for development
    let enable_docs = std::env::var("ENABLE_DOCS")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let app = build_router(state, mcp_state, cors, enable_docs);

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| {
        let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
        format!("0.0.0.0:{port}")
    });
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
