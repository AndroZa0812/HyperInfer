# Providers Guide

HyperInfer's provider system is **modular and extensible**. Every LLM backend (OpenAI, Anthropic, Azure, local models) is implemented as a pluggable provider that conforms to a common interface. This guide covers the built-in providers, how to configure them, and how to add your own.

## 1. What is a Provider?

A **provider** is a Rust trait implementation that translates HyperInfer's generic `ChatRequest` into a vendor-specific API call and back. The router doesn't care *which* provider handles a request — it just calls the standard interface.

### 1.1. The Provider Abstraction

```text
┌──────────────┐       ┌──────────────┐       ┌──────────────┐
│  ChatRequest │──────▶│  LlmProvider │──────▶│  Vendor API  │
└──────────────┘       └──────────────┘       └──────────────┘
                              │
                              ▼
                       ┌──────────────┐
                       │ ChatResponse │
                       └──────────────┘
```

### 1.2. The `LlmProvider` Trait (Summary)

The full trait is documented in the [Custom LLM Providers](custom-llm-providers.md) guide. At a glance:

| Method | Purpose |
| :--- | :--- |
| `name()` | Unique identifier (e.g., `"openai"`). |
| `base_url()` | API endpoint (overrideable for proxies/local models). |
| `supports_streaming()` | Whether the provider can stream responses. |
| `chat()` | Unary request — returns a complete `ChatResponse`. |
| `stream()` | Streaming request — returns a `Stream<ChatChunk>`. |
| `health_check()` | Validates the API key and connectivity. |

---

## 2. Built-in Providers

HyperInfer ships with first-class support for the most popular LLM providers.

### 2.1. Supported Providers

The following providers are currently implemented in the core:

| Provider | Streaming | Function Calling | Vision | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **OpenAI** | ✅ | ✅ | ✅ | Full support for GPT-4o, o1, o3, etc. |
| **Anthropic** | ✅ | ✅ | ✅ | Full support for Claude 3.5/4 family. |

!!! note "Adding More Providers"
    The provider system is designed to be extensible. Additional providers (Azure OpenAI, AWS Bedrock, Google Vertex AI, Ollama, vLLM, etc.) are planned as follow-up work. You can also implement your own by following the [Custom LLM Providers](custom-llm-providers.md) guide.

### 2.2. Configuring a Provider

Providers are configured at the **deployment level** via the control plane. Each deployment specifies which provider to use, the model name, and the API endpoint.

```bash
# Create an OpenAI deployment
curl -X POST http://localhost:8080/v1/deployments \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "gpt-4o-mini-prod",
    "provider": "openai",
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1",
    "api_key_ref": "openai_prod",
    "weight": 1
  }'

# Create an Anthropic deployment
curl -X POST http://localhost:8080/v1/deployments \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "claude-sonnet-prod",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "base_url": "https://api.anthropic.com/v1",
    "api_key_ref": "anthropic_prod",
    "weight": 1
  }'
```

### 2.3. Deployment Fields

The `CreateDeploymentRequest` struct accepts the following fields:

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `name` | `str` | Yes | Unique identifier for this deployment. |
| `provider` | `str` | Yes | Provider name (e.g., `"openai"`, `"anthropic"`). |
| `model` | `str` | Yes | The actual model name passed to the provider. |
| `base_url` | `str` | Yes | The API endpoint URL. |
| `api_key_ref` | `str \| None` | No | Reference to a stored API key. |
| `weight` | `int` | No | Static traffic weight (default: `0`). |
| `priority` | `int` | No | Priority for routing (default: `0`). |
| `max_tpm` | `int \| None` | No | Maximum tokens per minute for this deployment. |
| `max_rpm` | `int \| None` | No | Maximum requests per minute for this deployment. |
| `cost_per_1k_input_tokens` | `float \| None` | No | Cost per 1k input tokens (for cost-based routing). |
| `cost_per_1k_output_tokens` | `float \| None` | No | Cost per 1k output tokens (for cost-based routing). |
| `metadata` | `object \| None` | No | Arbitrary JSON metadata. |

---

## 3. Provider Registry

The `ProviderRegistry` is a thread-safe, in-memory store of all available providers. The router consults it to find the correct provider for each deployment.

### 3.1. Built-in Registration

The built-in providers are registered automatically when the data plane starts. You don't need to do anything special.

### 3.2. Custom Provider Registration (Rust)

If you've implemented a custom provider, you need to register it with the data plane at startup.

```rust
use hyperinfer_providers::ProviderRegistry;

fn main() {
    let registry = ProviderRegistry::new();

    // Register your custom provider (synchronous)
    registry.register(MyCustomProvider::new());

    // ... start the data plane with this registry ...
}
```

!!! warning "Duplicate Registration Panics"
    The `register` method **panics** if a provider with the same name is already registered. Use `register_arc_if_absent` if you need idempotent registration.

---

## 4. Health Checks

Each provider implements a `health_check` method that validates the API key and connectivity to the upstream service. This is used by:

*   **Liveness probes**: The data plane periodically checks provider health.
*   **Routing decisions**: Unhealthy providers are excluded from routing pools.

### How It Works

The `LlmProvider` trait has a default `health_check` implementation that sends a minimal probe request (`"ping"` with `max_tokens=1`) to the upstream API. If the call succeeds, the provider is considered healthy.

```rust
// From the LlmProvider trait:
async fn health_check(&self, api_key: &str) -> Result<(), HyperInferError> {
    let request = ChatRequest {
        model: "health-check-probe".to_string(),
        messages: vec![/* ping message */],
        max_tokens: Some(1),
        // ...
    };
    self.chat(&request, api_key).await?;
    Ok(())
}
```

!!! note "Manual Health Endpoint"
    The control plane does not currently expose a per-deployment health check HTTP endpoint. Health status is tracked internally and reflected in routing decisions automatically.

---

## 5. Custom Providers

!!! info "Deep Dive Available"
    For a complete guide on **implementing your own custom provider** (including the full `LlmProvider` trait, streaming with `Pin<Box<dyn Stream...>>`, and a complete `MockProvider` example), see the dedicated guide:

    **→ [Custom LLM Providers](custom-llm-providers.md)**

---

## 7. Comparison Table: Provider Concepts

| Concept | Description | Where to Configure |
| :--- | :--- | :--- |
| **Provider** | The vendor/implementation (OpenAI, Anthropic, etc.) | Deployment field |
| **Model** | The specific model name (gpt-4o, claude-3-5-sonnet) | Deployment field |
| **API Key** | The credential for the upstream service | Secret store |
| **Base URL** | The API endpoint (for proxies/local models) | Provider config |
| **Health Check** | Validates connectivity | Automatic + manual API |
