//! Authentication module for user/password auth with JWT
//!
//! Provides endpoints for:
//! - POST /auth/login - Login with email/password, returns JWT
//! - GET /auth/me - Get current user info from JWT
//! - POST /auth/logout - Logout (client-side token removal)

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use hyperinfer_core::User;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── JWT Claims ───────────────────────────────────────────────────────────────

/// Claims expected inside a user authentication JWT.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthClaims {
    /// User ID (UUID)
    pub sub: String,
    /// User email
    pub email: String,
    /// User role ("admin" or "member")
    pub role: String,
    /// Team ID
    pub team_id: String,
    /// Expiration timestamp
    pub exp: u64,
}

// ── Request/Response Types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub id: String,
    pub email: String,
    pub role: String,
    pub team_id: String,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: String,
    pub email: String,
    pub role: String,
    pub team_id: String,
}

// ── JWT Token Generation ────────────────────────────────────────────────────

pub fn create_auth_token(
    user: &User,
    jwt_secret: &str,
    expires_in_secs: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + expires_in_secs;

    let claims = AuthClaims {
        sub: user.id.clone(),
        email: user.email.clone(),
        role: user.role.clone(),
        team_id: user.team_id.clone(),
        exp,
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
}

pub fn validate_auth_token(
    token: &str,
    jwt_secret: &str,
) -> Result<AuthClaims, jsonwebtoken::errors::Error> {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

    let key = DecodingKey::from_secret(jwt_secret.as_bytes());
    let validation = Validation::new(Algorithm::HS256);
    let data = decode::<AuthClaims>(token, &key, &validation)?;
    Ok(data.claims)
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Extract JWT from Authorization header and validate it.
/// On success, adds AuthClaims to request extensions.
pub async fn auth_middleware(
    State(jwt_secret): State<Arc<String>>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let token = match extract_bearer_token(req.headers()) {
        Some(token) => token,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                "Missing or invalid Authorization header",
            )
                .into_response();
        }
    };

    let claims = match validate_auth_token(&token, &jwt_secret) {
        Ok(claims) => claims,
        Err(e) => {
            tracing::debug!("JWT validation failed: {:?}", e);
            return (StatusCode::UNAUTHORIZED, "Invalid or expired token").into_response();
        }
    };

    req.extensions_mut().insert(claims);
    next.run(req).await
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?;
    let s = value.to_str().ok()?;
    let mut parts = s.splitn(2, char::is_whitespace);
    let scheme = parts.next()?;
    if scheme.eq_ignore_ascii_case("bearer") {
        Some(parts.next()?.trim().to_string())
    } else {
        None
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_create_and_validate_token() {
        let user = User {
            id: "user-123".to_string(),
            team_id: "team-456".to_string(),
            email: "test@example.com".to_string(),
            role: "admin".to_string(),
            password_hash: None,
            created_at: Utc::now(),
        };
        let secret = "test-secret";

        let token = create_auth_token(&user, secret, 3600).unwrap();
        let claims = validate_auth_token(&token, secret).unwrap();

        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.email, user.email);
        assert_eq!(claims.role, user.role);
        assert_eq!(claims.team_id, user.team_id);
    }

    #[test]
    fn test_validate_token_wrong_secret() {
        let user = User {
            id: "user-123".to_string(),
            team_id: "team-456".to_string(),
            email: "test@example.com".to_string(),
            role: "admin".to_string(),
            password_hash: None,
            created_at: Utc::now(),
        };

        let token = create_auth_token(&user, "correct-secret", 3600).unwrap();
        assert!(validate_auth_token(&token, "wrong-secret").is_err());
    }

    #[test]
    fn test_extract_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer test-token"),
        );
        assert_eq!(
            extract_bearer_token(&headers),
            Some("test-token".to_string())
        );

        // Case insensitive
        let mut headers2 = HeaderMap::new();
        headers2.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("bearer test-token"),
        );
        assert_eq!(
            extract_bearer_token(&headers2),
            Some("test-token".to_string())
        );

        // Missing header
        let empty_headers = HeaderMap::new();
        assert_eq!(extract_bearer_token(&empty_headers), None);

        // Wrong scheme
        let mut headers3 = HeaderMap::new();
        headers3.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Basic test-token"),
        );
        assert_eq!(extract_bearer_token(&headers3), None);
    }
}
