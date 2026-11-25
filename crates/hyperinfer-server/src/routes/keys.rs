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
    #[serde(default)]
    pub budget_cents: Option<i64>,
    #[serde(default)]
    pub allowed_models: Option<Vec<String>>,
    #[serde(default)]
    pub rpm_limit: Option<i32>,
    #[serde(default)]
    pub tpm_limit: Option<i32>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
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
    if let Some(b) = req.budget_cents {
        if b < 0 {
            return (StatusCode::BAD_REQUEST, "budget_cents must be non-negative").into_response();
        }
    }
    if let Some(rpm) = req.rpm_limit {
        if rpm <= 0 {
            return (StatusCode::BAD_REQUEST, "rpm_limit must be positive").into_response();
        }
    }
    if let Some(tpm) = req.tpm_limit {
        if tpm <= 0 {
            return (StatusCode::BAD_REQUEST, "tpm_limit must be positive").into_response();
        }
    }

    let raw_key = req
        .key
        .filter(|k| !k.trim().is_empty())
        .unwrap_or_else(|| format!("hi-{}", uuid::Uuid::new_v4()));
    let user_id = req
        .user_id
        .filter(|u| !u.trim().is_empty())
        .unwrap_or(claims.sub);
    let key_hash = hash_key(&raw_key);
    let has_limits = req.budget_cents.is_some()
        || req.allowed_models.is_some()
        || req.rpm_limit.is_some()
        || req.tpm_limit.is_some()
        || req.expires_at.is_some();

    match state
        .db
        .create_api_key(&key_hash, &user_id, &req.team_id, req.name)
        .await
    {
        Ok(api_key) => {
            let api_key = if has_limits {
                match state
                    .db
                    .update_api_key_limits(
                        &api_key.id,
                        req.budget_cents,
                        req.allowed_models,
                        req.rpm_limit,
                        req.tpm_limit,
                        req.expires_at,
                    )
                    .await
                {
                    Ok(updated) => updated,
                    Err(_) => {
                        let _ = state.db.deactivate_api_key(&api_key.id).await;
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Failed to apply API key limits",
                        )
                            .into_response();
                    }
                }
            } else {
                api_key
            };
            Json(CreateApiKeyResponse {
                api_key,
                key: raw_key,
            })
            .into_response()
        }
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
