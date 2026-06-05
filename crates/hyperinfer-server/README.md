# hyperinfer-server

High-performance LLM Gateway server built with Axum.

## Features

- Multi-provider LLM routing (OpenAI, Anthropic, Azure, etc.)
- Rate limiting and request queuing
- API key management and authentication
- PostgreSQL persistence
- Redis caching
- OpenTelemetry observability

## Usage

```rust
use hyperinfer_server::Server;

#[tokio::main]
async fn main() {
    let server = Server::new().await;
    server.run().await;
}
```

## License

MIT
