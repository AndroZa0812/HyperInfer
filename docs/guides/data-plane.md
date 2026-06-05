# Data Plane Guide

The data plane client (`hyperinfer-client`) handles direct LLM calls from your application.

## Client Setup

```rust
use hyperinfer_client::HyperInferClient;

let client = HyperInferClient::new("redis://localhost:6379", "my-team").await?;
```

## Chat

```rust
let response = client.chat("gpt-4o-mini", "Hello!").await?;
println!("{}", response.choices[0].message.content);
```

## Streaming

```rust
use futures_util::StreamExt;

let mut stream = client.chat_stream("gpt-4o-mini", "Tell me a story").await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk.choices[0].delta.content);
}
```

## Caching

Caching is enabled by default when Redis is available. Responses are cached by SHA-256 of the canonical request JSON.

The cache TTL defaults to 5 minutes.

## Traffic Mirroring

Mirror a percentage of traffic to a secondary model:

```rust
client.set_mirror("claude-3-haiku", 0.1).await; // mirror 10%
```

Mirror requests are fire-and-forget — they don't affect the primary response.
