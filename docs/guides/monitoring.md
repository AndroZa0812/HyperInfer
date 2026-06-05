# Monitoring Guide

HyperInfer supports OpenTelemetry and Langfuse for observability.

## OpenTelemetry

```rust
use hyperinfer_client::telemetry_otlp::init_telemetry;

init_telemetry("my-service", "my-namespace").await?;
```

Enables:
- Traces for LLM calls
- Metrics for token usage and latency
- GenAI semantic conventions

## Langfuse

```rust
use hyperinfer_client::telemetry_otlp::init_langfuse_telemetry;

init_langfuse_telemetry(
    "lf-public-key",
    "lf-secret-key",
    "https://cloud.langfuse.com",
).await?;
```

## Usage Tracking

Usage records are pushed to Redis Streams via XADD. Usage is automatically tracked by the client. Records include: model, tokens, latency, timestamp, team.

## Telemetry Consumer

The server provides a `TelemetryConsumer` that reads usage records from Redis Streams consumer groups with automatic acknowledgment and retry.
