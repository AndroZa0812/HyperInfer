//! API key management routes.

use axum::{
    extract::{Extension, Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use hyperinfer_core::{ConfigStore, Database, DbError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::auth::{AuthClaims, RequireAdmin};
use crate::state::{can_access_team, hash_key, AppState};

#[derive(Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    pub team_id: String,
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct CreateApiKeyResponse {
    #[serde(flatten)]
    pub api_key: hyperinfer_core::ApiKey,
    pub key: String,
}

#[utoipa::path(
    get,
    path = "/v1/api_keys",
    params(("team_id" = Option<String>, Query, description = "Filter by team ID")),
    responses(
        (status = 200, description = "List of API keys"),
        (status = 403, description = "Access denied")
    ),
    tag = "api_keys"
)]
pub async fn list_api_keys<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let team_id = if claims.role == "admin" {
        params.get("team_id").cloned()
    } else {
        Some(claims.team_id.clone())
    };

    match state.db.list_api_keys(team_id).await {
        Ok(keys) => Json(keys).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list API keys").into_response(),
        },
    }
}

#[utoipa::path(
    get,
    path = "/v1/api_keys/{id}",
    params(("id" = String, Path, description = "API Key ID")),
    responses(
        (status = 200, description = "API key found"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "API key not found")
    ),
    tag = "api_keys"
)]
pub async fn get_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(key_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_api_key(&key_id).await {
        Ok(Some(key)) => {
            if !can_access_team(&claims, &key.team_id) {
                return (StatusCode::FORBIDDEN, "Access denied").into_response();
            }
            Json(key).into_response()
        }
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
pub async fn create_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    RequireAdmin(claims): RequireAdmin,
    Json(req): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    let raw_key = req
        .key
        .filter(|k| !k.trim().is_empty())
        .unwrap_or_else(|| format!("hi-{}", uuid::Uuid::new_v4()));
    let user_id = req
        .user_id
        .filter(|u| !u.trim().is_empty())
        .unwrap_or(claims.sub);
    let key_hash = hash_key(&raw_key);
    match state
        .db
        .create_api_key(&key_hash, &user_id, &req.team_id, req.name)
        .await
    {
        Ok(api_key) => Json(CreateApiKeyResponse {
            api_key,
            key: raw_key,
        })
        .into_response(),
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
pub async fn revoke_api_key<D: Database, C: ConfigStore>(
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
