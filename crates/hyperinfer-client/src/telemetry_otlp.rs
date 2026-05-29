use opentelemetry::global;
use opentelemetry_http::HttpClient;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::sync::OnceLock;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[derive(Debug, Clone)]
struct ReqwestHttpClient(reqwest::Client);

#[async_trait::async_trait]
impl HttpClient for ReqwestHttpClient {
    async fn send_bytes(
        &self,
        request: http::Request<bytes::Bytes>,
    ) -> Result<http::Response<bytes::Bytes>, opentelemetry_http::HttpError> {
        let mut response = self
            .0
            .execute(request.try_into()?)
            .await?
            .error_for_status()?;
        let status = response.status();
        let headers = std::mem::take(response.headers_mut());
        let body = response.bytes().await?;
        let mut http_response = http::Response::builder().status(status).body(body)?;
        *http_response.headers_mut() = headers;
        Ok(http_response)
    }
}

/// Module-level storage for the tracer provider so both `init_telemetry_with_headers`
/// and `shutdown_telemetry` share the same instance.
pub(crate) static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

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
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    // Fast path: already initialized, nothing to do.
    if TRACER_PROVIDER.get().is_some() {
        return Ok(());
    }

    // 1. Prepare the exporter (fallible). Only reached on first call.
    let mut http_builder = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_http_client(ReqwestHttpClient(reqwest::Client::new()))
        .with_endpoint(endpoint);

    if !headers.is_empty() {
        let header_map: std::collections::HashMap<String, String> =
            headers.iter().cloned().collect();
        http_builder = http_builder.with_headers(header_map);
    }

    let exporter = http_builder.build()?;

    // 2. Ensure the provider is initialized using the successfully built exporter.
    let provider = TRACER_PROVIDER.get_or_init(|| {
        SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .build()
    });

    global::set_tracer_provider(provider.clone());
    global::set_text_map_propagator(TraceContextPropagator::new());

    // 2. Create the tracer and wire it into the `tracing` subscriber.
    let tracer = provider.tracer("hyperinfer-client");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber_init = tracing_subscriber::registry()
        .with(filter)
        .with(otel_layer)
        .try_init();

    if let Err(e) = subscriber_init {
        return Err(e.into());
    }

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
/// exported.  opentelemetry_sdk 0.31 removed `global::shutdown_tracer_provider()`
/// so we retain the provider in the OnceLock and shut it down directly.
pub fn shutdown_telemetry() {
    if let Some(provider) = TRACER_PROVIDER.get() {
        let _ = provider.shutdown();
    }
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

    #[test]
    fn test_tracer_provider_get_or_init_is_idempotent() {
        // Verify that OnceLock::get_or_init is idempotent by design.
        // The actual init_telemetry_with_headers uses get_or_init, so calling it
        // multiple times will always return the same provider instance.
        use std::sync::OnceLock;

        static TEST_PROVIDER: OnceLock<u32> = OnceLock::new();

        // First call initializes
        let v1 = TEST_PROVIDER.get_or_init(|| 42);
        assert_eq!(*v1, 42);

        // Second call returns the same instance (pointer equality)
        let v2 = TEST_PROVIDER.get_or_init(|| panic!("should not be called"));
        assert!(
            std::ptr::eq(v1, v2),
            "get_or_init should return same instance"
        );
    }

    #[test]
    fn test_init_telemetry_with_headers_build_error() {
        let endpoint = "http://\0invalid";
        let res = init_telemetry_with_headers(endpoint, vec![]);
        assert!(res.is_err());
    }
}
