//! Cross-repo and team shared memory service routes (`/v1/memories`).

use axum::{
    extract::{Extension, Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use hyperinfer_core::{
    ConfigStore, CreateMemoryRequest, Database, DbError, ListMemoriesFilter, Memory,
    MemoryEventAction, MemoryUpdateEvent, SearchMemoriesRequest, UpdateMemoryRequest,
};
use serde::Deserialize;
use tracing::warn;

use crate::auth::AuthClaims;
use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;
const MAX_CONTENT_BYTES: usize = 32 * 1024;
const MAX_TAGS: usize = 32;
const MAX_TAG_LEN: usize = 64;

fn validate_content(content: &str) -> Result<(), &'static str> {
    if content.trim().is_empty() {
        return Err("Memory content must not be empty");
    }
    if content.len() > MAX_CONTENT_BYTES {
        return Err("Memory content exceeds maximum length (32 KiB)");
    }
    Ok(())
}

fn validate_tags(tags: &[String]) -> Result<(), &'static str> {
    if tags.len() > MAX_TAGS {
        return Err("Too many tags (max 32)");
    }
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_TAG_LEN {
            return Err("Each tag must be 1..=64 non-whitespace characters");
        }
    }
    Ok(())
}

fn is_valid_scope(scope: &str) -> bool {
    matches!(scope, "user" | "repo" | "team")
}

fn is_valid_status(status: &str) -> bool {
    matches!(status, "active" | "pending_approval" | "expired")
}

fn is_admin(claims: &AuthClaims) -> bool {
    claims.role == "admin"
}

fn can_read_memory(claims: &AuthClaims, mem: &Memory) -> bool {
    if mem.team_id != claims.team_id {
        return false;
    }
    if mem.scope == "user" && !is_admin(claims) && mem.user_id != claims.sub {
        return false;
    }
    true
}

fn can_mutate_memory(claims: &AuthClaims, mem: &Memory) -> bool {
    if mem.team_id != claims.team_id {
        return false;
    }
    is_admin(claims) || mem.user_id == claims.sub
}

async fn emit_memory_event<C: ConfigStore>(
    config_manager: &C,
    action: MemoryEventAction,
    mem: &Memory,
) {
    let event = MemoryUpdateEvent {
        action,
        memory_id: mem.id.clone(),
        team_id: mem.team_id.clone(),
        user_id: mem.user_id.clone(),
        repo_id: mem.repo_id.clone(),
        scope: mem.scope.clone(),
        timestamp: Utc::now(),
    };
    if let Err(e) = config_manager.publish_memory_event(&event).await {
        warn!("Failed to publish memory invalidation event: {}", e);
    }
}

#[derive(Debug, Deserialize)]
pub struct ListMemoriesQuery {
    pub scope: Option<String>,
    pub repo_id: Option<String>,
    pub tag: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub include_expired: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SearchMemoriesBody {
    pub query: String,
    pub scope: Option<String>,
    pub repo_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMemoryBody {
    pub content: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub repo_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub supersedes_id: Option<String>,
}

fn default_scope() -> String {
    "team".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemoryBody {
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<String>,
}

pub async fn list_memories<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<ListMemoriesQuery>,
) -> impl IntoResponse {
    if let Some(ref s) = query.scope {
        if !is_valid_scope(s) {
            return (StatusCode::BAD_REQUEST, "Invalid scope").into_response();
        }
    }
    if let Some(ref st) = query.status {
        if !is_valid_status(st) {
            return (StatusCode::BAD_REQUEST, "Invalid status").into_response();
        }
    }

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);
    let user_filter = if is_admin(&claims) {
        None
    } else {
        Some(claims.sub.clone())
    };

    let include_expired = query.include_expired || query.status.as_deref() == Some("expired");
    let filter = ListMemoriesFilter {
        team_id: claims.team_id,
        user_id: user_filter,
        scope: query.scope,
        repo_id: query.repo_id,
        tag: query.tag,
        status: query.status,
        include_expired,
        limit,
        offset,
    };

    match state.db.list_memories(filter).await {
        Ok(memories) => Json(memories).into_response(),
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

pub async fn get_memory<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_memory(&id).await {
        Ok(Some(mem)) => {
            if !can_read_memory(&claims, &mem) {
                return (StatusCode::FORBIDDEN, "Access denied").into_response();
            }
            Json(mem).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Memory not found").into_response(),
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid memory ID").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

pub async fn search_memories<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Json(body): Json<SearchMemoriesBody>,
) -> impl IntoResponse {
    if body.query.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "Search query must not be empty").into_response();
    }
    if let Some(ref s) = body.scope {
        if !is_valid_scope(s) {
            return (StatusCode::BAD_REQUEST, "Invalid scope").into_response();
        }
    }

    let limit = body.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let user_filter = if is_admin(&claims) {
        None
    } else {
        Some(claims.sub.clone())
    };

    let req = SearchMemoriesRequest {
        team_id: claims.team_id,
        user_id: user_filter,
        query: body.query,
        scope: body.scope,
        repo_id: body.repo_id,
        tags: body.tags,
        limit,
    };

    match state.db.search_memories(req).await {
        Ok(results) => Json(results).into_response(),
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

pub async fn create_memory<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Json(body): Json<CreateMemoryBody>,
) -> impl IntoResponse {
    if let Err(msg) = validate_content(&body.content) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }
    if let Err(msg) = validate_tags(&body.tags) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }
    if !is_valid_scope(&body.scope) {
        return (StatusCode::BAD_REQUEST, "Invalid scope").into_response();
    }
    if body.scope == "repo"
        && body
            .repo_id
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            "repo_id is required when scope is 'repo'",
        )
            .into_response();
    }

    let status = body.status.unwrap_or_else(|| "active".to_string());
    if !matches!(status.as_str(), "active" | "pending_approval") {
        return (
            StatusCode::BAD_REQUEST,
            "Initial status must be 'active' or 'pending_approval'",
        )
            .into_response();
    }

    // If superseding an existing memory, verify it exists and caller is authorized to supersede it
    if let Some(ref old_id) = body.supersedes_id {
        match state.db.get_memory(old_id).await {
            Ok(Some(existing)) => {
                if !can_mutate_memory(&claims, &existing) {
                    return (StatusCode::FORBIDDEN, "Access denied").into_response();
                }
                if existing.status == "pending_approval" && status == "active" && !is_admin(&claims)
                {
                    return (
                        StatusCode::FORBIDDEN,
                        "Only team admins can supersede a pending memory with an active one",
                    )
                        .into_response();
                }
            }
            Ok(None) => {
                return (StatusCode::NOT_FOUND, "Superseded memory not found").into_response()
            }
            Err(DbError::InvalidUuid) => {
                return (StatusCode::BAD_REQUEST, "Invalid supersedes_id").into_response()
            }
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        }
    }

    let is_supersede = body.supersedes_id.is_some();
    let req = CreateMemoryRequest {
        team_id: claims.team_id,
        user_id: claims.sub,
        repo_id: body.repo_id,
        scope: body.scope,
        content: body.content,
        tags: body.tags,
        status,
        supersedes_id: body.supersedes_id,
    };

    match state.db.create_memory(req).await {
        Ok(mem) => {
            let action = if is_supersede {
                MemoryEventAction::Superseded
            } else {
                MemoryEventAction::Created
            };
            emit_memory_event(&state.config_manager, action, &mem).await;
            (StatusCode::CREATED, Json(mem)).into_response()
        }
        Err(DbError::NotFound) => {
            (StatusCode::NOT_FOUND, "Superseded memory not found").into_response()
        }
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

pub async fn update_memory<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMemoryBody>,
) -> impl IntoResponse {
    if let Some(ref content) = body.content {
        if let Err(msg) = validate_content(content) {
            return (StatusCode::BAD_REQUEST, msg).into_response();
        }
    }
    if let Some(ref tags) = body.tags {
        if let Err(msg) = validate_tags(tags) {
            return (StatusCode::BAD_REQUEST, msg).into_response();
        }
    }
    if let Some(ref st) = body.status {
        if !is_valid_status(st) {
            return (StatusCode::BAD_REQUEST, "Invalid status").into_response();
        }
    }

    let existing = match state.db.get_memory(&id).await {
        Ok(Some(mem)) => mem,
        Ok(None) => return (StatusCode::NOT_FOUND, "Memory not found").into_response(),
        Err(DbError::InvalidUuid) => {
            return (StatusCode::BAD_REQUEST, "Invalid memory ID").into_response()
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };

    if !can_mutate_memory(&claims, &existing) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    // Disallow reactivating a superseded memory so temporal history stays immutable
    if existing.superseded_by.is_some() && body.status.as_deref() == Some("active") {
        return (
            StatusCode::CONFLICT,
            "Cannot reactivate a superseded memory",
        )
            .into_response();
    }

    // Only admins can transition a memory to active from pending_approval or expired
    let is_approval =
        existing.status == "pending_approval" && body.status.as_deref() == Some("active");
    let is_reactivation = existing.status == "expired" && body.status.as_deref() == Some("active");
    if (is_approval || is_reactivation) && !is_admin(&claims) {
        return (
            StatusCode::FORBIDDEN,
            "Only team admins can activate pending or expired memories",
        )
            .into_response();
    }

    let req = UpdateMemoryRequest {
        content: body.content,
        tags: body.tags,
        status: body.status,
    };

    match state.db.update_memory(&id, &claims.team_id, req).await {
        Ok(updated) => {
            let action = if is_approval {
                MemoryEventAction::Approved
            } else if updated.status == "expired" {
                MemoryEventAction::Expired
            } else {
                MemoryEventAction::Updated
            };
            emit_memory_event(&state.config_manager, action, &updated).await;
            Json(updated).into_response()
        }
        Err(DbError::NotFound) => (StatusCode::NOT_FOUND, "Memory not found").into_response(),
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid memory ID").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

pub async fn delete_memory<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let existing = match state.db.get_memory(&id).await {
        Ok(Some(mem)) => mem,
        Ok(None) => return (StatusCode::NOT_FOUND, "Memory not found").into_response(),
        Err(DbError::InvalidUuid) => {
            return (StatusCode::BAD_REQUEST, "Invalid memory ID").into_response()
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };

    if !can_mutate_memory(&claims, &existing) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    match state.db.expire_memory(&id, &claims.team_id).await {
        Ok(expired) => {
            emit_memory_event(&state.config_manager, MemoryEventAction::Expired, &expired).await;
            Json(expired).into_response()
        }
        Err(DbError::NotFound) => (StatusCode::NOT_FOUND, "Memory not found").into_response(),
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid memory ID").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}
