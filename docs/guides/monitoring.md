# Monitoring & Observability Guide

HyperInfer is designed to be **observable by default**. Every LLM call is traced, every token is counted, and every error is recorded. This guide covers the telemetry stack, metrics, logging, and how to integrate with your existing observability infrastructure.

## 1. The Two Pillars of Observability

HyperInfer currently emits data via two mechanisms:

| Pillar | What You Get | Where It Goes |
| :--- | :--- | :--- |
| **Distributed Traces** | Per-request spans with model, tokens, and latency | OpenTelemetry collector (OTLP) |
| **Structured Logs** | JSON logs via the `tracing` crate | stdout / log aggregators |

!!! note "Metrics Endpoint"
    A Prometheus `/metrics` endpoint is a planned follow-up. Currently, metrics are derived from logs and traces rather than scraped directly.

---

## 2. OpenTelemetry Integration

HyperInfer uses **OpenTelemetry** with OTLP export. The GenAI semantic conventions are followed for span attributes.

### 2.1. Enabling Telemetry (Rust)

The actual signature is:

```rust
use hyperinfer_client::init_telemetry;

// Takes a single endpoint URL
init_telemetry("http://localhost:4318").expect("Failed to init telemetry");
```

For custom headers (e.g., authentication):

```rust
use hyperinfer_client::init_telemetry_with_headers;

init_telemetry_with_headers(
    "https://otel-collector.example.com/v1/traces",
    vec![("Authorization".to_string(), "Bearer my-token".to_string())]
).expect("Failed to init telemetry");
```

### 2.2. Langfuse Integration

```rust
use hyperinfer_client::init_langfuse_telemetry;

init_langfuse_telemetry(
    "lf-public-key",
    "lf-secret-key",
    "https://cloud.langfuse.com",
).expect("Failed to init Langfuse");
```

### 2.3. GenAI Span Attributes

The client library automatically attaches GenAI-convention attributes to every LLM call span:

| Attribute | Description |
| :--- | :--- |
| `gen_ai.system` | Provider name (e.g., `openai`, `anthropic`) |
| `gen_ai.request.model` | The requested model |
| `gen_ai.response.model` | The actual model that responded |
| `gen_ai.usage.input_tokens` | Prompt tokens |
| `gen_ai.usage.output_tokens` | Completion tokens |
| `gen_ai.response.finish_reason` | `stop`, `length`, etc. |

These are set via helper functions:

```rust
use hyperinfer_client::{set_gen_ai_attributes, set_gen_ai_usage, set_gen_ai_response};

let span = tracing::info_span!("chat");
set_gen_ai_attributes(&span, "openai", "gpt-4o", "chat");
set_gen_ai_usage(&span, 100, 50);
set_gen_ai_response(&span, "resp-abc123", "stop");
```

---

## 3. Usage Tracking via Redis Streams

Usage data is pushed to **Redis Streams** via `XADD` for durability and ordered delivery. This allows multiple consumers to process the data independently.

### 3.1. The `TelemetryConsumer`

The control plane includes a `TelemetryConsumer` (in `hyperinfer_core`) that reads usage records from Redis Streams consumer groups. It provides:

*   **Consumer groups** for horizontal scaling.
*   **Automatic acknowledgment** of successfully processed records.
*   **Retry logic** for failed processing.

```rust
use hyperinfer_core::TelemetryConsumer;

let consumer = TelemetryConsumer::new(redis_url, "billing-pipeline").await?;
consumer.run().await?;
```

---

## 4. Structured Logging

HyperInfer emits structured logs via the `tracing` crate.

### 4.1. Log Levels

| Level | When to Use |
| :--- | :--- |
| `ERROR` | Something failed and the request could not complete. |
| `WARN` | Something unexpected happened, but the request succeeded (e.g., a fallback was triggered). |
| `INFO` | Normal operational events (server started, deployment created). |
| `DEBUG` | Detailed diagnostic info (every LLM call, every routing decision). |
| `TRACE` | Extremely verbose (full request/response bodies). |

### 4.2. Enabling Logs

Set the `RUST_LOG` environment variable to control verbosity:

```bash
# Show all info-level logs from hyperinfer crates
export RUST_LOG=hyperinfer=info
cargo run --bin hyperinfer-server

# Debug-level for the router only
export RUST_LOG=hyperinfer_router=debug
cargo run --bin hyperinfer-server
```

### 4.3. Example Log Line

Logs are emitted in a structured format via `tracing_subscriber`:

```text
2026-06-06T12:34:56.789Z  INFO hyperinfer::router: Request routed successfully
    request_id=req_abc123 model=gpt-4o deployment=gpt-4o-prod provider=openai
    latency_ms=342 tokens=150
```

---

## 5. The `Client` Telemetry Lifecycle

The `hyperinfer-client` library starts its own background task on `init()` that:

1.  Connects to Redis.
2.  Subscribes to configuration changes via Redis Pub/Sub.
3.  Pushes usage records to a Redis Stream.
4.  Rebuilds the deployment pool when config changes.

```text
[INFO  hyperinfer_client] Telemetry stream started
[INFO  hyperinfer_client] Connected to Redis at redis://localhost:6379
[INFO  hyperinfer_client] Subscribed to config changes
[INFO  hyperinfer_client] Rebuilt deployment pool after config update
```

This task runs until `close()` is called.

---

## 6. Comparison Table: Observability Features

| Feature | Data Plane (Client) | Control Plane (Server) |
| :--- | :--- | :--- |
| **OpenTelemetry Traces** | Emits per-request spans | N/A (no traces on server) |
| **Structured Logs** | Yes (via `tracing`) | Yes (via `tracing`) |
| **Usage Tracking** | Pushes to Redis Streams | Consumes via `TelemetryConsumer` |
| **Health Endpoints** | N/A (client-side) | `/healthz` (liveness only) |
| **Prometheus Metrics** | Not yet | Not yet (planned) |
