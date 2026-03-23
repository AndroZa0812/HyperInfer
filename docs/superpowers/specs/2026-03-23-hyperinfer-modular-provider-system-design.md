# HyperInfer Modular Provider System Design

## Context

HyperInfer currently hardcodes OpenAI and Anthropic providers directly in `hyperinfer-client/src/http_client.rs`. As the system grows, we need:

1. **Modular provider architecture** - Providers as separate, feature-gated modules
2. **Runtime registry** - Dynamic registration of built-in and custom providers
3. **Extensibility** - Users can implement custom providers in Rust or Python
4. **Python binding support** - Python classes must be able to implement providers

This design addresses the provider plugin system for the HyperInfer monorepo.

---

## Architecture Overview

```
┌────────────────────────────────────────────────────────────────────┐
│                         HyperInferClient                           │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │                    ProviderRegistry (Arc)                     │  │
│  │  RwLock<HashMap<String, Arc<dyn LlmProvider>>>               │  │
│  └──────────────────────────────────────────────────────────────┘  │
└───────────────────────────────┬────────────────────────────────────┘
                                │ dyn LlmProvider (Send + Sync)
          ┌─────────────────────┼─────────────────────┐
          ▼                     ▼                     ▼
┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐
│  OpenAI         │   │  Anthropic      │   │  Custom         │
│  Provider       │   │  Provider       │   │  (User Impl)    │
│  (feature-gated)│   │  (feature-gated)│   │                 │
└─────────────────┘   └─────────────────┘   └─────────────────┘
                                              ▲
                                    ┌─────────┴─────────┐
                                    │  Rust            │  Python
                                    │  impl LlmProvider│  PyClass
                                    │  + register()    │  + PyO3
                                    └──────────────────┘
```

---

## Core Trait Definition

### Location
`crates/hyperinfer-providers/src/trait.rs`

### Interface

```rust
use std::pin::Pin;
use std::future::Future;
use futures::Stream;
use hyperinfer_core::{ChatRequest, ChatResponse, ChatChunk, HyperInferError};

/// Primary trait for LLM providers.
///
/// Implementors must be thread-safe (Send + Sync) as the registry
/// may invoke providers concurrently from multiple tokio tasks.
pub trait LlmProvider: Send + Sync {
    /// Returns the provider's unique identifier string.
    /// Examples: "openai", "anthropic", "my-custom-provider"
    fn name(&self) -> &'static str;

    /// Executes a blocking chat completion request.
    fn chat(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, HyperInferError>> + Send + '_>>;

    /// Executes a streaming chat completion request.
    /// Returns a stream of token deltas followed by a final chunk with usage.
    fn stream(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + 'static>>;

    /// Returns true if this provider supports streaming.
    /// Defaults to true. Providers that cannot stream should return false.
    fn supports_streaming(&self) -> bool {
        true
    }

    /// Returns the base URL for this provider's API.
    /// Used for constructing endpoints and health checks.
    /// Examples: "https://api.openai.com", "https://api.anthropic.com"
    fn base_url(&self) -> &'static str {
        ""
    }
}
```

### Design Rationale

| Design Decision | Rationale |
|----------------|-----------|
| `Send + Sync` bounds | Registry may invoke providers from multiple concurrent tasks |
| `Arc<dyn LlmProvider>` in registry | Enables shared ownership without lifetime complexity |
| `&ChatRequest` (borrowed) | Avoid cloning large message structures |
| `Box<dyn Future>` return | Async trait objects; Rust doesn't support `async fn` in traits well yet |
| `Box<dyn Stream>` for streaming | Same reason as above |

---

## Provider Registry

### Location
`crates/hyperinfer-providers/src/registry.rs`

### Interface

```rust
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;

pub struct ProviderRegistry {
    providers: RwLock<HashMap<&'static str, Arc<dyn LlmProvider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a provider instance.
    /// Panics if a provider with the same name is already registered.
    pub fn register<P: LlmProvider + 'static>(&self, provider: P) {
        let name = provider.name();
        let mut providers = self.providers.write();
        if providers.contains_key(name) {
            panic!("Provider '{}' is already registered", name);
        }
        providers.insert(name, Arc::new(provider));
    }

    /// Register a provider from an `Arc`.
    /// Useful for registering providers that are already `Arc`-wrapped.
    pub fn register_arc(&self, name: &'static str, provider: Arc<dyn LlmProvider>) {
        let mut providers = self.providers.write();
        providers.insert(name, provider);
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn LlmProvider>> {
        let providers = self.providers.read();
        providers.get(name).cloned()
    }

    /// List all registered provider names.
    pub fn list(&self) -> Vec<&'static str> {
        let providers = self.providers.read();
        providers.keys().copied().collect()
    }

    /// Check if a provider is registered.
    pub fn contains(&self, name: &str) -> bool {
        let providers = self.providers.read();
        providers.contains_key(name)
    }

    /// Unregister a provider.
    /// Returns the provider if it existed.
    pub fn unregister(&self, name: &str) -> Option<Arc<dyn LlmProvider>> {
        let mut providers = self.providers.write();
        providers.remove(name)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ProviderRegistry {
    fn clone(&self) -> Self {
        Self {
            providers: self.providers.clone(),
        }
    }
}
```

### Design Rationale

| Decision | Rationale |
|----------|-----------|
| `parking_lot::RwLock` | Faster than `std::sync::RwLock` for read-heavy workloads |
| `Arc<dyn LlmProvider>` return | Caller may store the provider; clone is cheap |
| `register<P: LlmProvider>` | Accepts any impl, wraps in Arc internally |
| `register_arc` variant | Allows pre-constructed Arc for FFI/PyO3 cases |
| `&'static str` keys | Avoid heap allocation for provider names |

---

## Built-in Providers

### Location
`crates/hyperinfer-providers/src/`

### Feature Gates

```toml
# crates/hyperinfer-providers/Cargo.toml

[features]
default = ["openai", "anthropic"]  # Both enabled by default
openai = []
anthropic = []
azure = []  # Future
```

### OpenAI Provider

**File:** `crates/hyperinfer-providers/src/openai.rs`

```rust
#[cfg(feature = "openai")]
use reqwest::Client;
#[cfg(feature = "openai")]
use std::sync::Arc;
#[cfg(feature = "openai")]
use hyperinfer_core::{ChatRequest, ChatResponse, ChatChunk, HyperInferError, MessageRole};

#[cfg(feature = "openai")]
#[derive(Clone)]
pub struct OpenAiProvider {
    http_client: Client,
    base_url: &'static str,
}

#[cfg(feature = "openai")]
impl OpenAiProvider {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            base_url: "https://api.openai.com",
        })
    }
}

#[cfg(feature = "openai")]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn base_url(&self) -> &'static str {
        self.base_url
    }

    fn chat(&self, request: &ChatRequest, api_key: &str) -> ... { /* ... */ }
    
    fn stream(&self, request: &ChatRequest, api_key: &str) -> ... { /* ... */ }
}
```

### Anthropic Provider

**File:** `crates/hyperinfer-providers/src/anthropic.rs`

Similar structure with Anthropic-specific API format.

### Provider Registration Helper

**File:** `crates/hyperinfer-providers/src/lib.rs`

```rust
pub mod trait {
    pub use super::trait::LlmProvider;
}

pub mod registry {
    pub use super::registry::ProviderRegistry;
}

#[cfg(feature = "openai")]
pub mod openai {
    pub use super::openai::OpenAiProvider;
}

#[cfg(feature = "anthropic")]
pub mod anthropic {
    pub use super::anthropic::AnthropicProvider;
}

/// Initialize the default registry with built-in providers.
/// Called automatically when using `ProviderRegistry::default()`.
pub fn init_default_registry(registry: &ProviderRegistry) {
    #[cfg(feature = "openai")]
    {
        if let Ok(provider) = OpenAiProvider::new() {
            registry.register(provider);
        }
    }
    
    #[cfg(feature = "anthropic")]
    {
        if let Ok(provider) = AnthropicProvider::new() {
            registry.register(provider);
        }
    }
}
```

---

## Crate Structure

```
crates/hyperinfer-providers/
├── Cargo.toml
└── src/
    ├── lib.rs              # Re-exports + init_default_registry()
    ├── trait.rs            # LlmProvider trait
    ├── registry.rs         # ProviderRegistry
    ├── openai.rs            # OpenAI provider (feature = "openai")
    ├── anthropic.rs         # Anthropic provider (feature = "anthropic")
    ├── azure.rs             # Azure OpenAI (feature = "azure", future)
    └── python.rs            # Python callable wrapper (feature = "python")
```

### Cargo.toml

```toml
[package]
name = "hyperinfer-providers"
version = "0.1.0"
edition = "2021"

[features]
default = ["openai", "anthropic"]
openai = []
anthropic = []
azure = []

[dependencies]
hyperinfer-core = { path = "../hyperinfer-core" }
reqwest = { version = "0.12", features = ["json", "stream"] }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
futures = "0.3"
async-stream = "0.3"
parking-lot = "0.12"

[dev-dependencies]
tokio = { version = "1.0", features = ["macros", "rt-multi-thread"] }
```

---

## Python Binding Integration

### Location
`crates/hyperinfer-python/src/providers.rs`

### PyO3 Wrapper

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use futures::StreamExt;
use parking_lot::RwLock;

/// Wraps a Python callable as an LlmProvider.
pub struct PythonProvider {
    name: String,
    callable: Py<PyAny>,
    streaming_callable: Option<Py<PyAny>>,
}

impl PythonProvider {
    pub fn new(callable: Py<PyAny>) -> PyResult<Self> {
        let name = callable
            .getattr("_provider_name")?
            .extract::<String>()?;

        let streaming_callable = callable
            .getattr("stream", Some(callable.py().none()))
            .ok()
            .filter(|s| !s.is_none())
            .map(|s| s.into_py(callable.py()));

        Ok(Self {
            name,
            callable: callable.into_py(callable.py()),
            streaming_callable,
        })
    }
}

impl LlmProvider for PythonProvider {
    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn supports_streaming(&self) -> bool {
        self.streaming_callable.is_some()
    }

    fn chat(&self, request: &ChatRequest, api_key: &str) -> ... {
        let gil = GILGuard::acquire();
        let py = gil.python();
        
        // Convert ChatRequest to Python dict
        let py_request = Self::chat_request_to_py(py, request)?;
        
        let result = self.callable.call1(py, (py_request, api_key))?;
        // Parse result back to ChatResponse
        Self::py_response_to_chat_response(py, result)
    }

    fn stream(&self, request: &ChatRequest, api_key: &str) -> ... {
        // Similar but returns boxed stream
    }
}

#[pymethods]
impl HyperInferPythonClient {
    /// Register a Python provider class or instance.
    ///
    /// The Python object must implement:
    ///   - `_provider_name: str` class attribute
    ///   - `chat(messages: list[dict], api_key: str) -> dict` method
    ///   - `stream(messages: list[dict], api_key: str) -> Iterator[dict]` method (optional)
    ///
    /// Example:
    ///   ```python
    ///   class MyProvider:
    ///       _provider_name = "my-provider"
    ///
    ///       def chat(self, messages, api_key):
    ///           # ... call custom LLM
    ///           return {"id": "resp-1", "choices": [...], "usage": {...}}
    ///
    ///   client.register_provider(MyProvider())
    ///   ```
    #[pyo3(text_signature = "($self, provider)")]
    pub fn register_provider(&self, provider: Py<PyAny>) -> PyResult<()> {
        let wrapper = PythonProvider::new(provider)?;
        let name = wrapper.name().to_string();
        self.registry.register_arc(
            Box::leak(name.clone().into_boxed_str()),
            Arc::new(wrapper),
        );
        Ok(())
    }

    /// List all registered provider names.
    #[pyo3(text_signature = "($self)")]
    pub fn list_providers(&self) -> Vec<String> {
        self.registry.list().iter().map(|s| s.to_string()).collect()
    }
}
```

### Python Example Usage

```python
import hyperinfer

client = hyperinfer.Client()

class MyCustomProvider:
    _provider_name = "my-custom-llm"
    
    def chat(self, messages, api_key):
        # Call your custom LLM endpoint
        response = my_custom_llm.call(messages, api_key)
        return response
    
    def stream(self, messages, api_key):
        for token in my_custom_llm.stream(messages, api_key):
            yield token

client.register_provider(MyCustomProvider())

# Use it
response = client.chat(
    model="my-custom-llm/gpt-4",  # Format: provider/model
    messages=[{"role": "user", "content": "Hello!"}]
)
```

---

## Integration with hyperinfer-client

### Changes to Client

```rust
// crates/hyperinfer-client/src/lib.rs

pub struct HyperInferClient {
    registry: ProviderRegistry,
    http_caller: Arc<HttpCaller>,
    // ... existing fields
}

impl HyperInferClient {
    /// Create a new client with the default registry (includes built-in providers).
    pub fn new(config: Config) -> Result<Self, HyperInferError> {
        let registry = ProviderRegistry::new();
        hyperinfer_providers::init_default_registry(&registry);
        
        // ... rest of initialization
    }
    
    /// Create a new client with a custom registry.
    /// Use this when you want to provide your own providers or exclude defaults.
    pub fn with_registry(config: Config, registry: ProviderRegistry) -> Result<Self, HyperInferError> {
        // ... initialization with provided registry
    }
    
    /// Access the provider registry for custom registrations.
    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }
}
```

### Router Changes

```rust
// crates/hyperinfer-client/src/router.rs

async fn route_and_execute(
    &self,
    request: &ChatRequest,
    resolved: ResolvedRoute,
) -> Result<ChatResponse, HyperInferError> {
    let provider = self.registry.get(&resolved.provider_name)
        .ok_or_else(|| HyperInferError::Config(
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Provider '{}' not found", resolved.provider_name)
            )
        ))?;
    
    provider.chat(request, &resolved.api_key).await
}
```

---

## Config Schema Changes

### Provider Reference

Provider is now a string identifier instead of an enum:

```rust
// In Config or ChatRequest
pub struct ResolvedRoute {
    pub provider_name: String,  // "openai", "anthropic", "my-custom"
    pub model: String,
    pub api_key: String,
}
```

### Model Alias Resolution

```rust
impl Config {
    /// Resolve a model identifier to provider and model name.
    /// Example: "corporate-gpt4" -> ("openai", "gpt-4o")
    pub fn resolve_model(&self, alias: &str) -> Option<(&str, &str)> {
        if let Some(target) = self.model_aliases.get(alias) {
            // Parse "provider/model" format
            let parts: Vec<&str> = target.split('/').collect();
            if parts.len() == 2 {
                return Some((parts[0], parts[1]));
            }
            // Alias maps directly to a model name; use default provider
            return self.default_provider
                .map(|p| (p.as_str(), target.as_str()));
        }
        // No alias; check if it contains a provider prefix
        if alias.contains('/') {
            let parts: Vec<&str> = alias.split('/').collect();
            if parts.len() == 2 {
                return Some((parts[0], parts[1]));
            }
        }
        None
    }
}
```

---

## Error Handling

### Provider Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Provider '{name}' not found in registry")]
    NotFound { name: String },
    
    #[error("Provider '{name}' does not support streaming")]
    StreamingNotSupported { name: String },
    
    #[error("API request failed: {status} - {message}")]
    ApiError { status: u16, message: String },
    
    #[error("Stream parse error: {message}")]
    StreamParse { message: String },
    
    #[error("Python provider error: {0}")]
    PythonError(String),
}
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    struct MockProvider {
        name: &'static str,
        should_fail: bool,
    }
    
    impl MockProvider {
        fn new(name: &'static str) -> Self {
            Self { name, should_fail: false }
        }
        fn failing() -> Self {
            Self { name: "failing", should_fail: true }
        }
    }
    
    impl LlmProvider for MockProvider {
        fn name(&self) -> &'static str { self.name }
        fn chat(&self, _: &ChatRequest, _: &str) -> ... {
            if self.should_fail {
                Err(HyperInferError::Config(...))
            } else {
                Ok(ChatResponse::default())
            }
        }
        fn stream(&self, _: &ChatRequest, _: &str) -> ... { todo!() }
    }
    
    #[test]
    fn test_registry_register_and_get() {
        let registry = ProviderRegistry::new();
        registry.register(MockProvider::new("test"));
        
        assert!(registry.contains("test"));
        assert!(registry.get("test").is_some());
    }
    
    #[test]
    fn test_registry_list() {
        let registry = ProviderRegistry::new();
        registry.register(MockProvider::new("a"));
        registry.register(MockProvider::new("b"));
        
        let names = registry.list();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }
    
    #[test]
    fn test_registry_duplicate_panics() {
        let registry = ProviderRegistry::new();
        registry.register(MockProvider::new("dup"));
        should_panic!(registry.register(MockProvider::new("dup")));
    }
}
```

### Integration Tests

- Test OpenAI provider with mock server
- Test Anthropic provider with mock server
- Test Python provider wrapper with actual Python code

---

## Migration Plan

### Phase 1: Extract Current Providers

1. Create `hyperinfer-providers` crate structure
2. Move OpenAI implementation from `http_client.rs` to `openai.rs`
3. Move Anthropic implementation to `anthropic.rs`
4. Create `ProviderRegistry` with `init_default_registry()`

### Phase 2: Update Client

1. Update `hyperinfer-client` to depend on `hyperinfer-providers`
2. Refactor `router.rs` to use `registry.get()` instead of match statements
3. Remove `http_client.rs` OpenAI/Anthropic code (keep utility functions)

### Phase 3: Python Bindings

1. Add `python.rs` module with `PythonProvider` wrapper
2. Expose `register_provider()` method on Python client
3. Add tests with actual Python provider implementations

### Phase 4: Documentation & Cleanup

1. Document custom provider development in README
2. Add examples for Rust and Python custom providers
3. Update PROJECT.md roadmap

---

## Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `crates/hyperinfer-providers/` | CREATE | New provider modular crate |
| `crates/hyperinfer-client/src/lib.rs` | MODIFY | Use registry for routing |
| `crates/hyperinfer-client/src/router.rs` | MODIFY | Use registry.get() |
| `crates/hyperinfer-client/src/http_client.rs` | MODIFY | Keep utilities, remove provider impls |
| `crates/hyperinfer-python/src/` | MODIFY | Add Python provider wrapper |
| `crates/hyperinfer-python/src/lib.rs` | MODIFY | Expose register_provider() |
| `Cargo.toml` (workspace) | MODIFY | Add hyperinfer-providers |
| `Cargo.toml` (client) | MODIFY | Add hyperinfer-providers dependency |

---

## Open Questions

1. **Provider priority/fallback**: Should the registry support priority ordering for fallback routing, or is that handled at a higher layer?

2. **Provider health checks**: Should providers expose a `health_check()` method for monitoring?

3. **Credentials per-request**: Currently API keys are in a global config map. Should we support per-provider API keys stored in the registry?

4. **Version compatibility**: How should we handle provider schema changes (e.g., OpenAI adds a new field to ChatRequest)?

These can be addressed in Phase 2+ if needed.
