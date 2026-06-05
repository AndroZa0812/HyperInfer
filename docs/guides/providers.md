# Providers Guide

HyperInfer's provider system is modular and extensible.

## Built-in Providers

### OpenAI

```rust
use hyperinfer_providers::OpenAiProvider;

let provider = OpenAiProvider::new("sk-...");
```

### Anthropic

```rust
use hyperinfer_providers::AnthropicProvider;

let provider = AnthropicProvider::new("sk-ant-...");
```

## Custom Providers

Implement the `LlmProvider` trait for any LLM API:

```rust
use async_trait::async_trait;
use hyperinfer_providers::{LlmProvider, ProviderRegistry};
use hyperinfer_core::{ChatRequest, ChatResponse};

struct MyProvider;

#[async_trait]
impl LlmProvider for MyProvider {
    fn name(&self) -> &'static str { "my-provider" }
    fn base_url(&self) -> &str { "https://api.my-llm.com" }
    fn supports_streaming(&self) -> bool { true }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // Implement your LLM call here
    }

    async fn stream(&self, request: ChatRequest) -> Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>> {
        // Implement streaming here
    }
}

let mut registry = ProviderRegistry::new();
registry.register(MyProvider);
```
