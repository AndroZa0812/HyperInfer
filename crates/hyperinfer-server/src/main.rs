//! HyperInfer Server (Control Plane)

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use hyperinfer_core::{Config, ConfigStore, Database, DbError, TelemetryConsumer, UsageRecord};
use hyperinfer_server::{RedisConfigStore, SqlxDb};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

#[derive(Clone)]
struct AppState<D: Database, C: ConfigStore> {
    config: Arc<RwLock<Config>>,
    db: D,
    #[allow(dead_code)]
    config_manager: C,
}

type ProdState = AppState<SqlxDb, RedisConfigStore>;

async fn config_sync<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    Json(config.clone())
}

async fn get_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_team(&team_id).await {
        Ok(Some(team)) => Json(team).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Team not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Team not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

async fn create_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateTeamRequest>,
) -> impl IntoResponse {
    match state.db.create_team(&req.name, req.budget_cents).await {
        Ok(team) => Json(team).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            DbError::UniqueViolation(msg) => (StatusCode::CONFLICT, msg).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create team").into_response(),
        },
    }
}

async fn get_user<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_user(&user_id).await {
        Ok(Some(user)) => Json(user).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "User not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

async fn create_user<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateUserRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_user(&req.team_id, &req.email, &req.role)
        .await
    {
        Ok(user) => Json(user).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user").into_response(),
        },
    }
}

async fn get_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(key_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_api_key(&key_id).await {
        Ok(Some(key)) => Json(key).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "API key not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "API key not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

async fn create_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_api_key(&req.key_hash, &req.user_id, &req.team_id, req.name)
        .await
    {
        Ok(key) => Json(key).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create API key",
            )
                .into_response(),
        },
    }
}

async fn get_model_alias<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(alias_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_model_alias(&alias_id).await {
        Ok(Some(alias)) => Json(alias).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Model alias not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Model alias not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

async fn create_model_alias<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateModelAliasRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_model_alias(&req.team_id, &req.alias, &req.target_model, &req.provider)
        .await
    {
        Ok(alias) => Json(alias).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create model alias",
            )
                .into_response(),
        },
    }
}

async fn get_quota<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_quota(&team_id).await {
        Ok(Some(quota)) => Json(quota).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Quota not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Quota not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

async fn create_quota<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateQuotaRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_quota(&req.team_id, req.rpm_limit, req.tpm_limit)
        .await
    {
        Ok(quota) => Json(quota).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create quota").into_response(),
        },
    }
}

#[derive(Deserialize)]
struct CreateTeamRequest {
    name: String,
    budget_cents: i64,
}

#[derive(Deserialize)]
struct CreateUserRequest {
    team_id: String,
    email: String,
    role: String,
}

#[derive(Deserialize)]
struct CreateApiKeyRequest {
    key_hash: String,
    user_id: String,
    team_id: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct CreateModelAliasRequest {
    team_id: String,
    alias: String,
    target_model: String,
    provider: String,
}

#[derive(Deserialize)]
struct CreateQuotaRequest {
    team_id: String,
    rpm_limit: i32,
    tpm_limit: i32,
}

fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn key_id(key: &str) -> String {
    let hash = hash_key(key);
    // Return last 8 characters of hash
    if hash.len() >= 8 {
        format!("...{}", &hash[hash.len() - 8..])
    } else {
        hash
    }
}

async fn resolve_api_key<D: Database>(db: &D, key: &str) -> Option<(String, String)> {
    let key_hash = hash_key(key);
    match db.get_api_key_by_hash(&key_hash).await {
        Ok(Some(api_key)) => Some((api_key.team_id, api_key.id)),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!("Failed to resolve API key: {:?}", e);
            None
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/hyperinfer".to_string());

    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let db = SqlxDb::new(pool);
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
    let _telemetry_handle = telemetry_consumer
        .start_consuming(move |record: UsageRecord| {
            let db = db_clone.clone();
            async move {
                if let Some((team_id, api_key_id)) = resolve_api_key(&db, &record.key).await {
                    match db
                        .record_usage(
                            &team_id,
                            &api_key_id,
                            &record.model,
                            i32::try_from(record.input_tokens).unwrap_or_else(|_| {
                                tracing::warn!("input_tokens overflow: {}", record.input_tokens);
                                i32::MAX
                            }),
                            i32::try_from(record.output_tokens).unwrap_or_else(|_| {
                                tracing::warn!("output_tokens overflow: {}", record.output_tokens);
                                i32::MAX
                            }),
                            i64::try_from(record.response_time_ms).unwrap_or_else(|_| {
                                tracing::warn!(
                                    "response_time_ms overflow: {}",
                                    record.response_time_ms
                                );
                                i64::MAX
                            }),
                        )
                        .await
                    {
                        Ok(_) => {
                            tracing::debug!("Recorded usage for key_id: {}", key_id(&record.key))
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to record usage for key_id {}: {:?}",
                                key_id(&record.key),
                                e
                            );
                            return Err(e.into());
                        }
                    }
                }
                Ok(())
            }
        })
        .await?;

    let state: ProdState = AppState {
        config,
        db,
        config_manager,
    };

    let cors = {
        let allowed_origins = std::env::var("ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        let origins: Vec<_> = allowed_origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if origins.is_empty() {
            tracing::warn!("No valid CORS origins configured, defaulting to localhost:3000");
            CorsLayer::new().allow_origin(
                "http://localhost:3000"
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
        } else {
            CorsLayer::new().allow_origin(origins)
        }
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE])
    };

    let app = Router::new()
        .route("/v1/config/sync", get(config_sync))
        .route("/v1/teams/:id", get(get_team))
        .route("/v1/teams", post(create_team))
        .route("/v1/users/:id", get(get_user))
        .route("/v1/users", post(create_user))
        .route("/v1/api_keys/:id", get(get_api_key))
        .route("/v1/api_keys", post(create_api_key))
        .route("/v1/model_aliases/:id", get(get_model_alias))
        .route("/v1/model_aliases", post(create_model_alias))
        .route("/v1/quotas/:team_id", get(get_quota))
        .route("/v1/quotas", post(create_quota))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperinfer_core::{
        ApiKey, ConfigError, DbError, ModelAlias, PolicyUpdate, Quota, Team, UsageLog, User,
    };
    use mockall::mock;
    use mockall::predicate::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    mock! {
        pub Database {}

        impl Clone for Database {
            fn clone(&self) -> Self;
        }

        #[async_trait::async_trait]
        impl hyperinfer_core::Database for Database {
            async fn get_team(&self, id: &str) -> Result<Option<Team>, DbError>;
            async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, DbError>;
            async fn get_user(&self, id: &str) -> Result<Option<User>, DbError>;
            async fn create_user(&self, team_id: &str, email: &str, role: &str) -> Result<User, DbError>;
            async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError>;
            async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DbError>;
            async fn create_api_key(&self, key_hash: &str, user_id: &str, team_id: &str, name: Option<String>) -> Result<ApiKey, DbError>;
            async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError>;
            async fn create_model_alias(&self, team_id: &str, alias: &str, target_model: &str, provider: &str) -> Result<ModelAlias, DbError>;
            async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError>;
            async fn create_quota(&self, team_id: &str, rpm_limit: i32, tpm_limit: i32) -> Result<Quota, DbError>;
            async fn record_usage(&self, team_id: &str, api_key_id: &str, model: &str, input_tokens: i32, output_tokens: i32, response_time_ms: i64) -> Result<UsageLog, DbError>;
        }
    }

    mock! {
        pub ConfigStore {}

        impl Clone for ConfigStore {
            fn clone(&self) -> Self;
        }

        #[async_trait::async_trait]
        impl hyperinfer_core::ConfigStore for ConfigStore {
            async fn fetch_config(&self) -> Result<Config, ConfigError>;
            async fn publish_config_update(&self, config: &Config) -> Result<(), ConfigError>;
            async fn publish_policy_update(&self, update: &PolicyUpdate) -> Result<(), ConfigError>;
        }
    }

    fn create_test_state() -> AppState<MockDatabase, MockConfigStore> {
        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        AppState {
            config: Arc::new(RwLock::new(config)),
            db: MockDatabase::new(),
            config_manager: MockConfigStore::new(),
        }
    }

    #[tokio::test]
    async fn test_config_sync() {
        let state = create_test_state();
        let response = config_sync(State(state)).await;
        let json = response.into_response();
        assert_eq!(json.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_team_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_team()
            .with(eq("nonexistent-id"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_team(State(state), Path("nonexistent-id".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_team_found() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();
        let team = Team {
            id: "test-team-id".to_string(),
            name: "Test Team".to_string(),
            budget_cents: 10000,
            created_at: now,
            updated_at: now,
        };
        let team_clone = team.clone();
        db.expect_get_team()
            .with(eq("test-team-id"))
            .times(1)
            .returning(move |_| Ok(Some(team_clone.clone())));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_team(State(state), Path("test-team-id".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_team() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();
        let team = Team {
            id: "new-team-id".to_string(),
            name: "New Team".to_string(),
            budget_cents: 5000,
            created_at: now,
            updated_at: now,
        };
        db.expect_create_team()
            .with(eq("New Team"), eq(5000i64))
            .times(1)
            .returning(move |_, _| Ok(team.clone()));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = create_team(
            State(state),
            Json(CreateTeamRequest {
                name: "New Team".to_string(),
                budget_cents: 5000,
            }),
        )
        .await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_user_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_user()
            .with(eq("nonexistent-user"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_user(State(state), Path("nonexistent-user".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_api_key_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_api_key()
            .with(eq("nonexistent-key"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_api_key(State(state), Path("nonexistent-key".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_model_alias_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_model_alias()
            .with(eq("nonexistent-alias"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_model_alias(State(state), Path("nonexistent-alias".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_quota_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_quota()
            .with(eq("nonexistent-team"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_quota(State(state), Path("nonexistent-team".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_team_database_error() {
        let mut db = MockDatabase::new();
        db.expect_get_team()
            .with(eq("error-id"))
            .times(1)
            .returning(|_| Err(DbError::Sqlx(sqlx::Error::Protocol("test error".into()))));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_team(State(state), Path("error-id".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
