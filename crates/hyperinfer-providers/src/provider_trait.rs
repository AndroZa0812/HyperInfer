use async_trait::async_trait;
use futures::Stream;
use hyperinfer_core::{ChatChunk, ChatRequest, ChatResponse};
use std::pin::Pin;
use std::sync::Arc;

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
    ) -> Pin<
        Box<
            dyn Stream<Item = Result<ChatChunk, hyperinfer_core::HyperInferError>> + Send + 'static,
        >,
    >;

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

/// Holds a reference-counted LlmProvider and produces a 'static stream.
/// Cloning the Arc is O(1) — no deep-clone of the provider's HTTP client.
pub struct StreamingProvider {
    inner: Arc<dyn LlmProvider>,
}

impl StreamingProvider {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
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
        self.inner.stream(request, api_key)
    }
}
