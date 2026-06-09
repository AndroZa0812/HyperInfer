# Custom LLM Providers

This guide explains how to implement and use custom LLM providers in HyperInfer. We cover the core `LlmProvider` trait for developers extending the system and how to interact with providers via the Python client.

## Conceptual Overview

HyperInfer uses a provider-based abstraction to decouple the core routing and telemetry logic from specific LLM implementations (like OpenAI, Anthropic, or local models).

### The Request Lifecycle

1.  **Client Request**: A user sends a `ChatRequest` through the `Router`.
2.  **Routing**: The `Router` selects a specific `Deployment` based on your routing strategy.
3.  **Provider Execution**: The `Router` invokes the `LlmProvider` associated with that deployment.
4.  **Response**: The provider translates the generic `ChatRequest` into a vendor-specific API call and returns a `ChatResponse` or a `Stream` of `ChatChunk`s.

---

## Implementing the `LlmProvider` Trait

The `LlmProvider` trait defines how the engine interacts with an LLM backend. This is the core interface you must implement when adding a new provider.

=== "Python"

    In Python, you generally consume providers by name. The high-level `Client` abstracts away the underlying implementation.

    ```python
    from hyperinfer import Client, Config

    async def main():
        client = Client(config=Config())
        await client.init()

        # Use a provider by its registered name
        response = await client.chat(
            key="my-key",
            model="my-custom-provider",
            messages=[{"role": "user", "content": "Hello!"}]
        )
        print(response)
    ```

=== "Rust"

    ```rust
    use async_trait::async_trait;
    use hyperinfer_providers::LlmProvider;
    use hyperinfer_core::{ChatRequest, ChatResponse, HyperInferError};

    pub struct MyCustomProvider {
        base_url: String,
    }

    impl MyCustomProvider {
        pub fn new(base_url: impl Into<String>) -> Self {
            Self { base_url: base_url.into() }
        }
    }

    #[async_trait]
    impl LlmProvider for MyCustomProvider {
        fn name(&self) -> &str {
            "my-custom-provider"
        }

        fn base_url(&self) -> &str {
            &self.base_url
        }

        async fn chat(
            &self,
            request: &ChatRequest,
            api_key: &str,
        ) -> Result<ChatResponse, HyperInferError> {
            // Your logic to call the external API goes here
            unimplemented!()
        }
    }
    ```

---

## Handling Asynchronous Streams

Implementing streaming requires careful management of asynchronous streams to ensure they are thread-safe and pinned for the async executor.

=== "Python"

    Python uses native async generators (`async for`) to handle streams cleanly. The `Client.stream` method returns an `AsyncIterator`.

    ```python
    from hyperinfer import Client, Config

    async def main():
        client = Client(config=Config())
        await client.init()

        messages = [{"role": "user", "content": "Tell me a story."}]

        # Stream chunks as they arrive
        async for chunk in client.stream(
            key="my-key",
            model="my-custom-provider",
            messages=messages
        ):
            print(chunk.delta, end="", flush=True)
    ```

=== "Rust"

    ```rust
    use futures::Stream;
    use std::pin::Pin;

    // ... inside impl LlmProvider for MyCustomProvider ...

    fn stream(
        &self,
        _request: &ChatRequest,
        _api_key: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + 'static>> {
        // 1. Create a stream of chunks
        let stream = async_stream::stream! {
            yield Ok(ChatChunk { /* ... */ });
        };

        // 2. Box and Pin the stream for the trait requirement
        Box::pin(stream)
    }
    ```

---

## Using Custom Endpoints

If you are running a local model that is OpenAI-compatible (e.g., Ollama, vLLM), you can route requests to it by using an alias that points to the local model name and configuring the appropriate API key.

=== "Python"

    ```python
    from hyperinfer import Client, Config

    # Configure the alias to point to your local model
    # and provide the API key for the local server
    config = (Config()
        .with_api_key("openai", "ollama")  # Local servers often accept any key
        .with_alias("local-llama", "llama3")
    )

    client = Client(redis_url="redis://localhost:6379", config=config)

    async def main():
        await client.init()

        response = await client.chat(
            key="local-key",
            model="local-llama",
            messages=[{"role": "user", "content": "Explain quantum physics."}]
        )
        print(response["choices"][0]["message"]["content"])

    if __name__ == "__main__":
        import asyncio
        asyncio.run(main())
    ```

=== "Rust"

    ```rust
    // In Rust, custom endpoints are handled during Deployment configuration
    // via the Control Plane. The `base_url` is set on the `LlmProvider` trait.
    let provider = MyCustomProvider::new("http://localhost:11434/v1");
    ```

!!! note "Custom Provider Endpoints"
    Pointing a client at a non-default endpoint requires implementing a custom `LlmProvider` in Rust (with the desired `base_url()`) and registering it with the data plane. The Python client itself does not expose a `base_url` configuration option.

---

## Comparison Table

| Feature | Rust (Implementation) | Python (Usage) |
| :--- | :--- | :--- |
| **Goal** | Define *how* a provider works | *Consume* a provider |
| **Custom Logic** | Implement `LlmProvider` trait | Configure `base_url` |
| **Async Model** | `Pin<Box<dyn Stream...>>` | `AsyncIterator` / `async for` |
| **Configuration** | Hardcoded in Trait or via API | Passed via `Config` object |
