//! Deployment CRUD routes.

use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use hyperinfer_core::{ConfigStore, Database, DbError};
use serde::Deserialize;

use crate::auth::RequireAdmin;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListDeploymentsQuery {
    #[serde(default)]
    pub model: String,
    pub is_active: Option<bool>,
}

pub async fn list_deployments<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
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

pub async fn get_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
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

pub async fn create_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
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
            DbError::UniqueViolation(msg) => (StatusCode::CONFLICT, msg).into_response(),
            DbError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create deployment",
            )
                .into_response(),
        },
    }
}

pub async fn update_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
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
            DbError::UniqueViolation(msg) => (StatusCode::CONFLICT, msg).into_response(),
            DbError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update deployment",
            )
                .into_response(),
        },
    }
}

pub async fn delete_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    _admin: RequireAdmin,
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
