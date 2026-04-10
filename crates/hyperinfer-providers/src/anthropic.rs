use super::provider_trait::LlmProvider;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Choice, HyperInferError, MessageRole, Usage,
};
use reqwest::Client;
use std::pin::Pin;

pub struct AnthropicProvider {
    http_client: Client,
    base_url: &'static str,
}

fn build_anthropic_request_body(
    request: &ChatRequest,
    stream: bool,
) -> (
    Option<String>,
    Vec<serde_json::Value>,
    serde_json::Map<String, serde_json::Value>,
) {
    let system_messages: Vec<_> = request
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .map(|m| m.content.as_str())
        .collect();

    let system = if system_messages.is_empty() {
        None
    } else {
        Some(system_messages.join("\n"))
    };

    let messages: Vec<_> = request
        .messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .map(|m| {
            serde_json::json!({
                "role": match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    _ => "user",
                },
                "content": m.content
            })
        })
        .collect();

    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), serde_json::json!(request.model));
    body.insert("messages".to_string(), serde_json::json!(messages));
    body.insert(
        "max_tokens".to_string(),
        serde_json::json!(request.max_tokens.unwrap_or(1024)),
    );

    if stream {
        body.insert("stream".to_string(), serde_json::json!(true));
    }
    if let Some(s) = &system {
        body.insert("system".to_string(), serde_json::json!(s));
    }
    if let Some(t) = request.temperature {
        body.insert("temperature".to_string(), serde_json::json!(t));
    }
    if let Some(stop) = &request.stop {
        body.insert("stop_sequences".to_string(), serde_json::json!(stop));
    }

    (system, messages, body)
}

impl AnthropicProvider {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            base_url: "https://api.anthropic.com",
        })
    }
}

// Clone is required by LlmProvider supertrait. The HTTP client is cheap to clone.
impl Clone for AnthropicProvider {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            base_url: self.base_url,
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn base_url(&self) -> &'static str {
        self.base_url
    }

    async fn chat(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Result<ChatResponse, HyperInferError> {
        let url = format!("{}/v1/messages", self.base_url);

        let (_system, _messages, body) = build_anthropic_request_body(request, false);
        let body = serde_json::Value::Object(body);

        let response = self
            .http_client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
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
        struct AnthropicResponse {
            id: String,
            content: Vec<ContentBlock>,
            usage: AnthropicUsageDetail,
            stop_reason: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct ContentBlock {
            text: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct AnthropicUsageDetail {
            input_tokens: u32,
            output_tokens: u32,
        }

        let data: AnthropicResponse = response.json().await?;

        let content = data
            .content
            .into_iter()
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ChatResponse {
            id: data.id,
            model: request.model.clone(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: MessageRole::Assistant,
                    content,
                },
                finish_reason: data.stop_reason,
            }],
            usage: Usage {
                input_tokens: data.usage.input_tokens,
                output_tokens: data.usage.output_tokens,
            },
        })
    }

    fn stream(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + '_>> {
        let url = format!("{}/v1/messages", self.base_url);
        let client = self.http_client.clone();
        let model = request.model.clone();
        let api_key = api_key.to_string();

        let (_system, _messages, mut body) = build_anthropic_request_body(request, true);
        body.insert("stream".to_string(), serde_json::json!(true));
        let body = serde_json::Value::Object(body);

        let stream = async_stream::try_stream! {
            let response = client
                .post(&url)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
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
            let mut stream_id = String::new();
            let mut cached_input_tokens: u32 = 0;

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

                    #[derive(serde::Deserialize)]
                    struct AnthropicEvent {
                        #[serde(rename = "type")]
                        event_type: String,
                        message: Option<AnthropicMessage>,
                        delta: Option<AnthropicDelta>,
                        usage: Option<AnthropicStreamUsage>,
                    }
                    #[derive(serde::Deserialize)]
                    struct AnthropicMessage {
                        id: String,
                        usage: Option<AnthropicStreamUsage>,
                    }
                    #[derive(serde::Deserialize)]
                    struct AnthropicDelta {
                        #[serde(rename = "type")]
                        delta_type: String,
                        #[serde(default)]
                        text: String,
                        stop_reason: Option<String>,
                    }
                    #[derive(serde::Deserialize)]
                    struct AnthropicStreamUsage {
                        input_tokens: Option<u32>,
                        output_tokens: Option<u32>,
                    }
                    #[derive(serde::Deserialize)]
                    struct AnthropicStreamError {
                        error: AnthropicErrorDetail,
                    }
                    #[derive(serde::Deserialize)]
                    struct AnthropicErrorDetail {
                        message: String,
                    }

                    match serde_json::from_str::<AnthropicEvent>(&data) {
                        Ok(event) => match event.event_type.as_str() {
                            "error" => {
                                let msg = serde_json::from_str::<AnthropicStreamError>(&data)
                                    .map(|e| e.error.message)
                                    .unwrap_or_else(|_| data.clone());
                                Err(HyperInferError::StreamParse {
                                    message: msg,
                                    raw: data.clone(),
                                })?;
                                return;
                            }
                            "message_start" => {
                                if let Some(msg) = event.message {
                                    stream_id = msg.id;
                                    if let Some(u) = msg.usage {
                                        cached_input_tokens = u.input_tokens.unwrap_or(0);
                                    }
                                }
                            }
                            "content_block_delta" => {
                                if let Some(delta) = event.delta {
                                    if delta.delta_type == "text_delta" {
                                        yield ChatChunk {
                                            id: stream_id.clone(),
                                            model: model.clone(),
                                            delta: delta.text,
                                            finish_reason: None,
                                            usage: None,
                                        };
                                    }
                                }
                            }
                            "message_delta" => {
                                let finish_reason = event.delta
                                    .as_ref()
                                    .and_then(|d| d.stop_reason.clone());
                                let usage = event.usage.map(|u| Usage {
                                    input_tokens: cached_input_tokens,
                                    output_tokens: u.output_tokens.unwrap_or(0),
                                });
                                yield ChatChunk {
                                    id: stream_id.clone(),
                                    model: model.clone(),
                                    delta: String::new(),
                                    finish_reason,
                                    usage,
                                };
                            }
                            "message_stop" => return,
                            _ => {}
                        },
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
    fn test_anthropic_provider_name() {
        let provider = AnthropicProvider::new().unwrap();
        assert_eq!(provider.name(), "anthropic");
    }

    #[test]
    fn test_anthropic_provider_base_url() {
        let provider = AnthropicProvider::new().unwrap();
        assert_eq!(provider.base_url(), "https://api.anthropic.com");
    }

    #[test]
    fn test_anthropic_provider_supports_streaming() {
        let provider = AnthropicProvider::new().unwrap();
        assert!(provider.supports_streaming());
    }
}
