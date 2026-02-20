//! HyperInfer Server (Control Plane)

use axum::{
    Router,
    routing::{get, post},
    extract::{State, Path, Json},
    response::IntoResponse,
    http::StatusCode,
};
use hyperinfer_core::{Config, redis::ConfigManager};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

mod db;
use db::Db;

#[derive(Clone)]
#[allow(dead_code)]
struct AppState {
    config: Arc<RwLock<Config>>,
    db: Db,
    config_manager: Arc<ConfigManager>,
}

async fn config_sync(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    Json(config.clone())
}

async fn get_team(
    State(state): State<AppState>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_team(&team_id).await {
        Ok(Some(team)) => Json(team).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Team not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn create_team(
    State(state): State<AppState>,
    Json(req): Json<CreateTeamRequest>,
) -> impl IntoResponse {
    match state.db.create_team(&req.name, req.budget_cents).await {
        Ok(team) => Json(team).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create team").into_response(),
    }
}

async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_user(&user_id).await {
        Ok(Some(user)) => Json(user).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn create_user(
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> impl IntoResponse {
    match state.db.create_user(&req.team_id, &req.email, &req.role).await {
        Ok(user) => Json(user).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user").into_response(),
    }
}

async fn get_api_key(
    State(state): State<AppState>,
    Path(key_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_api_key(&key_id).await {
        Ok(Some(key)) => Json(key).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "API key not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn create_api_key(
    State(state): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    match state.db.create_api_key(&req.key_hash, &req.user_id, &req.team_id, &req.name).await {
        Ok(key) => Json(key).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create API key").into_response(),
    }
}

async fn get_model_alias(
    State(state): State<AppState>,
    Path(alias_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_model_alias(&alias_id).await {
        Ok(Some(alias)) => Json(alias).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Model alias not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn create_model_alias(
    State(state): State<AppState>,
    Json(req): Json<CreateModelAliasRequest>,
) -> impl IntoResponse {
    match state.db.create_model_alias(&req.team_id, &req.alias, &req.target_model, &req.provider).await {
        Ok(alias) => Json(alias).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create model alias").into_response(),
    }
}

async fn get_quota(
    State(state): State<AppState>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_quota(&team_id).await {
        Ok(Some(quota)) => Json(quota).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Quota not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn create_quota(
    State(state): State<AppState>,
    Json(req): Json<CreateQuotaRequest>,
) -> impl IntoResponse {
    match state.db.create_quota(&req.team_id, req.rpm_limit, req.tpm_limit, req.budget_cents).await {
        Ok(quota) => Json(quota).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create quota").into_response(),
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
    name: String,
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
    budget_cents: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/hyperinfer".to_string());
    
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await?;

    let db = Db::new(pool);
    let config_manager = Arc::new(ConfigManager::new(&redis_url)?);
    let config = config_manager.fetch_config().await.unwrap_or_else(|_| Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
    });

    let state = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager,
    };

    let cors = CorsLayer::permissive();

    let app = Router::new()
        .route("/v1/config/sync", get(config_sync))
        .route("/v1/teams/{id}", get(get_team))
        .route("/v1/teams", post(create_team))
        .route("/v1/users/{id}", get(get_user))
        .route("/v1/users", post(create_user))
        .route("/v1/api_keys/{id}", get(get_api_key))
        .route("/v1/api_keys", post(create_api_key))
        .route("/v1/model_aliases/{id}", get(get_model_alias))
        .route("/v1/model_aliases", post(create_model_alias))
        .route("/v1/quotas/{team_id}", get(get_quota))
        .route("/v1/quotas", post(create_quota))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
