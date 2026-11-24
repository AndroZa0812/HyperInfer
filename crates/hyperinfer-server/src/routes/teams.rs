//! Team, model alias, and quota management routes.

use axum::{
    extract::{Extension, Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use hyperinfer_core::{ConfigStore, Database, DbError};
use serde::Deserialize;
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::auth::{AuthClaims, RequireAdmin};
use crate::state::{can_access_team, AppState};

#[derive(Deserialize, ToSchema)]
pub struct CreateTeamRequest {
    pub name: String,
    pub budget_cents: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateTeamRequest {
    pub name: String,
    pub budget_cents: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct UpsertQuotaRequest {
    pub rpm_limit: i32,
    pub tpm_limit: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateModelAliasRequest {
    pub team_id: String,
    pub alias: String,
    pub target_model: String,
    pub provider: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateQuotaRequest {
    pub team_id: String,
    pub rpm_limit: i32,
    pub tpm_limit: i32,
}

#[utoipa::path(
    get,
    path = "/v1/teams/{id}",
    params(("id" = String, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team found"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Team not found")
    ),
    tag = "teams"
)]
pub async fn get_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    if !can_access_team(&claims, &team_id) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
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
pub async fn create_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Json(req): Json<CreateTeamRequest>,
) -> impl IntoResponse {
    match state.db.create_team(&req.name, req.budget_cents).await {
        Ok(team) => Json(team).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::UniqueViolation(msg) => (StatusCode::CONFLICT, msg).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create team").into_response(),
        },
    }
}

#[utoipa::path(
    get,
    path = "/v1/model_aliases/{id}",
    params(("id" = String, Path, description = "Model alias ID")),
    responses(
        (status = 200, description = "Model alias found"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Model alias not found")
    ),
    tag = "model_aliases"
)]
pub async fn get_model_alias<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(alias_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_model_alias(&alias_id).await {
        Ok(Some(alias)) => {
            if !can_access_team(&claims, &alias.team_id) {
                return (StatusCode::FORBIDDEN, "Access denied").into_response();
            }
            Json(alias).into_response()
        }
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
pub async fn create_model_alias<D: Database, C: ConfigStore>(
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
        (status = 403, description = "Access denied"),
        (status = 404, description = "Quota not found")
    ),
    tag = "quotas"
)]
pub async fn get_quota<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    if !can_access_team(&claims, &team_id) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
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
pub async fn create_quota<D: Database, C: ConfigStore>(
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

#[utoipa::path(
    get,
    path = "/v1/teams",
    responses(
        (status = 200, description = "List of teams"),
        (status = 403, description = "Access denied")
    ),
    tag = "teams"
)]
pub async fn list_teams<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
) -> impl IntoResponse {
    if claims.role == "admin" {
        match state.db.list_teams().await {
            Ok(teams) => Json(teams).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list teams").into_response(),
        }
    } else {
        match state.db.get_team(&claims.team_id).await {
            Ok(Some(team)) => Json(vec![team]).into_response(),
            Ok(None) => (StatusCode::NOT_FOUND, "Team not found").into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get team").into_response(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/v1/usage/summary",
    params(("team_id" = Option<String>, Query, description = "Filter by team ID")),
    responses(
        (status = 200, description = "Usage summary"),
        (status = 403, description = "Access denied")
    ),
    tag = "usage"
)]
pub async fn get_usage_summary<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let team_id = if claims.role == "admin" {
        params.get("team_id").cloned()
    } else {
        Some(claims.team_id.clone())
    };

    match state.db.get_usage_summary(team_id).await {
        Ok(summary) => Json(summary).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get usage summary",
            )
                .into_response(),
        },
    }
}

pub async fn update_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Path(team_id): Path<String>,
    Json(req): Json<UpdateTeamRequest>,
) -> impl IntoResponse {
    let name = req.name.trim();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "Team name is required").into_response();
    }
    if req.budget_cents < 0 {
        return (StatusCode::BAD_REQUEST, "Budget must be non-negative").into_response();
    }

    match state.db.update_team(&team_id, name, req.budget_cents).await {
        Ok(team) => Json(team).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Team not found").into_response(),
            DbError::UniqueViolation(msg) => (StatusCode::CONFLICT, msg).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to update team").into_response(),
        },
    }
}

pub async fn delete_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    RequireAdmin(claims): RequireAdmin,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    if claims.team_id == team_id {
        return (StatusCode::BAD_REQUEST, "Cannot delete your own team").into_response();
    }

    match state.db.delete_team(&team_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Team not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete team").into_response(),
        },
    }
}

pub async fn list_model_aliases<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let team_id = if claims.role == "admin" {
        params
            .get("team_id")
            .cloned()
            .unwrap_or_else(|| claims.team_id.clone())
    } else {
        claims.team_id.clone()
    };

    match state.db.list_model_aliases(&team_id).await {
        Ok(aliases) => Json(aliases).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to list model aliases",
            )
                .into_response(),
        },
    }
}

pub async fn delete_model_alias<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Path(alias_id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_model_alias(&alias_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            DbError::NotFound => (StatusCode::NOT_FOUND, "Model alias not found").into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete model alias",
            )
                .into_response(),
        },
    }
}

pub async fn upsert_quota<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
    Path(team_id): Path<String>,
    Json(req): Json<UpsertQuotaRequest>,
) -> impl IntoResponse {
    if req.rpm_limit <= 0 || req.tpm_limit <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            "RPM and TPM limits must be positive",
        )
            .into_response();
    }

    match state
        .db
        .upsert_quota(&team_id, req.rpm_limit, req.tpm_limit)
        .await
    {
        Ok(quota) => Json(quota).into_response(),
        Err(e) => match e {
            DbError::InvalidUuid => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save quota").into_response(),
        },
    }
}

// ── Deployment Handlers ────────────────────────────────────────────────────
