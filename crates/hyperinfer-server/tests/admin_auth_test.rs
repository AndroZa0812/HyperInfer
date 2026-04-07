use axum::{http::StatusCode, middleware, routing::get, Router};
use axum_test::TestServer;
use hyperinfer_server::{admin_auth_middleware, AdminAuthState};
use std::sync::Arc;

async fn test_handler() -> StatusCode {
    StatusCode::OK
}

fn build_app(token: &str) -> Router {
    let state = AdminAuthState {
        admin_token: Arc::new(token.to_string()),
    };
    Router::new()
        .route("/admin/test", get(test_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state)
}

#[tokio::test]
async fn test_admin_auth_missing_token_env() {
    let app = build_app("");
    let server = TestServer::new(app);

    let resp: axum_test::TestResponse = server.get("/admin/test").await;
    assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_auth_missing_header() {
    let app = build_app("supersecret");
    let server = TestServer::new(app);

    let resp: axum_test::TestResponse = server.get("/admin/test").await;
    assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_auth_invalid_header_format() {
    let app = build_app("supersecret");
    let server = TestServer::new(app);

    let resp: axum_test::TestResponse = server
        .get("/admin/test")
        .add_header(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("supersecret"),
        )
        .await;

    assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_auth_wrong_token() {
    let app = build_app("supersecret");
    let server = TestServer::new(app);

    let resp: axum_test::TestResponse = server
        .get("/admin/test")
        .add_header(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer wrongsecret"),
        )
        .await;

    assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_auth_success() {
    let app = build_app("supersecret");
    let server = TestServer::new(app);

    let resp: axum_test::TestResponse = server
        .get("/admin/test")
        .add_header(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer supersecret"),
        )
        .await;

    assert_eq!(resp.status_code(), StatusCode::OK);
}
