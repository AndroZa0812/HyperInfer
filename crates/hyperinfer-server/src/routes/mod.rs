//! HTTP route modules and Axum router construction.

pub mod deployments;
pub mod keys;
pub mod multi_sdk;
pub mod routing;
pub mod teams;
pub mod users;

use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use hyperinfer_core::{ConfigStore, Database};
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::auth::{auth_middleware, LoginRequest, LoginResponse, MeResponse};
use crate::frontend::spa_handler;
use crate::mcp::{jwt_auth_middleware, mcp_message_handler, mcp_sse_handler, McpState};
use crate::state::{admin_auth_middleware, AppState};

pub async fn healthz_handler<D: Database, C: ConfigStore>(
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

#[derive(OpenApi)]
#[openapi(
    paths(
        teams::list_teams,
        teams::get_team,
        teams::create_team,
        users::get_user,
        users::create_user,
        keys::list_api_keys,
        keys::get_api_key,
        keys::create_api_key,
        keys::revoke_api_key,
        teams::get_usage_summary,
        teams::get_model_alias,
        teams::create_model_alias,
        teams::get_quota,
        teams::create_quota,
        users::login_handler,
        users::me_handler,
        users::logout_handler,
        users::change_password_handler,
    ),
    components(schemas(
        teams::CreateTeamRequest,
        users::CreateUserRequest,
        keys::CreateApiKeyRequest,
        teams::CreateModelAliasRequest,
        teams::CreateQuotaRequest,
        users::ChangePasswordRequest,
        LoginRequest,
        LoginResponse,
        MeResponse,
    )),
    tags(
        (name = "teams", description = "Team management"),
        (name = "users", description = "User management"),
        (name = "api_keys", description = "API key management"),
        (name = "usage", description = "Usage statistics"),
        (name = "model_aliases", description = "Model alias management"),
        (name = "quotas", description = "Quota management"),
        (name = "auth", description = "Authentication"),
    ),
    info(
        title = "HyperInfer Control Plane API",
        version = "0.2.1",
        description = "Next-Generation LLM Gateway - Control Plane API",
    )
)]
pub struct ApiDoc;

/// Construct the full HyperInfer control-plane Axum `Router`.
///
/// Exposed from the library crate so integration tests can instantiate the
/// application directly without going through `main.rs`.
pub fn build_router<D, C>(
    state: AppState<D, C>,
    mcp_state: McpState,
    cors: CorsLayer,
    enable_docs: bool,
) -> Router
where
    D: Database + Clone + Send + Sync + 'static,
    C: ConfigStore + Clone + Send + Sync + 'static,
{
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
        .route("/v1/config/sync", get(routing::config_sync))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware::<D, C>,
        ));

    // Dashboard-facing v1 routes protected by JWT auth (role checks via RequireAdmin extractor)
    let v1_jwt_router = Router::new()
        .route(
            "/v1/teams/{id}",
            get(teams::get_team)
                .put(teams::update_team)
                .delete(teams::delete_team),
        )
        .route("/v1/teams", get(teams::list_teams).post(teams::create_team))
        .route(
            "/v1/users/{id}",
            get(users::get_user).delete(users::delete_user),
        )
        .route(
            "/v1/users/{id}/role",
            axum::routing::put(users::update_user_role),
        )
        .route("/v1/users", get(users::list_users).post(users::create_user))
        .route("/v1/api_keys/{id}", get(keys::get_api_key))
        .route("/v1/api_keys/{id}/revoke", post(keys::revoke_api_key))
        .route(
            "/v1/api_keys",
            get(keys::list_api_keys).post(keys::create_api_key),
        )
        .route("/v1/usage/summary", get(teams::get_usage_summary))
        .route(
            "/v1/model_aliases/{id}",
            get(teams::get_model_alias).delete(teams::delete_model_alias),
        )
        .route(
            "/v1/model_aliases",
            get(teams::list_model_aliases).post(teams::create_model_alias),
        )
        .route(
            "/v1/quotas/{team_id}",
            get(teams::get_quota).put(teams::upsert_quota),
        )
        .route("/v1/quotas", post(teams::create_quota))
        .route(
            "/v1/deployments",
            get(deployments::list_deployments).post(deployments::create_deployment),
        )
        .route(
            "/v1/deployments/{id}",
            get(deployments::get_deployment)
                .put(deployments::update_deployment)
                .delete(deployments::delete_deployment),
        )
        .layer(middleware::from_fn_with_state(
            state.jwt_secret.clone(),
            auth_middleware,
        ));

    // Auth routes for user/password authentication
    let auth_public_routes = Router::new()
        .route("/v1/auth/login", post(users::login_handler))
        .route("/healthz", get(healthz_handler));

    let auth_protected_routes = Router::new()
        .route("/v1/auth/me", get(users::me_handler))
        .route("/v1/auth/logout", post(users::logout_handler))
        .route(
            "/v1/auth/change-password",
            post(users::change_password_handler),
        )
        .layer(middleware::from_fn_with_state(
            state.jwt_secret.clone(),
            auth_middleware,
        ));

    // OpenAI-compatible proxy endpoint (public, authenticated via virtual API key)
    let proxy_router = Router::new()
        .route(
            "/v1/chat/completions",
            post(routing::chat_completions_handler),
        )
        .route("/v1/messages", post(multi_sdk::anthropic_messages_handler))
        .route("/v1/responses", post(multi_sdk::openai_responses_handler))
        .route(
            "/v1beta/models/{model_action}",
            post(multi_sdk::gemini_generate_content_handler),
        )
        .with_state(state.clone());

    // Routing config routes - protected by admin token
    let routing_config_routes = Router::new()
        .route(
            "/v1/routing/config",
            get(routing::get_routing_config_handler).put(routing::update_routing_config_handler),
        )
        .route("/v1/routing/health", get(routing::get_routing_health))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware::<D, C>,
        ));

    let mut app = Router::new()
        .merge(config_sync_router)
        .merge(v1_jwt_router)
        .merge(mcp_router)
        .merge(auth_public_routes)
        .merge(auth_protected_routes)
        .merge(proxy_router)
        .merge(routing_config_routes);

    if enable_docs {
        info!("Swagger UI enabled at /docs");
        app = app.merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()));
    }

    app.fallback(spa_handler).layer(cors).with_state(state)
}

#[cfg(test)]
mod tests;
