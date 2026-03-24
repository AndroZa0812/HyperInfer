use async_trait::async_trait;
use futures::Stream;
use hyperinfer_core::{ChatChunk, ChatRequest, ChatResponse};
use std::pin::Pin;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn base_url(&self) -> &'static str {
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

    // TODO: stream() and supports_streaming() are currently dead code because
    // chat_stream() in hyperinfer-client routes streaming through HttpCaller
    // directly (http_caller.stream_openai() / http_caller.stream_anthropic())
    // instead of through the provider registry. In future, routing streaming
    // through LlmProvider::stream() would allow custom providers to implement
    // their own streaming logic.
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
