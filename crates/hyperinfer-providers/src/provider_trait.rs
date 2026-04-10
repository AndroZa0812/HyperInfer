use async_trait::async_trait;
use futures::Stream;
use hyperinfer_core::{ChatChunk, ChatRequest, ChatResponse};
use std::pin::Pin;

#[async_trait]
pub trait LlmProvider: dyn_clone::DynClone + Send + Sync {
    fn name(&self) -> &str;

    fn base_url(&self) -> &str {
        ""
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn chat(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Result<ChatResponse, hyperinfer_core::HyperInferError>;

    fn stream(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, hyperinfer_core::HyperInferError>> + Send + '_>>;

    async fn health_check(&self, api_key: &str) -> Result<(), hyperinfer_core::HyperInferError> {
        let request = ChatRequest {
            model: "health-check-probe".to_string(),
            messages: vec![hyperinfer_core::ChatMessage {
                role: hyperinfer_core::MessageRole::User,
                content: "ping".to_string(),
            }],
            temperature: None,
            max_tokens: Some(1),
            stream: None,
            stop: None,
        };
        self.chat(&request, api_key).await?;
        Ok(())
    }
}

dyn_clone::clone_trait_object!(LlmProvider);

/// Helper to obtain an owned clone of a boxed trait object.
/// The returned `Box<dyn LlmProvider>` is `'static` so its `stream()`
/// method can be called without lifetime issues.
pub struct OwnedClone {
    inner: Box<dyn LlmProvider + Send + 'static>,
}

impl OwnedClone {
    pub fn new(provider: Box<dyn LlmProvider + Send + 'static>) -> Self {
        Self { inner: provider }
    }

    pub fn into_stream(
        self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Pin<
        Box<
            dyn Stream<Item = Result<ChatChunk, hyperinfer_core::HyperInferError>> + Send + 'static,
        >,
    > {
        use futures::StreamExt;

        let provider = self.inner;
        let request = request.clone();
        let api_key = api_key.to_string();

        // Use async_stream to take ownership of `provider`, `request`, and `api_key`.
        let stream = async_stream::try_stream! {
            // We can now call provider.stream() here because provider is alive
            // for the duration of this async block.
            let mut inner_stream = provider.stream(&request, &api_key);
            while let Some(chunk) = inner_stream.next().await {
                yield chunk?;
            }
        };

        Box::pin(stream)
    }
}
