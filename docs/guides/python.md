# Python Guide

HyperInfer provides first-class Python bindings built directly on top of the high-performance Rust core via **PyO3**. This means you get the safety and speed of Rust without sacrificing the ergonomics of Python.

## 1. Installation

HyperInfer is published on PyPI. We recommend using a virtual environment to avoid conflicts with system packages.

=== "pip"

    ```bash
    # Create and activate a virtual environment (optional but recommended)
    python -m venv .venv
    source .venv/bin/activate

    # Install the core package
    pip install hyperinfer
    ```

=== "uv"

    ```bash
    # Install using uv (extremely fast)
    uv pip install hyperinfer
    ```

=== "poetry"

    ```bash
    poetry add hyperinfer
    ```

!!! info "Python Version Support"
    HyperInfer requires **Python 3.10 or higher**.

---

## 2. Configuration with `Config`

The `Config` class uses a **fluent builder API**. You chain `.with_*` methods to assemble your configuration, then pass it to the `Client`.

### 2.1. Available Builder Methods

| Method | Purpose |
| :--- | :--- |
| `.with_api_key(provider, key)` | Register an API key for a provider (e.g., `"openai"`, `"anthropic"`). |
| `.with_alias(alias, target)` | Map a short alias to a real model name (e.g., `"fast" → "gpt-4o-mini"`). |
| `.with_routing_rule(name, priority, fallbacks)` | Add a routing rule with a priority and list of fallback models. |
| `.with_quota(key, rpm, tpm, budget_cents)` | Set per-key limits for requests/min, tokens/min, and monthly budget. |
| `.with_default_provider(provider)` | Set the provider used when none is specified for a model. |

### 2.2. Example

```python
from hyperinfer import Config

config = (Config()
    .with_api_key("openai", "sk-...")
    .with_api_key("anthropic", "sk-ant-...")
    .with_alias("fast", "gpt-4o-mini")
    .with_routing_rule("default", priority=1, fallbacks=["gpt-4o-mini", "claude-3-haiku-20240307"])
    .with_quota("my-team", rpm=100, tpm=50_000, budget_cents=10_000)
    .with_default_provider("openai")
)
```

!!! tip "Builder Order"
    The builder methods can be chained in any order. They just populate the config dict that's passed to the Rust core at `init()` time.

---

## 3. Client Initialization

The `Client` is the main entry point. It takes a Redis URL and an optional `Config`.

### 3.1. Basic Initialization

```python
from hyperinfer import Client

# Minimal: just provide a Redis URL
client = Client(redis_url="redis://localhost:6379")
```

### 3.2. With Configuration

```python
from hyperinfer import Client, Config

config = (Config()
    .with_api_key("openai", "sk-...")
    .with_alias("fast", "gpt-4o-mini")
)

client = Client(redis_url="redis://localhost:6379", config=config)
await client.init()
```

### 3.3. Async Context Manager

Always use the `async with` syntax to ensure the client is properly initialized and cleaned up.

```python
async def main():
    async with Client(redis_url="redis://localhost:6379", config=config) as client:
        response = await client.chat(
            key="my-team-key",
            model="fast",
            messages=[{"role": "user", "content": "Hello!"}]
        )
        print(response)
```

### 3.4. `Client` Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `redis_url` | `str` | `"redis://localhost:6379"` | Connection string for the Redis state store. |
| `config` | `Config \| None` | `None` | A pre-built `Config` object. |

---

## 4. Making Chat Requests

### 4.1. Unary (Non-Streaming) Chat

Use `client.chat` when you need the complete response before proceeding.

```python
response = await client.chat(
    key="my-team-key",
    model="gpt-4o",
    messages=[
        {"role": "system", "content": "You are a helpful assistant."},
        {"role": "user", "content": "Explain quantum entanglement in 3 sentences."}
    ],
    temperature=0.7,
    max_tokens=150,
    stop=["\n\n"]  # Stop generation at double newline
)
```

The response is a `dict` with the standard OpenAI-style shape:

```python
{
    "id": "chatcmpl-abc123",
    "model": "gpt-4o",
    "choices": [
        {
            "index": 0,
            "message": {"role": "assistant", "content": "..."},
            "finish_reason": "stop"
        }
    ],
    "usage": {
        "prompt_tokens": 42,
        "completion_tokens": 150,
        "total_tokens": 192
    }
}
```

### 4.2. Streaming Chat

Use `client.stream` to receive tokens as soon as they are generated.

```python
async for chunk in client.stream(
    key="my-team-key",
    model="gpt-4o",
    messages=[{"role": "user", "content": "Write a poem about Rust."}]
):
    # Each chunk is a dict with the following keys:
    #   "id", "model", "delta", "finish_reason", "usage"
    print(chunk["delta"], end="", flush=True)
```

!!! tip "Performance Tip"
    Streaming reduces **Time-To-First-Token (TTFT)** significantly because the client doesn't have to wait for the entire generation to complete.

### 4.3. Request Parameters

| Parameter | Type | Description |
| :--- | :--- | :--- |
| `key` | `str` | Your virtual API key (used for auth, quotas, and tracking). |
| `model` | `str` | The model name or alias (e.g., `"gpt-4o"` or `"fast"`). |
| `messages` | `list[dict]` | Conversation history. Each dict has `role` and `content`. |
| `temperature` | `float \| None` | Sampling temperature (0.0 = deterministic, 2.0 = creative). |
| `max_tokens` | `int \| None` | Hard limit on generated tokens. |
| `stop` | `list[str] \| None` | Sequences that will halt generation. |

---

## 5. Traffic Mirroring

The `Client` has a built-in `set_mirror` method for shadowing traffic to a secondary model.

```python
# Mirror traffic to "claude-3-haiku-20240307" with a 10% sample rate
await client.set_mirror(model="claude-3-haiku-20240307", sample_rate=0.1)
```

**How it works:**
*   With probability `sample_rate`, the client asynchronously fires a **second request** to the mirror model.
*   The mirror response is **discarded** — only the primary response is returned.
*   Mirror errors do **not** affect the primary response.

!!! info "Fire-and-Forget"
    Mirror requests are fire-and-forget. They are not awaited, so they add zero latency to your critical path.

---

## 6. Error Handling

The Python client surfaces errors as standard exceptions. You should wrap your calls in `try/except` blocks.

```python
try:
    response = await client.chat(
        key="my-team-key",
        model="gpt-4o",
        messages=[{"role": "user", "content": "Hello!"}]
    )
except Exception as e:
    # The underlying Rust core raises HyperInferError, which surfaces
    # as a Python exception with the error message attached.
    print(f"Request failed: {e}")
```

!!! note "Exception Types"
    The current Python client surfaces all errors as generic Python exceptions. Granular exception types (e.g., `RateLimitError`, `AuthenticationError`) are a planned follow-up.

---

## 7. Framework Integrations

HyperInfer integrates with popular LLM frameworks.

### 7.1. LangChain

```bash
pip install hyperinfer-langchain
```

```python
from hyperinfer import Config
from hyperinfer_langchain import HyperInferChatModel

# Construct a HyperInfer-backed LangChain chat model
llm = await HyperInferChatModel.from_config(
    config=config,
    model="fast",
    virtual_key="my-team",
)
```

### 7.2. LlamaIndex

```bash
pip install hyperinfer-llamaindex
```

```python
from hyperinfer_llamaindex import HyperInferLLM

llm = HyperInferLLM.from_config(
    config=config,
    model="fast",
)
```

---

## 8. Comparison Table: Python vs. Rust

| Feature | Python (Client) | Rust (Data Plane) |
| :--- | :--- | :--- |
| **Primary Use** | Application-level LLM calls | Building custom clients/gateways |
| **Async Model** | `async/await` with `asyncio` | `tokio` runtime |
| **Streaming** | `async for chunk in client.stream(...)` | `futures::StreamExt` |
| **Error Handling** | Exceptions (`try/except`) | `Result<T, HyperInferError>` |
| **Performance** | Thin PyO3 wrapper (near-native speed) | Native (zero-copy) |
