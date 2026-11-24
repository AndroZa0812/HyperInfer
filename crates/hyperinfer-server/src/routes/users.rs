//! User management and authentication routes.

use axum::{
    extract::{Extension, Json, Path, State},
    http::{header::SET_COOKIE, StatusCode},
    response::IntoResponse,
};
use hyperinfer_core::{ConfigStore, Database, DbError};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::auth::{
    create_auth_token, AuthClaims, LoginRequest, LoginResponse, MeResponse, RequireAdmin,
};
use crate::db::{hash_password, verify_password};
use crate::state::{can_access_team, AppState};

#[derive(Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub team_id: String,
    pub email: String,
    pub role: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[utoipa::path(
    get,
    path = "/v1/users/{id}",
    params(("id" = String, Path, description = "User ID")),
    responses(
        (status = 200, description = "User found"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "User not found")
    ),
    tag = "users"
)]
pub async fn get_user<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_user(&user_id).await {
        Ok(Some(user)) => {
            if !can_access_team(&claims, &user.team_id) {
                return (StatusCode::FORBIDDEN, "Access denied").into_response();
            }
            Json(user).into_response()
        }
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
pub async fn create_user<D: Database, C: ConfigStore>(
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
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful, returns JWT cookie"),
        (status = 401, description = "Invalid credentials")
    ),
    tag = "auth"
)]
pub async fn login_handler<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
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
        [(SET_COOKIE, crate::auth::auth_cookie(&token))],
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
pub async fn me_handler(Extension(claims): Extension<AuthClaims>) -> impl IntoResponse {
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
pub async fn logout_handler() -> impl IntoResponse {
    (
        [(SET_COOKIE, crate::auth::clear_auth_cookie())],
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
pub async fn change_password_handler<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
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
