# Data Plane Guide

The **Data Plane** is the high-performance component of HyperInfer that handles direct LLM calls from your application. It is designed for low latency, high throughput, and zero-downtime operation.

!!! info "Terminology"
    The terms **Data Plane** and **Client** are used interchangeably throughout this documentation — they refer to the same thing. Similarly, **Control Plane** and **Server** are synonyms. We use "Data Plane"/"Control Plane" in architecture discussions and "Client"/"Server" when talking about specific code or binaries.

## 1. What is the Data Plane?

The data plane is a lightweight client that:

*   Receives chat requests from your application.
*   Consults the **Control Plane** (or local config) for routing rules.
*   Forwards the request to the optimal upstream LLM provider.
*   Streams the response back to your application.
*   Records usage, latency, and errors in **Redis** for observability.

!!! tip "Stateless by Design"
    The data plane holds **no persistent state** of its own. All routing, caching, and quota data lives in Redis. This means you can run as many data plane instances as you need behind a load balancer, and they will all behave consistently.

---

## 2. Installation

The data plane is available in both Rust and Python.

=== "Python"

    ```bash
    pip install hyperinfer
    ```

=== "Rust"

    Add the data plane client to your `Cargo.toml`:

    ```toml
    [dependencies]
    hyperinfer-client = "0.1"
    tokio = { version = "1", features = ["full"] }
    futures-util = "0.3"
    ```

---

## 3. Client Initialization

### 3.1. Basic Setup

=== "Python"

    ```python
    from hyperinfer import Client, Config

    config = (Config()
        .with_api_key("openai", "sk-...")
        .with_alias("fast", "gpt-4o-mini")
    )

    client = Client(redis_url="redis://localhost:6379", config=config)
    await client.init()
    ```

=== "Rust"

    ```rust
    use hyperinfer_client::HyperInferClient;

    #[tokio::main]
    async fn main() -> Result<(), Box<dyn std::error::Error>> {
        let client = HyperInferClient::new("redis://localhost:6379", "my-team").await?;
        Ok(())
    }
    ```

### 3.2. Key Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `redis_url` | `str` | `"redis://localhost:6379"` | Connection string for the Redis state store. |
| `virtual_key` | `str` | `""` | Your team's virtual key (Rust only, passed to `HyperInferClient::new`). |
| `config` | `Config \| None` | `None` | Python-only: a pre-built `Config` object with API keys, aliases, etc. |

---

## 4. Making Chat Requests

### 4.1. Unary (Non-Streaming) Chat

Use unary chat when you need the complete response before proceeding.

=== "Python"

    ```python
    response = await client.chat(
        key="my-team-key",
        model="gpt-4o-mini",
        messages=[{"role": "user", "content": "Hello!"}]
    )
    print(response["choices"][0]["message"]["content"])
    ```

=== "Rust"

    ```rust
    let response = client.chat("gpt-4o-mini", "Hello!").await?;
    println!("{}", response.choices[0].message.content);
    ```

### 4.2. Streaming Chat

Streaming is essential for chat UIs and long-form generation. It reduces **Time-To-First-Token (TTFT)**.

=== "Python"

    ```python
    async for chunk in client.stream(
        key="my-team-key",
        model="gpt-4o-mini",
        messages=[{"role": "user", "content": "Tell me a story"}]
    ):
        # chunk is a dict with: id, model, delta, finish_reason, usage
        print(chunk["delta"], end="", flush=True)
    ```

=== "Rust"

    ```rust
    use futures_util::StreamExt;

    let mut stream = client.chat_stream("gpt-4o-mini", "Tell me a story").await?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        print!("{}", chunk.choices[0].delta.content);
    }
    ```

---

## 5. Traffic Mirroring

**Traffic mirroring** (also called shadowing) lets you send a copy of your production traffic to a secondary model for evaluation, A/B testing, or debugging — without affecting your users.

=== "Python"

    ```python
    # Mirror traffic to "claude-3-haiku-20240307" with a 10% sample rate
    await client.set_mirror(model="claude-3-haiku-20240307", sample_rate=0.1)
    ```

=== "Rust"

    ```rust
    client.set_mirror("claude-3-haiku", 0.1).await; // mirror 10%
    ```

### How Mirroring Works

1.  The data plane receives a request for the primary model.
2.  With probability `sample_rate`, it asynchronously fires a **second request** to the mirror model.
3.  The mirror response is **discarded** — only the primary response is returned to the user.
4.  Mirror errors do **not** affect the primary response.

!!! info "Fire-and-Forget"
    Mirror requests are **fire-and-forget**. They are not awaited, so they add zero latency to your critical path. If the mirror model is slow or down, you won't notice.

---

## 6. Error Handling

The data plane surfaces errors so you can build robust retry logic.

=== "Python"

    ```python
    try:
        response = await client.chat(key, model, messages)
    except Exception as e:
        # The underlying Rust core raises HyperInferError, which surfaces
        # as a Python exception with the error message attached.
        print(f"Request failed: {e}")
    ```

=== "Rust"

    ```rust
    use hyperinfer_core::HyperInferError;

    match client.chat("gpt-4o", "Hello!").await {
        Ok(response) => println!("{}", response.choices[0].message.content),
        Err(e) => eprintln!("Request failed: {:?}", e),
    }
    ```

!!! note "Granular Exception Types"
    The current Python client surfaces all errors as generic Python exceptions. Granular types (e.g., `RateLimitError`, `ProviderError`) are a planned follow-up.

---

## 7. Performance Tuning

For high-throughput applications, consider these optimizations:

| Optimization | Impact | How |
| :--- | :--- | :--- |
| **Connection Pooling** | High | Reuse a single `Client` instance across your app. |
| **Streaming** | High | Use `stream` instead of `chat` for TTFT-sensitive UIs. |
| **Async Context** | Medium | Always use `async with Client(...)` to avoid resource leaks. |

---

## 8. Comparison Table: Data Plane vs. Control Plane

| Feature | Data Plane (Client) | Control Plane (Server) |
| :--- | :--- | :--- |
| **Primary Role** | Forward LLM requests | Manage configuration, auth, and routing |
| **Latency Sensitivity** | Critical (microseconds matter) | Low (admin operations) |
| **State** | Stateless (reads from Redis) | Stateful (owns the database) |
| **Scaling** | Horizontal (add more instances) | Vertical (usually 1-3 replicas) |
| **User-Facing** | Yes | No (admin-only) |
