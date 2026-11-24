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
    HyperInfer requires **Python 3.11 or higher** (tested on 3.11–3.15).

---

## 2. Configuration with `Config`

You can configure `Client` directly with keyword arguments (or environment variables), or construct a reusable `Config` object.

### 2.1. Keyword Arguments

| Parameter | Type | Purpose |
| :--- | :--- | :--- |
| `api_keys` | `dict[str, str]` | Provider API keys (e.g., `{"openai": "sk-...", "anthropic": "sk-ant-..."}`). |
| `model_aliases` | `dict[str, str]` | Map short aliases to real model names (e.g., `{"fast": "gpt-4o-mini"}`). |
| `routing_rules` | `list[dict]` | Routing rules with priority and fallback model lists. |
| `quotas` | `dict[str, dict]` | Per-key limits (`rpm`, `tpm`, `budget_cents`). |
| `default_provider` | `str \| None` | Provider used when none is specified for a model. |

### 2.2. Examples

```python
from hyperinfer import Config

# Direct keyword constructor
config = Config(
    api_keys={"openai": "sk-...", "anthropic": "sk-ant-..."},
    model_aliases={"fast": "gpt-4o-mini"},
    routing_rules=[
        {"name": "default", "priority": 1, "fallback_models": ["gpt-4o-mini", "claude-3-haiku-20240307"]}
    ],
    quotas={"my-team": {"rpm": 100, "tpm": 50_000, "budget_cents": 10_000}},
    default_provider="openai",
)

# Or load automatically from OPENAI_API_KEY / ANTHROPIC_API_KEY / GEMINI_API_KEY
config = Config.from_env()
```

!!! tip "Fluent Builder Compatibility"
    The legacy fluent builder methods (`.with_api_key(...)`, `.with_alias(...)`, `.with_routing_rule(...)`, `.with_quota(...)`, `.with_default_provider(...)`) remain supported for backward compatibility.

---

## 3. Client Initialization

The `Client` is the main entry point. It accepts configuration directly as keyword arguments, automatically reads `REDIS_URL` and provider API keys from environment variables when omitted, and initializes lazily on first use.

### 3.1. Minimal Initialization (Environment Variables)

```python
from hyperinfer import Client

# Reads OPENAI_API_KEY / ANTHROPIC_API_KEY / GEMINI_API_KEY and REDIS_URL from env
client = Client()
```

### 3.2. Direct Keyword Configuration

```python
from hyperinfer import Client

client = Client(
    api_keys={"openai": "sk-..."},
    model_aliases={"fast": "gpt-4o-mini"},
    default_provider="openai",
    virtual_key="my-team-key",
    redis_url="redis://localhost:6379",
)
```

### 3.3. Async Context Manager

Use `async with` to ensure the client is cleanly initialized and closed:

```python
async def main():
    async with Client(
        api_keys={"openai": "sk-..."},
        model_aliases={"fast": "gpt-4o-mini"},
        virtual_key="my-team-key",
    ) as client:
        response = await client.chat(
            model="fast",
            messages=[{"role": "user", "content": "Hello!"}],
        )
        print(response["choices"][0]["message"]["content"])
```

### 3.4. `Client` Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `redis_url` | `str \| None` | `REDIS_URL` or `"redis://localhost:6379"` | Connection string for the Redis state store. |
| `config` | `Config \| None` | `None` | Optional pre-built `Config` object. |
| `api_keys` | `dict[str, str] \| None` | `None` | Provider API keys (merged over `config` if both are given). |
| `model_aliases` | `dict[str, str] \| None` | `None` | Model alias mappings. |
| `routing_rules` | `list[dict] \| None` | `None` | Routing rule definitions. |
| `quotas` | `dict[str, dict] \| None` | `None` | Per-key quota definitions. |
| `default_provider` | `str \| None` | `None` | Default provider name. |
| `virtual_key` | `str` | `"default"` | Default virtual key used when `key` is omitted in `chat()` / `stream()`. |

---

## 4. Making Chat Requests

### 4.1. Unary (Non-Streaming) Chat

Use `client.chat` when you need the complete response before proceeding.

```python
response = await client.chat(
    model="gpt-4o",
    messages=[
        {"role": "system", "content": "You are a helpful assistant."},
        {"role": "user", "content": "Explain quantum entanglement in 3 sentences."},
    ],
    temperature=0.7,
    max_tokens=150,
    stop=["\n\n"],  # Stop generation at double newline
)

# Shorthand 2-arg form is also supported:
response = await client.chat("gpt-4o", [{"role": "user", "content": "Hello!"}])
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
    model="gpt-4o",
    messages=[{"role": "user", "content": "Write a poem about Rust."}],
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
| `model` | `str` | The model name or alias (e.g., `"gpt-4o"` or `"fast"`). |
| `messages` | `list[dict]` | Conversation history. Each dict has `role` and `content`. |
| `key` | `str \| None` | Optional virtual API key override (defaults to `Client`'s `virtual_key`). |
| `temperature` | `float \| None` | Sampling temperature (0.0 = deterministic, 2.0 = creative). |
| `max_tokens` | `int \| None` | Hard limit on generated tokens. |
| `stop` | `list[str] \| None` | Sequences that will halt generation. |
| `tools` | `list[dict] \| None` | Tool definitions for function calling. |
| `tool_choice` | `Any` | Tool selection strategy (`"auto"`, `"none"`, `"required"`, or specific function). |
| `response_format` | `dict \| None` | Structured output specification (e.g., `{"type": "json_object"}`). |

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
from hyperinfer_langchain import HyperInferChatModel

# Direct idiomatic constructor (no async from_config needed)
llm = HyperInferChatModel(
    model="gpt-4o",
    api_key="sk-...",
    virtual_key="my-team",
    temperature=0.7,
)
response = llm.invoke("Hello!")
```

### 7.2. LlamaIndex

```bash
pip install hyperinfer-llamaindex
```

```python
from hyperinfer_llamaindex import HyperInferLLM

llm = HyperInferLLM(
    model="gpt-4o",
    api_key="sk-...",
    temperature=0.7,
)
response = llm.complete("Hello!")
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
