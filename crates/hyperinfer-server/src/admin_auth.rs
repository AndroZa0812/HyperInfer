use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AdminAuthState {
    pub admin_token: Arc<String>,
}

pub async fn admin_auth_middleware(
    State(state): State<AdminAuthState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if state.admin_token.is_empty() {
        tracing::error!("ADMIN_TOKEN is not configured. Admin endpoints are disabled for security.");
        return (StatusCode::UNAUTHORIZED, "Admin endpoints are disabled").into_response();
    }

    let auth_header = req.headers().get(axum::http::header::AUTHORIZATION);

    let token = match auth_header.and_then(|h| h.to_str().ok()) {
        Some(v) if v.starts_with("Bearer ") => v.trim_start_matches("Bearer "),
        _ => {
            tracing::debug!("Missing or invalid Authorization header format");
            return (StatusCode::UNAUTHORIZED, "Missing or invalid Authorization header").into_response();
        }
    };

    // Constant-time comparison is better for highly sensitive tokens.
    // String equality is used here for simplicity as typical for internal admin tokens.
    if token == state.admin_token.as_str() {
        next.run(req).await
    } else {
        tracing::warn!("Failed admin authentication attempt");
        (StatusCode::UNAUTHORIZED, "Invalid admin token").into_response()
    }
}
