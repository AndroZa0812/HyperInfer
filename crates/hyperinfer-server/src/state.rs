//! Shared application state, authentication middleware, and common helpers.

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use hyperinfer_core::{Config, ConfigStore, Database, DbError, RateLimiter};
use sha2::Digest;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

use crate::auth::AuthClaims;
use crate::db::{RedisConfigStore, SqlxDb};
use crate::proxy;

#[derive(Clone)]
pub struct AppState<D: Database, C: ConfigStore> {
    pub config: Arc<RwLock<Config>>,
    pub db: D,
    #[allow(dead_code)]
    pub config_manager: C,
    pub admin_token: Arc<String>,
    pub jwt_secret: Arc<String>,
    pub proxy_router: Option<Arc<proxy::ProxyRouter>>,
    pub rate_limiter: Option<Arc<RateLimiter>>,
}

pub type ProdState = AppState<SqlxDb, RedisConfigStore>;

pub async fn admin_auth_middleware<
    D: Database + Clone + Send + Sync + 'static,
    C: ConfigStore + Clone + Send + Sync + 'static,
>(
    State(state): State<AppState<D, C>>,
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

pub fn parse_bearer_token(header: &str) -> Option<String> {
    let mut parts = header.splitn(2, char::is_whitespace);
    let scheme = parts.next()?;
    if scheme.eq_ignore_ascii_case("bearer") {
        parts.next()?.trim().to_owned().into()
    } else {
        None
    }
}

pub fn can_access_team(claims: &AuthClaims, target_team_id: &str) -> bool {
    claims.role == "admin" || claims.team_id == target_team_id
}

// ── Helper Functions ─────────────────────────────────────────────────────────

pub use hyperinfer_core::{hash_key, key_id};

pub fn build_cors_layer_from_string(origins_str: Option<&str>) -> Result<CorsLayer, &'static str> {
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
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]))
}

pub async fn resolve_api_key<D: Database>(
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
