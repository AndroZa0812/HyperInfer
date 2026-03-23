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

    fn stream(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, hyperinfer_core::HyperInferError>> + Send + '_>>;
}
