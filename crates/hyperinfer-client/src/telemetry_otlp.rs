use opentelemetry::global;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub fn init_telemetry(endpoint: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use opentelemetry_otlp::WithExportConfig;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    global::set_tracer_provider(provider);
    global::set_text_map_propagator(TraceContextPropagator::new());

    Ok(())
}

pub fn set_gen_ai_attributes(span: &Span, system: &str, model: &str, operation: &str) {
    span.set_attribute("gen_ai.system", system);
    span.set_attribute("gen_ai.request.model", model);
    span.set_attribute("gen_ai.operation.name", operation);
}

pub fn set_gen_ai_usage(span: &Span, input_tokens: u32, output_tokens: u32) {
    span.set_attribute("gen_ai.usage.input_tokens", input_tokens as i64);
    span.set_attribute("gen_ai.usage.output_tokens", output_tokens as i64);
}

pub fn set_gen_ai_response(span: &Span, response_id: &str, finish_reason: &str) {
    span.set_attribute("gen_ai.response.id", response_id);
    span.set_attribute("gen_ai.response.finish_reasons", finish_reason);
}
