use axum::http::Uri;
use axum::response::IntoResponse;

#[cfg(feature = "embedded-frontend")]
use axum::http::header;

#[cfg(feature = "embedded-frontend")]
use rust_embed::RustEmbed;

#[cfg(feature = "embedded-frontend")]
#[derive(RustEmbed)]
#[folder = "../../apps/dashboard/build/"]
struct Frontend;

#[cfg(feature = "embedded-frontend")]
pub async fn spa_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if let Some(content) = Frontend::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response();
    }

    let api_prefixes = ["/v1", "/mcp", "/health"];
    if api_prefixes.iter().any(|prefix| path.starts_with(prefix)) {
        return (axum::http::StatusCode::NOT_FOUND, "Not Found").into_response();
    }

    if let Some(index) = Frontend::get("index.html") {
        return ([(header::CONTENT_TYPE, "text/html")], index.data).into_response();
    }

    (axum::http::StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

#[cfg(not(feature = "embedded-frontend"))]
pub async fn spa_handler(_uri: Uri) -> impl IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        "Dashboard not built. Run: cd apps/dashboard && npm run build, then rebuild with --features embedded-frontend",
    )
        .into_response()
}
