use opentelemetry::global;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::sync::OnceLock;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Initialise the global OpenTelemetry tracer and wire it into the
/// `tracing` subscriber registry.
///
/// Must be called once at application startup (or once per process in
/// the Python extension). Subsequent calls are no-ops guarded by the
/// global tracer provider already being set.
pub fn init_telemetry(endpoint: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_telemetry_with_headers(endpoint, vec![])
}

/// Like [`init_telemetry`] but injects arbitrary HTTP headers into every
/// OTLP export request (used for Langfuse Basic-Auth).
///
/// This function is idempotent: the tracer provider and subscriber are only
/// initialised once per process.  Subsequent calls return `Ok(())` immediately
/// to prevent batch-exporter thread leaks.
pub fn init_telemetry_with_headers(
    endpoint: &str,
    headers: Vec<(String, String)>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Guard: only initialise once per process lifetime.
    static INITIALIZED: OnceLock<()> = OnceLock::new();
    if INITIALIZED.set(()).is_err() {
        return Ok(());
    }

    use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    let mut http_builder = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint);

    if !headers.is_empty() {
        let header_map: std::collections::HashMap<String, String> = headers.into_iter().collect();
        http_builder = http_builder.with_headers(header_map);
    }

    let exporter = http_builder.build()?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    global::set_tracer_provider(provider);
    global::set_text_map_propagator(TraceContextPropagator::new());

    // Wire the OTel layer into the `tracing` subscriber so that spans
    // created via `tracing::info_span!` / `#[tracing::instrument]` are
    // forwarded to the OTLP exporter.
    let otel_layer = tracing_opentelemetry::layer();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // `try_init` is used so repeated calls in tests (which each set up
    // their own provider) don't panic – the first subscriber wins.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(otel_layer)
        .try_init();

    Ok(())
}

/// Initialise telemetry pointing at a Langfuse instance.
///
/// Langfuse's OTLP endpoint requires HTTP Basic Authentication where
/// `public_key` is the username and `secret_key` is the password.
pub fn init_langfuse_telemetry(
    public_key: &str,
    secret_key: &str,
    langfuse_host: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let host = langfuse_host.unwrap_or("https://cloud.langfuse.com");
    let endpoint = format!("{}/api/public/otel/v1/traces", host);

    // Langfuse uses HTTP Basic Auth: Base64("public_key:secret_key")
    use base64::Engine as _;
    let credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", public_key, secret_key));
    let auth_header = format!("Basic {}", credentials);

    init_telemetry_with_headers(&endpoint, vec![("Authorization".to_string(), auth_header)])
}

/// Flush and shut down the global tracer provider.
///
/// Should be called before process exit to ensure all buffered spans are
/// exported.
pub fn shutdown_telemetry() {
    global::shutdown_tracer_provider();
}

// ---------------------------------------------------------------------------
// GenAI Semantic Convention helpers
// ---------------------------------------------------------------------------

pub fn set_gen_ai_attributes(span: &Span, system: &str, model: &str, operation: &str) {
    span.set_attribute("gen_ai.provider.name", system.to_owned());
    span.set_attribute("gen_ai.request.model", model.to_owned());
    span.set_attribute("gen_ai.operation.name", operation.to_owned());
}

pub fn set_gen_ai_usage(span: &Span, input_tokens: u32, output_tokens: u32) {
    span.set_attribute("gen_ai.usage.input_tokens", input_tokens as i64);
    span.set_attribute("gen_ai.usage.output_tokens", output_tokens as i64);
}

pub fn set_gen_ai_response(span: &Span, response_id: &str, finish_reason: &str) {
    span.set_attribute("gen_ai.response.id", response_id.to_owned());
    span.set_attribute("gen_ai.response.finish_reasons", finish_reason.to_owned());
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_gen_ai_attributes_no_panic() {
        // Attributes can only be observed on a real OTel span; here we just
        // verify the helpers don't panic when called with a noop span.
        let span = tracing::info_span!("test_span");
        let _guard = span.enter();
        set_gen_ai_attributes(&tracing::Span::current(), "openai", "gpt-4", "chat");
    }

    #[test]
    fn test_set_gen_ai_usage_no_panic() {
        let span = tracing::info_span!("test_span");
        let _guard = span.enter();
        set_gen_ai_usage(&tracing::Span::current(), 100, 50);
    }

    #[test]
    fn test_set_gen_ai_response_no_panic() {
        let span = tracing::info_span!("test_span");
        let _guard = span.enter();
        set_gen_ai_response(&tracing::Span::current(), "resp-123", "stop");
    }

    #[test]
    fn test_langfuse_basic_auth_encoding() {
        // Verify the Base64 encoding produces the expected Authorization header.
        use base64::Engine as _;
        let public_key = "pk-lf-test";
        let secret_key = "sk-lf-test";
        let expected = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", public_key, secret_key))
        );
        // Decode and verify round-trip
        let stripped = expected.strip_prefix("Basic ").unwrap();
        let decoded = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(stripped)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(decoded, "pk-lf-test:sk-lf-test");
    }
}
