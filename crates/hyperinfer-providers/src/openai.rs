use super::provider_trait::LlmProvider;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Choice, HyperInferError, MessageRole, Usage,
};
use reqwest::Client;
use std::pin::Pin;

pub struct OpenAiProvider {
    http_client: Client,
    base_url: &'static str,
}

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

// Clone is required by LlmProvider supertrait. The HTTP client is cheap to clone.
impl Clone for OpenAiProvider {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            base_url: self.base_url,
        }
    }
}

fn chat_request_to_openai_body(request: &ChatRequest) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), serde_json::json!(request.model));
    body.insert("messages".to_string(), serde_json::json!(request.messages));
    if let Some(temperature) = request.temperature {
        body.insert("temperature".to_string(), serde_json::json!(temperature));
    }
    if let Some(max_tokens) = request.max_tokens {
        body.insert("max_tokens".to_string(), serde_json::json!(max_tokens));
    }
    if let Some(stop) = &request.stop {
        body.insert("stop".to_string(), serde_json::json!(stop));
    }
    serde_json::Value::Object(body)
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn base_url(&self) -> &'static str {
        self.base_url
    }

    async fn chat(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Result<ChatResponse, HyperInferError> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = chat_request_to_openai_body(request);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(HyperInferError::ApiError {
                status: status.as_u16(),
                message: error_text,
            });
        }

        #[derive(serde::Deserialize)]
        struct OpenAiResponse {
            id: String,
            choices: Vec<OpenAiChoice>,
            usage: UsageDetail,
        }

        #[derive(serde::Deserialize)]
        struct OpenAiChoice {
            index: u32,
            message: Message,
            finish_reason: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct Message {
            role: String,
            content: String,
        }

        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct UsageDetail {
            prompt_tokens: u32,
            completion_tokens: u32,
            total_tokens: u32,
        }

        let data: OpenAiResponse = response.json().await?;

        Ok(ChatResponse {
            id: data.id,
            model: request.model.clone(),
            choices: data
                .choices
                .into_iter()
                .map(|c| Choice {
                    index: c.index,
                    message: ChatMessage {
                        role: match c.message.role.as_str() {
                            "assistant" => MessageRole::Assistant,
                            "user" => MessageRole::User,
                            "system" => MessageRole::System,
                            other => {
                                tracing::warn!(
                                    "Unknown OpenAI role '{}', defaulting to Assistant",
                                    other
                                );
                                MessageRole::Assistant
                            }
                        },
                        content: c.message.content,
                    },
                    finish_reason: c.finish_reason,
                })
                .collect(),
            usage: Usage {
                input_tokens: data.usage.prompt_tokens,
                output_tokens: data.usage.completion_tokens,
            },
        })
    }

    fn stream(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + '_>> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let mut body = serde_json::Map::new();
        body.insert(
            "model".to_string(),
            serde_json::json!(request.model.clone()),
        );
        body.insert(
            "messages".to_string(),
            serde_json::json!(request.messages.clone()),
        );
        body.insert("stream".to_string(), serde_json::json!(true));
        body.insert(
            "stream_options".to_string(),
            serde_json::json!({ "include_usage": true }),
        );
        if let Some(temperature) = request.temperature {
            body.insert("temperature".to_string(), serde_json::json!(temperature));
        }
        if let Some(max_tokens) = request.max_tokens {
            body.insert("max_tokens".to_string(), serde_json::json!(max_tokens));
        }
        if let Some(ref stop) = request.stop {
            body.insert("stop".to_string(), serde_json::json!(stop));
        }
        let body = serde_json::Value::Object(body);
        let client = self.http_client.clone();
        let api_key = api_key.to_string();

        let stream = async_stream::try_stream! {
            let response = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                Err(HyperInferError::ApiError {
                    status: status.as_u16(),
                    message: error_text,
                })?;
                return;
            }

            let mut byte_stream = response.bytes_stream();
            let mut raw_buf: Vec<u8> = Vec::new();

            while let Some(bytes) = byte_stream.next().await {
                let bytes = bytes?;
                raw_buf.extend_from_slice(&bytes);

                let mut lines = Vec::new();
                super::drain_lines(&mut raw_buf, &mut lines);

                for line in lines {
                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let data = if let Some(d) = line.strip_prefix("data: ") { d.to_owned() } else { continue };
                    if data == "[DONE]" {
                        return;
                    }

                    #[derive(serde::Deserialize)]
                    struct StreamChoice {
                        delta: DeltaContent,
                        finish_reason: Option<String>,
                    }
                    #[derive(serde::Deserialize)]
                    struct DeltaContent {
                        #[serde(default)]
                        content: String,
                    }
                    #[derive(serde::Deserialize)]
                    struct StreamEvent {
                        #[serde(default)]
                        id: String,
                        #[serde(default)]
                        model: String,
                        #[serde(default)]
                        choices: Vec<StreamChoice>,
                        usage: Option<OpenAiStreamUsage>,
                    }
                    #[derive(serde::Deserialize)]
                    struct OpenAiStreamUsage {
                        prompt_tokens: u32,
                        completion_tokens: u32,
                    }
                    #[derive(serde::Deserialize)]
                    struct OpenAiStreamError {
                        error: OpenAiErrorDetail,
                    }
                    #[derive(serde::Deserialize)]
                    struct OpenAiErrorDetail {
                        message: String,
                    }

                    if let Ok(err_event) = serde_json::from_str::<OpenAiStreamError>(&data) {
                        Err(HyperInferError::StreamParse {
                            message: err_event.error.message,
                            raw: data.clone(),
                        })?;
                        return;
                    }

                    match serde_json::from_str::<StreamEvent>(&data) {
                        Ok(event) => {
                            let finish_reason = event.choices.first()
                                .and_then(|c| c.finish_reason.clone());
                            let delta = event.choices.first()
                                .map(|c| c.delta.content.clone())
                                .unwrap_or_default();
                            let usage = event.usage.map(|u| Usage {
                                input_tokens: u.prompt_tokens,
                                output_tokens: u.completion_tokens,
                            });

                            yield ChatChunk {
                                id: event.id,
                                model: event.model,
                                delta,
                                finish_reason,
                                usage,
                            };
                        }
                        Err(parse_err) => {
                            Err(HyperInferError::StreamParse {
                                message: parse_err.to_string(),
                                raw: data.clone(),
                            })?;
                            return;
                        }
                    }
                }
            }
        };

        Box::pin(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_name() {
        let provider = OpenAiProvider::new().unwrap();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_provider_base_url() {
        let provider = OpenAiProvider::new().unwrap();
        assert_eq!(provider.base_url(), "https://api.openai.com");
    }

    #[test]
    fn test_openai_provider_supports_streaming() {
        let provider = OpenAiProvider::new().unwrap();
        assert!(provider.supports_streaming());
    }
}
