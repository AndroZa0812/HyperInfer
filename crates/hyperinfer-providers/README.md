# hyperinfer-providers

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-providers?style=flat-square)](https://crates.io/crates/hyperinfer-providers)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-providers?style=flat-square)](https://docs.rs/hyperinfer-providers)

Modular LLM provider system for HyperInfer — trait definition, thread-safe registry, and built-in OpenAI/Anthropic implementations.

## The Provider Trait

```rust
#[async_trait]
pub trait LlmProvider: DynClone + Send + Sync {
    fn name(&self) -> &'static str;
    fn base_url(&self) -> &str;
    fn supports_streaming(&self) -> bool;
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    async fn stream(&self, request: ChatRequest) -> Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>>;
    async fn health_check(&self) -> Result<()>;
}
```

## Built-in Providers

| Provider | Feature Flag | Status |
|----------|-------------|--------|
| OpenAI | `openai` (default) | Supported |
| Anthropic | `anthropic` (default) | Supported |
| Azure | `azure` | Declared (not yet implemented) |

## Custom Providers

```rust
use std::pin::Pin;
use tokio_stream::Stream;
use hyperinfer_core::{ChatChunk, ChatRequest, ChatResponse, Result};
use hyperinfer_providers::{LlmProvider, ProviderRegistry};

struct MyProvider;

#[async_trait]
impl LlmProvider for MyProvider {
    fn name(&self) -> &'static str { "my-provider" }
    fn base_url(&self) -> &str { "https://api.my-llm.com" }
    fn supports_streaming(&self) -> bool { true }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // Your custom LLM logic here
        todo!()
    }

    async fn stream(&self, request: ChatRequest) -> Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>> {
        todo!()
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }
}

let mut registry = ProviderRegistry::new();
registry.register(MyProvider);
```

## License

MIT
