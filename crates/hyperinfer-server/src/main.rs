//! HyperInfer Server (Control Plane)

use axum::{
    body::Body,
    extract::{Extension, Json, Path, Query, State},
    http::{header::SET_COOKIE, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use hyperinfer_core::{Config, ConfigStore, Database, DbError, TelemetryConsumer, UsageRecord};
use hyperinfer_server::{
    auth::{
        auth_middleware, create_auth_token, AuthClaims, LoginRequest, LoginResponse, MeResponse,
        RequireAdmin,
    },
    db::{hash_password, verify_password},
    mcp::{jwt_auth_middleware, mcp_message_handler, mcp_sse_handler, McpState},
    proxy, RedisConfigStore, SqlxDb,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
struct AppState<D: Database, C: ConfigStore> {
    config: Arc<RwLock<Config>>,
    db: D,
    #[allow(dead_code)]
    config_manager: C,
    admin_token: Arc<String>,
    jwt_secret: Arc<String>,
}

type ProdState = AppState<SqlxDb, RedisConfigStore>;

pub(crate) async fn admin_auth_middleware(
    State(state): State<AppState<SqlxDb, RedisConfigStore>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    let expected_token = state.admin_token.as_ref();

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header) => {
            let provided_token = parse_bearer_token(header);
            if let Some(token) = provided_token {
                let digest_provided = sha2::Sha256::digest(token.as_bytes());
                let digest_expected = sha2::Sha256::digest(expected_token.as_bytes());
                let eq = digest_provided.ct_eq(&digest_expected);
                if eq.into() {
                    return Ok(next.run(req).await);
                }
            }
            Err((StatusCode::UNAUTHORIZED, "Unauthorized"))
        }
        None => Err((StatusCode::UNAUTHORIZED, "Unauthorized")),
    }
}

fn parse_bearer_token(header: &str) -> Option<String> {
    let mut parts = header.splitn(2, char::is_whitespace);
    let scheme = parts.next()?;
    if scheme.eq_ignore_ascii_case("bearer") {
        parts.next()?.trim().to_owned().into()
    } else {
        None
    }
}

async fn healthz_handler<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    let db_ok = state.db.ping().await.is_ok();
    let redis_ok = state.config_manager.ping().await.is_ok();

    let status = if db_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(serde_json::json!({
            "status": if db_ok && redis_ok { "ok" } else { "degraded" },
            "database": if db_ok { "ok" } else { "error" },
            "redis": if redis_ok { "ok" } else { "error" }
        })),
    )
}

async fn config_sync<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    Json(config.clone())
}

async fn get_routing_config_handler<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    match state.db.get_routing_config().await {
        Ok(Some(config)) => Json(config).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Routing config not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to get routing config: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get routing config",
            )
                .into_response()
        }
    }
}

async fn update_routing_config_handler<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<UpdateRoutingConfigRequest>,
) -> impl IntoResponse {
    let core_req = hyperinfer_core::UpdateRoutingConfigRequest {
        strategy: req.strategy,
        strategy_params: req.strategy_params,
        fallback_config: req.fallback_config,
        routing_groups: req.routing_groups,
    };
    match state.db.update_routing_config(core_req).await {
        Ok(config) => Json(config).into_response(),
        Err(e) => {
            tracing::error!("Failed to update routing config: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update routing config",
            )
                .into_response()
        }
    }
}

async fn get_routing_health<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    let deployments = state.db.list_deployments("", Some(true)).await;
    let deployment_count = match deployments {
        Ok(d) => d.len(),
        Err(_) => 0,
    };

    let config = state.db.get_routing_config().await;
    let strategy = match config {
        Ok(Some(c)) => c.strategy,
        Ok(None) => "weighted-shuffle".to_string(),
        Err(_) => "unknown".to_string(),
    };

    Json(serde_json::json!({
        "active_deployments": deployment_count,
        "strategy": strategy,
        "status": "healthy",
    }))
    .into_response()
}

#[utoipa::path(
    get,
    path = "/v1/teams/{id}",
    params(("id" = String, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team found"),
        (status = 404, description = "Team not found")
    ),
    tag = "teams"
)]
async fn get_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_team(&team_id).await {
        Ok(Some(team)) => Json(team).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Team not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Team not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

#[utoipa::path(
    post,
    path = "/v1/teams",
    request_body = CreateTeamRequest,
    responses(
        (status = 200, description = "Team created"),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Admin access required"),
        (status = 409, description = "Team name already exists")
    ),
    tag = "teams"
)]
async fn create_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Json(req): Json<CreateTeamRequest>,
) -> impl IntoResponse {
    match state.db.create_team(&req.name, req.budget_cents).await {
        Ok(team) => Json(team).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::UniqueViolation(_) => {
                (StatusCode::CONFLICT, "Resource already exists").into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create team").into_response(),
        },
    }
}

#[utoipa::path(
    get,
    path = "/v1/users/{id}",
    params(("id" = String, Path, description = "User ID")),
    responses(
        (status = 200, description = "User found"),
        (status = 404, description = "User not found")
    ),
    tag = "users"
)]
async fn get_user<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_user(&user_id).await {
        Ok(Some(user)) => Json(user).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "User not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

#[utoipa::path(
    post,
    path = "/v1/users",
    request_body = CreateUserRequest,
    responses(
        (status = 200, description = "User created"),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Admin access required")
    ),
    tag = "users"
)]
async fn create_user<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Json(req): Json<CreateUserRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_user(&req.team_id, &req.email, &req.role, None) // Pass None as no password provided for API-created users
        .await
    {
        Ok(user) => Json(user).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user").into_response(),
        },
    }
}

#[utoipa::path(
    get,
    path = "/v1/api_keys/{id}",
    params(("id" = String, Path, description = "API Key ID")),
    responses(
        (status = 200, description = "API key found"),
        (status = 404, description = "API key not found")
    ),
    tag = "api_keys"
)]
async fn get_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(key_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_api_key(&key_id).await {
        Ok(Some(key)) => Json(key).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "API key not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "API key not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

#[utoipa::path(
    post,
    path = "/v1/api_keys",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 200, description = "API key created"),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Admin access required")
    ),
    tag = "api_keys"
)]
async fn create_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Json(req): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    let key_hash = hash_key(&req.key);
    match state
        .db
        .create_api_key(&key_hash, &req.user_id, &req.team_id, req.name)
        .await
    {
        Ok(key) => Json(key).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create API key",
            )
                .into_response(),
        },
    }
}

#[utoipa::path(
    post,
    path = "/v1/api_keys/{id}/revoke",
    params(("id" = String, Path, description = "API Key ID")),
    responses(
        (status = 200, description = "API key revoked"),
        (status = 404, description = "API key not found"),
        (status = 403, description = "Admin access required")
    ),
    tag = "api_keys"
)]
async fn revoke_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Path(key_id): Path<String>,
) -> impl IntoResponse {
    match state.db.deactivate_api_key(&key_id).await {
        Ok(key) => Json(key).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "API key not found").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to revoke API key",
            )
                .into_response(),
        },
    }
}

#[utoipa::path(
    get,
    path = "/v1/model_aliases/{id}",
    params(("id" = String, Path, description = "Model alias ID")),
    responses(
        (status = 200, description = "Model alias found"),
        (status = 404, description = "Model alias not found")
    ),
    tag = "model_aliases"
)]
async fn get_model_alias<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(alias_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_model_alias(&alias_id).await {
        Ok(Some(alias)) => Json(alias).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Model alias not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Model alias not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

#[utoipa::path(
    post,
    path = "/v1/model_aliases",
    request_body = CreateModelAliasRequest,
    responses(
        (status = 200, description = "Model alias created"),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Admin access required")
    ),
    tag = "model_aliases"
)]
async fn create_model_alias<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Json(req): Json<CreateModelAliasRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_model_alias(&req.team_id, &req.alias, &req.target_model, &req.provider)
        .await
    {
        Ok(alias) => Json(alias).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create model alias",
            )
                .into_response(),
        },
    }
}

#[utoipa::path(
    get,
    path = "/v1/quotas/{team_id}",
    params(("team_id" = String, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Quota found"),
        (status = 404, description = "Quota not found")
    ),
    tag = "quotas"
)]
async fn get_quota<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_quota(&team_id).await {
        Ok(Some(quota)) => Json(quota).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Quota not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Quota not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        },
    }
}

#[utoipa::path(
    post,
    path = "/v1/quotas",
    request_body = CreateQuotaRequest,
    responses(
        (status = 200, description = "Quota created"),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Admin access required")
    ),
    tag = "quotas"
)]
async fn create_quota<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Json(req): Json<CreateQuotaRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_quota(&req.team_id, req.rpm_limit, req.tpm_limit)
        .await
    {
        Ok(quota) => Json(quota).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create quota").into_response(),
        },
    }
}

// ── Deployment Handlers ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListDeploymentsQuery {
    model: String,
    is_active: Option<bool>,
}

async fn list_deployments<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Query(query): Query<ListDeploymentsQuery>,
) -> impl IntoResponse {
    match state
        .db
        .list_deployments(&query.model, query.is_active)
        .await
    {
        Ok(deployments) => Json(deployments).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to list deployments",
            )
                .into_response(),
        },
    }
}

async fn get_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_deployment(&id).await {
        Ok(Some(deployment)) => Json(deployment).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Deployment not found").into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Deployment not found").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get deployment",
            )
                .into_response(),
        },
    }
}

async fn create_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<hyperinfer_core::CreateDeploymentRequest>,
) -> impl IntoResponse {
    if let Err(msg) = req.validate() {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }
    match state.db.create_deployment(req).await {
        Ok(deployment) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": deployment.id,
                "name": deployment.name,
                "provider": deployment.provider,
                "model": deployment.model,
                "base_url": deployment.base_url,
                "is_active": deployment.is_active,
                "weight": deployment.weight,
                "priority": deployment.priority,
                "max_tpm": deployment.max_tpm,
                "max_rpm": deployment.max_rpm,
                "cost_per_1k_input_tokens": deployment.cost_per_1k_input_tokens,
                "cost_per_1k_output_tokens": deployment.cost_per_1k_output_tokens,
                "metadata": deployment.metadata,
                "sort_order": deployment.sort_order,
                "created_at": deployment.created_at,
                "updated_at": deployment.updated_at,
            })),
        )
            .into_response(),
        Err(e) => match e {
            DbError::UniqueViolation(_) => {
                (StatusCode::CONFLICT, "Resource already exists").into_response()
            }
            DbError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create deployment",
            )
                .into_response(),
        },
    }
}

async fn update_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(id): Path<String>,
    Json(req): Json<hyperinfer_core::CreateDeploymentRequest>,
) -> impl IntoResponse {
    if let Err(msg) = req.validate() {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }
    match state.db.update_deployment(&id, req).await {
        Ok(deployment) => Json(deployment).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Deployment not found").into_response(),
            DbError::UniqueViolation(_) => {
                (StatusCode::CONFLICT, "Resource already exists").into_response()
            }
            DbError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update deployment",
            )
                .into_response(),
        },
    }
}

async fn delete_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_deployment(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Deployment not found").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete deployment",
            )
                .into_response(),
        },
    }
}

// ── OpenAI-Compatible Proxy Handler ────────────────────────────────────────

#[derive(Serialize)]
struct ProxyError {
    error: String,
    code: u16,
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self)).into_response()
    }
}

async fn chat_completions_handler<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<hyperinfer_core::ChatRequest>,
) -> Result<Json<serde_json::Value>, ProxyError> {
    // 1. Extract and validate API key
    let api_key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let auth = match proxy::validate_api_key(&state.db, api_key).await {
        Ok(auth) => auth,
        Err(status) => {
            return Err(ProxyError {
                error: "Invalid or missing API key".to_string(),
                code: status,
            })
        }
    };

    // 2. Check team quota
    if let Ok(Some(quota)) = state.db.get_quota(&auth.team_id).await {
        tracing::debug!(
            "Team {} quota: rpm={}, tpm={}",
            auth.team_id,
            quota.rpm_limit,
            quota.tpm_limit
        );
    }

    // 3. Load deployments from database
    let deployments = state
        .db
        .list_deployments(&request.model, Some(true))
        .await
        .map_err(|e| ProxyError {
            error: format!("Failed to load deployments: {}", e),
            code: 500,
        })?;

    if deployments.is_empty() {
        return Err(ProxyError {
            error: format!("No active deployments found for model '{}'", request.model),
            code: 404,
        });
    }

    // 4. Select deployment using routing
    let config = state.config.read().await;
    let selected = proxy::select_deployment(
        &state.db,
        &request,
        &deployments,
        &config.model_aliases,
        Some(&auth),
    )
    .await
    .map_err(|e| ProxyError {
        error: format!("Routing failed: {}", e),
        code: 503,
    })?;

    // 5. Forward request to selected deployment
    let body = proxy::forward_request(
        &request,
        &selected.base_url,
        &selected.api_key,
        &selected.provider,
    )
    .await
    .map_err(|code| ProxyError {
        error: "Upstream request failed".to_string(),
        code,
    })?;

    Ok(Json(body))
}

#[derive(Deserialize, ToSchema)]
struct CreateTeamRequest {
    name: String,
    budget_cents: i64,
}

#[derive(Deserialize, ToSchema)]
struct CreateUserRequest {
    team_id: String,
    email: String,
    role: String,
}

#[derive(Deserialize, ToSchema)]
struct CreateApiKeyRequest {
    key: String,
    user_id: String,
    team_id: String,
    name: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct CreateModelAliasRequest {
    team_id: String,
    alias: String,
    target_model: String,
    provider: String,
}

#[derive(Deserialize, ToSchema)]
struct CreateQuotaRequest {
    team_id: String,
    rpm_limit: i32,
    tpm_limit: i32,
}

#[derive(Deserialize)]
struct UpdateRoutingConfigRequest {
    strategy: Option<String>,
    strategy_params: Option<serde_json::Value>,
    fallback_config: Option<serde_json::Value>,
    routing_groups: Option<serde_json::Value>,
}

#[derive(Deserialize, ToSchema)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

// ── Auth Handlers ────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful, returns JWT cookie"),
        (status = 401, description = "Invalid credentials")
    ),
    tag = "auth"
)]
async fn login_handler(
    State(state): State<ProdState>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    // Find user by email
    let user = match state.db.get_user_by_email(&req.email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            tracing::warn!("Login failed: user not found");
            return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response();
        }
        Err(e) => {
            tracing::error!(error = ?e, "Database error during login");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    // Verify password
    let password_hash = match &user.password_hash {
        Some(hash) => hash,
        None => {
            tracing::warn!(user_id = %user.id, "Login failed: user has no password");
            return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response();
        }
    };

    if !verify_password(&req.password, password_hash) {
        tracing::warn!(user_id = %user.id, "Login failed: password verification failed");
        return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response();
    }

    // Generate JWT token (24 hours expiry)
    let token = match create_auth_token(&user, &state.jwt_secret, 24 * 3600) {
        Ok(token) => token,
        Err(e) => {
            tracing::error!("Failed to create JWT: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let response = LoginResponse {
        id: user.id,
        email: user.email,
        role: user.role,
        team_id: user.team_id,
    };

    (
        [(SET_COOKIE, hyperinfer_server::auth::auth_cookie(&token))],
        Json(response),
    )
        .into_response()
}

#[utoipa::path(
    get,
    path = "/v1/auth/me",
    responses(
        (status = 200, description = "Current user info"),
        (status = 401, description = "Not authenticated")
    ),
    tag = "auth"
)]
async fn me_handler(Extension(claims): Extension<AuthClaims>) -> impl IntoResponse {
    let response = MeResponse {
        id: claims.sub,
        email: claims.email,
        role: claims.role,
        team_id: claims.team_id,
    };

    Json(response).into_response()
}

#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    responses(
        (status = 204, description = "Logged out")
    ),
    tag = "auth"
)]
async fn logout_handler() -> impl IntoResponse {
    (
        [(SET_COOKIE, hyperinfer_server::auth::clear_auth_cookie())],
        StatusCode::NO_CONTENT,
    )
}

#[utoipa::path(
    post,
    path = "/v1/auth/change-password",
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Password changed"),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Current password incorrect")
    ),
    tag = "auth"
)]
async fn change_password_handler(
    State(state): State<ProdState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<ChangePasswordRequest>,
) -> impl IntoResponse {
    if req.new_password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            "New password must be at least 8 characters",
        )
            .into_response();
    }

    let user = match state.db.get_user_by_email(&claims.email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, "User not found").into_response();
        }
        Err(e) => {
            tracing::error!(error = ?e, "Database error during password change");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let current_hash = match &user.password_hash {
        Some(hash) => hash,
        None => {
            return (StatusCode::UNAUTHORIZED, "No password set").into_response();
        }
    };

    if !verify_password(&req.current_password, current_hash) {
        return (StatusCode::UNAUTHORIZED, "Current password is incorrect").into_response();
    }

    let new_hash = match hash_password(&req.new_password) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!(error = ?e, "Failed to hash new password");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    match state.db.update_password_hash(&user.id, &new_hash).await {
        Ok(_) => (StatusCode::NO_CONTENT, "").into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "Failed to update password");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        }
    }
}

// ── Helper Functions ─────────────────────────────────────────────────────────

fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
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

pub(crate) fn build_cors_layer_from_string(
    origins_str: Option<&str>,
) -> Result<CorsLayer, &'static str> {
    let allowed_origins = origins_str.ok_or("ALLOWED_ORIGINS must be set to a non-empty value.")?;

    if allowed_origins.trim().is_empty() {
        return Err("ALLOWED_ORIGINS must contain at least one valid origin.");
    }

    let origins: Vec<_> = allowed_origins
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<axum::http::HeaderValue>().ok())
        .collect();

    if origins.is_empty() {
        return Err("ALLOWED_ORIGINS must contain at least one valid origin.");
    }

    Ok(CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]))
}

async fn resolve_api_key<D: Database>(
    db: &D,
    key: &str,
) -> Result<Option<(String, String)>, DbError> {
    let key_hash = hash_key(key);
    match db.get_api_key_by_hash(&key_hash).await {
        Ok(Some(api_key)) => Ok(Some((api_key.team_id, api_key.id))),
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

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

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
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
    let _telemetry_handle = telemetry_consumer
        .start_consuming(
            move |record: UsageRecord| {
                let db = db_clone.clone();
                async move {
                    match resolve_api_key(&db, &record.key).await {
                        Ok(Some((team_id, api_key_id))) => {
                            match db
                                .record_usage(
                                    &team_id,
                                    &api_key_id,
                                    &record.model,
                                    i32::try_from(record.input_tokens).unwrap_or_else(|_| {
                                        tracing::warn!(
                                            "input_tokens overflow: {}",
                                            record.input_tokens
                                        );
                                        i32::MAX
                                    }),
                                    i32::try_from(record.output_tokens).unwrap_or_else(|_| {
                                        tracing::warn!(
                                            "output_tokens overflow: {}",
                                            record.output_tokens
                                        );
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
                                    tracing::debug!(
                                        "Recorded usage for key_id: {}",
                                        key_id(&record.key)
                                    )
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
                        Ok(None) => {
                            tracing::debug!(
                                "API key not found for key_id: {}, skipping usage record",
                                key_id(&record.key)
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to resolve API key for key_id {}: {:?}",
                                key_id(&record.key),
                                e
                            );
                            return Err(e.into());
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

    let state: ProdState = AppState {
        config,
        db: db.clone(),
        config_manager,
        admin_token: Arc::new(admin_token),
        jwt_secret: jwt_secret_arc.clone(),
    };

    let mcp_state = McpState::new(jwt_secret);

    let cors = build_cors_layer_from_string(std::env::var("ALLOWED_ORIGINS").ok().as_deref())?;

    // MCP routes protected by JWT auth middleware.
    let mcp_router = Router::new()
        .route("/mcp/sse", get(mcp_sse_handler))
        .route("/mcp/message", post(mcp_message_handler))
        .layer(middleware::from_fn_with_state(
            mcp_state.clone(),
            jwt_auth_middleware,
        ))
        .with_state(mcp_state);

    // Internal service-to-service routes protected by static admin token
    let config_sync_router = Router::new()
        .route("/v1/config/sync", get(config_sync))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ));

    // Dashboard-facing v1 routes protected by JWT auth (role checks via RequireAdmin extractor)
    let v1_jwt_router = Router::new()
        .route("/v1/teams/{id}", get(get_team))
        .route("/v1/teams", post(create_team))
        .route("/v1/users/{id}", get(get_user))
        .route("/v1/users", post(create_user))
        .route("/v1/api_keys/{id}", get(get_api_key))
        .route("/v1/api_keys/{id}/revoke", post(revoke_api_key))
        .route("/v1/api_keys", post(create_api_key))
        .route("/v1/model_aliases/{id}", get(get_model_alias))
        .route("/v1/model_aliases", post(create_model_alias))
        .route("/v1/quotas/{team_id}", get(get_quota))
        .route("/v1/quotas", post(create_quota))
        .route(
            "/v1/deployments",
            get(list_deployments).post(create_deployment),
        )
        .route(
            "/v1/deployments/{id}",
            get(get_deployment)
                .put(update_deployment)
                .delete(delete_deployment),
        )
        .layer(middleware::from_fn_with_state(
            state.jwt_secret.clone(),
            auth_middleware,
        ));

    // Auth routes for user/password authentication
    let auth_public_routes = Router::new()
        .route("/v1/auth/login", post(login_handler))
        .route("/healthz", get(healthz_handler));

    let auth_protected_routes = Router::new()
        .route("/v1/auth/me", get(me_handler))
        .route("/v1/auth/logout", post(logout_handler))
        .route("/v1/auth/change-password", post(change_password_handler))
        .layer(middleware::from_fn_with_state(
            state.jwt_secret.clone(),
            auth_middleware,
        ));

    // OpenAI-compatible proxy endpoint (public, no auth required)
    let proxy_router = Router::new()
        .route("/v1/chat/completions", post(chat_completions_handler))
        .with_state(state.clone());

    // Routing config routes - protected by admin token
    let routing_config_routes = Router::new()
        .route(
            "/v1/routing/config",
            get(get_routing_config_handler).put(update_routing_config_handler),
        )
        .route("/v1/routing/health", get(get_routing_health))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ));

    let mut app = Router::new()
        .merge(config_sync_router)
        .merge(v1_jwt_router)
        .merge(mcp_router)
        .merge(auth_public_routes)
        .merge(auth_protected_routes)
        .merge(proxy_router)
        .merge(routing_config_routes);

    // Swagger UI - disabled by default, set ENABLE_DOCS=true to enable for development
    let enable_docs = std::env::var("ENABLE_DOCS")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if enable_docs {
        #[derive(OpenApi)]
        #[openapi(
            paths(
                get_team,
                create_team,
                get_user,
                create_user,
                get_api_key,
                create_api_key,
                revoke_api_key,
                get_model_alias,
                create_model_alias,
                get_quota,
                create_quota,
                login_handler,
                me_handler,
                logout_handler,
                change_password_handler,
            ),
            components(schemas(
                CreateTeamRequest,
                CreateUserRequest,
                CreateApiKeyRequest,
                CreateModelAliasRequest,
                CreateQuotaRequest,
                ChangePasswordRequest,
                LoginRequest,
                LoginResponse,
                MeResponse,
            )),
            tags(
                (name = "teams", description = "Team management"),
                (name = "users", description = "User management"),
                (name = "api_keys", description = "API key management"),
                (name = "model_aliases", description = "Model alias management"),
                (name = "quotas", description = "Quota management"),
                (name = "auth", description = "Authentication"),
            ),
            info(
                title = "HyperInfer Control Plane API",
                version = "0.1.0",
                description = "Next-Generation LLM Gateway - Control Plane API",
            )
        )]
        struct ApiDoc;

        info!("Swagger UI enabled at /docs");
        app = app.merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()));
    }

    let app = app
        .fallback(hyperinfer_server::frontend::spa_handler)
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
            async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, DbError>;
            async fn create_user(&self, team_id: &str, email: &str, role: &str, password_hash: Option<String>) -> Result<User, DbError>;
            async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError>;
            async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DbError>;
            async fn create_api_key(&self, key_hash: &str, user_id: &str, team_id: &str, name: Option<String>) -> Result<ApiKey, DbError>;
            async fn deactivate_api_key(&self, id: &str) -> Result<ApiKey, DbError>;
            async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError>;
            async fn create_model_alias(&self, team_id: &str, alias: &str, target_model: &str, provider: &str) -> Result<ModelAlias, DbError>;
            async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError>;
            async fn create_quota(&self, team_id: &str, rpm_limit: i32, tpm_limit: i32) -> Result<Quota, DbError>;
            async fn record_usage(&self, team_id: &str, api_key_id: &str, model: &str, input_tokens: i32, output_tokens: i32, response_time_ms: i64) -> Result<UsageLog, DbError>;
            async fn count_users_by_role(&self, role: &str) -> Result<i64, DbError>;
            async fn update_password_hash(&self, user_id: &str, password_hash: &str) -> Result<(), DbError>;
            async fn list_deployments(&self, model: &str, is_active: Option<bool>) -> Result<Vec<hyperinfer_core::Deployment>, DbError>;
            async fn get_deployment(&self, id: &str) -> Result<Option<hyperinfer_core::Deployment>, DbError>;
            async fn create_deployment(&self, req: hyperinfer_core::CreateDeploymentRequest) -> Result<hyperinfer_core::Deployment, DbError>;
            async fn update_deployment(&self, id: &str, req: hyperinfer_core::CreateDeploymentRequest) -> Result<hyperinfer_core::Deployment, DbError>;
            async fn delete_deployment(&self, id: &str) -> Result<(), DbError>;
            async fn get_routing_config(&self) -> Result<Option<hyperinfer_core::RoutingConfig>, DbError>;
            async fn update_routing_config(&self, req: hyperinfer_core::UpdateRoutingConfigRequest) -> Result<hyperinfer_core::RoutingConfig, DbError>;
            async fn ping(&self) -> Result<(), DbError>;
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
            async fn ping(&self) -> Result<(), ConfigError>;
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let admin = RequireAdmin(AuthClaims {
            sub: "admin-123".to_string(),
            email: "admin@test.com".to_string(),
            role: "admin".to_string(),
            team_id: "team-123".to_string(),
            exp: 9999999999,
        });

        let response = create_team(
            State(state),
            admin,
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let response = get_team(State(state), Path("error-id".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_hash_key() {
        let key = "test-api-key";
        let hash1 = hash_key(key);
        let hash2 = hash_key(key);

        // Same input should produce same hash
        assert_eq!(hash1, hash2);

        // Hash should be hex string
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));

        // Different inputs should produce different hashes
        let different_hash = hash_key("different-key");
        assert_ne!(hash1, different_hash);
    }

    #[test]
    fn test_hash_key_empty_string() {
        let hash = hash_key("");
        assert!(!hash.is_empty());
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_key_special_characters() {
        let key = "test-key-with-!@#$%^&*()";
        let hash = hash_key(key);
        assert!(!hash.is_empty());
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_key_unicode() {
        let key = "test-key-with-unicode-🔑";
        let hash = hash_key(key);
        assert!(!hash.is_empty());
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_key_long_string() {
        let key = "a".repeat(10000);
        let hash = hash_key(&key);
        assert!(!hash.is_empty());
        // SHA-256 produces 64 hex characters
        assert_eq!(hash.len(), 64);
    }

    #[tokio::test]
    async fn test_resolve_api_key_found() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();
        let api_key = ApiKey {
            id: "key-id".to_string(),
            key_hash: hash_key("test-key"),
            user_id: "user-id".to_string(),
            team_id: "team-id".to_string(),
            name: None,
            is_active: true,
            created_at: now,
            expires_at: None,
        };
        let api_key_clone = api_key.clone();

        db.expect_get_api_key_by_hash()
            .withf(|hash: &str| hash == hash_key("test-key"))
            .times(1)
            .returning(move |_| Ok(Some(api_key_clone.clone())));

        let result = resolve_api_key(&db, "test-key").await;
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.is_some());
        let (team_id, key_id) = resolved.unwrap();
        assert_eq!(team_id, "team-id");
        assert_eq!(key_id, "key-id");
    }

    #[tokio::test]
    async fn test_create_user_success() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();
        let user = User {
            id: "new-user-id".to_string(),
            team_id: "team-id".to_string(),
            email: "new@example.com".to_string(),
            role: "member".to_string(),
            password_hash: None,
            created_at: now,
        };
        db.expect_create_user()
            .with(
                eq("team-id"),
                eq("new@example.com"),
                eq("member"),
                eq(None::<String>),
            )
            .times(1)
            .returning(move |_, _, _, _| Ok(user.clone()));

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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let admin = RequireAdmin(AuthClaims {
            sub: "admin-123".to_string(),
            email: "admin@test.com".to_string(),
            role: "admin".to_string(),
            team_id: "team-id".to_string(),
            exp: 9999999999,
        });

        let response = create_user(
            State(state),
            admin,
            Json(CreateUserRequest {
                team_id: "team-id".to_string(),
                email: "new@example.com".to_string(),
                role: "member".to_string(),
            }),
        )
        .await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_api_key_success() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();

        let expected_hash = hash_key("test-secret-key");

        let api_key = ApiKey {
            id: "new-key-id".to_string(),
            key_hash: expected_hash.clone(),
            user_id: "user-id".to_string(),
            team_id: "team-id".to_string(),
            name: Some("Test Key".to_string()),
            is_active: true,
            created_at: now,
            expires_at: None,
        };
        db.expect_create_api_key()
            .with(
                eq(expected_hash),
                eq("user-id"),
                eq("team-id"),
                eq(Some("Test Key".to_string())),
            )
            .times(1)
            .returning(move |_, _, _, _| Ok(api_key.clone()));

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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let admin = RequireAdmin(AuthClaims {
            sub: "admin-123".to_string(),
            email: "admin@test.com".to_string(),
            role: "admin".to_string(),
            team_id: "team-id".to_string(),
            exp: 9999999999,
        });

        let response = create_api_key(
            State(state),
            admin,
            Json(CreateApiKeyRequest {
                key: "test-secret-key".to_string(),
                user_id: "user-id".to_string(),
                team_id: "team-id".to_string(),
                name: Some("Test Key".to_string()),
            }),
        )
        .await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_model_alias_success() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();
        let alias = ModelAlias {
            id: "alias-id".to_string(),
            team_id: "team-id".to_string(),
            alias: "gpt-4-fast".to_string(),
            target_model: "gpt-4-turbo".to_string(),
            provider: "openai".to_string(),
            created_at: now,
        };
        db.expect_create_model_alias()
            .with(
                eq("team-id"),
                eq("gpt-4-fast"),
                eq("gpt-4-turbo"),
                eq("openai"),
            )
            .times(1)
            .returning(move |_, _, _, _| Ok(alias.clone()));

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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let admin = RequireAdmin(AuthClaims {
            sub: "admin-123".to_string(),
            email: "admin@test.com".to_string(),
            role: "admin".to_string(),
            team_id: "team-id".to_string(),
            exp: 9999999999,
        });

        let response = create_model_alias(
            State(state),
            admin,
            Json(CreateModelAliasRequest {
                team_id: "team-id".to_string(),
                alias: "gpt-4-fast".to_string(),
                target_model: "gpt-4-turbo".to_string(),
                provider: "openai".to_string(),
            }),
        )
        .await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_quota_success() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();
        let quota = Quota {
            id: "quota-id".to_string(),
            team_id: "team-id".to_string(),
            rpm_limit: 100,
            tpm_limit: 10000,
            updated_at: now,
        };
        db.expect_create_quota()
            .with(eq("team-id"), eq(100i32), eq(10000i32))
            .times(1)
            .returning(move |_, _, _| Ok(quota.clone()));

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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let admin = RequireAdmin(AuthClaims {
            sub: "admin-123".to_string(),
            email: "admin@test.com".to_string(),
            role: "admin".to_string(),
            team_id: "team-id".to_string(),
            exp: 9999999999,
        });

        let response = create_quota(
            State(state),
            admin,
            Json(CreateQuotaRequest {
                team_id: "team-id".to_string(),
                rpm_limit: 100,
                tpm_limit: 10000,
            }),
        )
        .await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_team_unique_violation() {
        let mut db = MockDatabase::new();
        db.expect_create_team().times(1).returning(|_, _| {
            Err(DbError::UniqueViolation(
                "Team name already exists".to_string(),
            ))
        });

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
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let admin = RequireAdmin(AuthClaims {
            sub: "admin-123".to_string(),
            email: "admin@test.com".to_string(),
            role: "admin".to_string(),
            team_id: "team-id".to_string(),
            exp: 9999999999,
        });

        let response = create_team(
            State(state),
            admin,
            Json(CreateTeamRequest {
                name: "Duplicate Team".to_string(),
                budget_cents: 5000,
            }),
        )
        .await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_build_cors_layer_from_string() {
        // Test valid origins
        let valid_cors =
            build_cors_layer_from_string(Some("http://localhost:3000,https://example.com"));
        assert!(valid_cors.is_ok());

        // Test None
        let none_cors = build_cors_layer_from_string(None);
        assert!(none_cors.is_err());
        assert_eq!(
            none_cors.unwrap_err(),
            "ALLOWED_ORIGINS must be set to a non-empty value."
        );

        // Test empty string
        let empty_cors = build_cors_layer_from_string(Some(""));
        assert!(empty_cors.is_err());
        assert_eq!(
            empty_cors.unwrap_err(),
            "ALLOWED_ORIGINS must contain at least one valid origin."
        );

        // Test whitespace string
        let whitespace_cors = build_cors_layer_from_string(Some("   "));
        assert!(whitespace_cors.is_err());
        assert_eq!(
            whitespace_cors.unwrap_err(),
            "ALLOWED_ORIGINS must contain at least one valid origin."
        );
    }
}
