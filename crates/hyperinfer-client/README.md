# hyperinfer-client

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-client?style=flat-square)](https://crates.io/crates/hyperinfer-client)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-client?style=flat-square)](https://docs.rs/hyperinfer-client)

The HyperInfer data plane client library. A distributed gateway node that handles direct LLM calls, local routing, caching, rate limiting, telemetry, and traffic mirroring — without proxy latency.

## Features

- **Direct LLM calls** — no proxy hop, sub-millisecond routing overhead
- **Local routing** — model alias resolution and provider inference
- **Response caching** — Redis-backed exact-match cache with configurable TTL
- **Rate limiting** — distributed GCRA token bucket + sliding window counters
- **Telemetry** — usage tracking via Redis Streams, OpenTelemetry, Langfuse
- **Traffic mirroring** — probabilistic shadow requests to secondary models
- **Config sync** — live configuration updates via Redis Pub/Sub

## Usage

```rust
use hyperinfer_client::HyperInferClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = HyperInferClient::new("redis://localhost:6379", "my-team").await?;

    // Non-streaming chat
    let response = client.chat("gpt-4o-mini", "What is Rust?").await?;
    println!("{}", response.choices[0].message.content);

    // Streaming chat
    let mut stream = client.chat_stream("gpt-4o-mini", "Tell me a story").await?;
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk.choices[0].delta.content);
    }

    // Enable traffic mirroring (shadows 10% of requests to claude-3-haiku)
    client.set_mirror("claude-3-haiku", 0.1).await;

    Ok(())
}
```

## Feature Flags

- `openai` — OpenAI provider support (default)
- `anthropic` — Anthropic provider support (default)
- `telemetry` — OpenTelemetry integration
- `cache` — Redis response caching

## License

MIT
