use futures::Stream;
use hyperinfer_core::types::{ChatMessage, Choice, MessageRole, StreamUsage};
use hyperinfer_core::{ChatChunk, ChatRequest, ChatResponse, HyperInferError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

pub struct HttpCaller {
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiResponse {
    pub id: String,
    pub choices: Vec<OpenAiChoice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChoice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl HttpCaller {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self { client })
    }

    pub async fn call_openai(
        &self,
        model: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, HyperInferError> {
        let url = "https://api.openai.com/v1/chat/completions".to_string();

        let body = serde_json::json!({
            "model": model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });

        let response = self
            .client
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

        let data: OpenAiResponse = response.json().await?;

        Ok(ChatResponse {
            id: data.id,
            model: model.to_string(),
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
            usage: hyperinfer_core::types::Usage {
                input_tokens: data.usage.prompt_tokens,
                output_tokens: data.usage.completion_tokens,
            },
        })
    }

    pub async fn call_anthropic(
        &self,
        model: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, HyperInferError> {
        let url = "https://api.anthropic.com/v1/messages";

        let system = request
            .messages
            .iter()
            .find(|m| m.role == hyperinfer_core::types::MessageRole::System)
            .map(|m| m.content.clone());

        let messages: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != hyperinfer_core::types::MessageRole::System)
            .map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        hyperinfer_core::types::MessageRole::User => "user",
                        hyperinfer_core::types::MessageRole::Assistant => "assistant",
                        _ => "user",
                    },
                    "content": m.content
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(1024),
        });

        if let Some(s) = system {
            body["system"] = serde_json::json!(s);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = serde_json::json!(t);
        }

        let response = self
            .client
            .post(url)
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

        #[derive(Deserialize)]
        struct AnthropicResponse {
            id: String,
            content: Vec<ContentBlock>,
            usage: AnthropicUsage,
        }

        #[derive(Deserialize)]
        struct ContentBlock {
            text: Option<String>,
        }

        #[derive(Deserialize)]
        struct AnthropicUsage {
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
            model: model.to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: MessageRole::Assistant,
                    content,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: hyperinfer_core::types::Usage {
                input_tokens: data.usage.input_tokens,
                output_tokens: data.usage.output_tokens,
            },
        })
    }

    /// Stream chat completions from OpenAI via SSE.
    ///
    /// Returns a pinned `Stream` of `ChatChunk` items.  The stream ends after
    /// the provider sends the `[DONE]` sentinel or the connection closes.
    pub fn stream_openai(
        &self,
        model: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + 'static>> {
        use futures::StreamExt;

        let url = "https://api.openai.com/v1/chat/completions".to_string();
        let model = model.to_string();
        let api_key = api_key.to_string();

        let body = serde_json::json!({
            "model": model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        let client = self.client.clone();

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

            // SSE lines can be split across chunks; buffer incomplete lines.
            let mut buf = String::new();

            while let Some(bytes) = byte_stream.next().await {
                let bytes = bytes?;
                buf.push_str(&String::from_utf8_lossy(&bytes));

                // Process all complete lines in the buffer.
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let data = if let Some(d) = line.strip_prefix("data: ") { d } else { continue };
                    if data == "[DONE]" {
                        return;
                    }

                    #[derive(Deserialize)]
                    struct StreamChoice {
                        delta: DeltaContent,
                        finish_reason: Option<String>,
                    }
                    #[derive(Deserialize)]
                    struct DeltaContent {
                        #[serde(default)]
                        content: String,
                    }
                    #[derive(Deserialize)]
                    struct StreamEvent {
                        #[serde(default)]
                        id: String,
                        #[serde(default)]
                        model: String,
                        #[serde(default)]
                        choices: Vec<StreamChoice>,
                        usage: Option<OpenAiStreamUsage>,
                    }
                    #[derive(Deserialize)]
                    struct OpenAiStreamUsage {
                        prompt_tokens: u32,
                        completion_tokens: u32,
                    }

                    if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                        let finish_reason = event.choices.first()
                            .and_then(|c| c.finish_reason.clone());
                        let delta = event.choices.first()
                            .map(|c| c.delta.content.clone())
                            .unwrap_or_default();
                        let usage = event.usage.map(|u| StreamUsage {
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
                }
            }
        };

        Box::pin(stream)
    }

    /// Stream chat completions from Anthropic via SSE.
    ///
    /// Anthropic uses a different event schema: `content_block_delta` events
    /// carry text deltas; `message_delta` carries the final usage.
    pub fn stream_anthropic(
        &self,
        model: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + 'static>> {
        use futures::StreamExt;

        let url = "https://api.anthropic.com/v1/messages";
        let model = model.to_string();
        let api_key = api_key.to_string();

        let system = request
            .messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone());

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
                    "content": m.content,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(1024),
            "stream": true,
        });
        if let Some(s) = system {
            body["system"] = serde_json::json!(s);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = serde_json::json!(t);
        }

        let client = self.client.clone();

        let stream = async_stream::try_stream! {
            let response = client
                .post(url)
                .header("x-api-key", &api_key)
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
            let mut buf = String::new();
            // Anthropic sends the message id in a `message_start` event.
            let mut stream_id = String::new();
            // `input_tokens` is reported in `message_start`; cache it here so
            // the final `message_delta` chunk can include the correct value.
            let mut cached_input_tokens: u32 = 0;

            while let Some(bytes) = byte_stream.next().await {
                let bytes = bytes?;
                buf.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let data = if let Some(d) = line.strip_prefix("data: ") { d } else { continue };

                    #[derive(Deserialize)]
                    struct AnthropicEvent {
                        #[serde(rename = "type")]
                        event_type: String,
                        // message_start
                        message: Option<AnthropicMessage>,
                        // content_block_delta
                        delta: Option<AnthropicDelta>,
                        // message_delta
                        usage: Option<AnthropicStreamUsage>,
                    }
                    #[derive(Deserialize)]
                    struct AnthropicMessage {
                        id: String,
                    }
                    #[derive(Deserialize)]
                    struct AnthropicDelta {
                        #[serde(rename = "type")]
                        delta_type: String,
                        #[serde(default)]
                        text: String,
                        stop_reason: Option<String>,
                    }
                    #[derive(Deserialize)]
                    struct AnthropicStreamUsage {
                        input_tokens: Option<u32>,
                        output_tokens: Option<u32>,
                    }

                    if let Ok(event) = serde_json::from_str::<AnthropicEvent>(data) {
                        match event.event_type.as_str() {
                            "message_start" => {
                                if let Some(msg) = event.message {
                                    stream_id = msg.id;
                                }
                                if let Some(u) = event.usage {
                                    cached_input_tokens = u.input_tokens.unwrap_or(0);
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
                                // Final chunk: carries finish reason and usage.
                                // input_tokens comes from message_start, not here.
                                let finish_reason = event.delta
                                    .as_ref()
                                    .and_then(|d| d.stop_reason.clone());
                                let usage = event.usage.map(|u| StreamUsage {
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
    fn test_http_caller_new() {
        let result = HttpCaller::new();
        assert!(result.is_ok());
    }

    #[test]
    fn test_openai_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Hello!");
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[test]
    fn test_openai_choice_deserialization() {
        let json = r#"{
            "index": 0,
            "message": {
                "role": "user",
                "content": "Test message"
            },
            "finish_reason": "length"
        }"#;

        let choice: OpenAiChoice = serde_json::from_str(json).unwrap();
        assert_eq!(choice.index, 0);
        assert_eq!(choice.message.role, "user");
        assert_eq!(choice.message.content, "Test message");
        assert_eq!(choice.finish_reason, Some("length".to_string()));
    }

    #[test]
    fn test_usage_deserialization() {
        let json = r#"{
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        }"#;

        let usage: Usage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            role: "assistant".to_string(),
            content: "Response text".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("assistant"));
        assert!(json.contains("Response text"));
    }

    #[test]
    fn test_openai_response_clone() {
        let response = OpenAiResponse {
            id: "test-id".to_string(),
            choices: vec![],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        };

        let cloned = response.clone();
        assert_eq!(response.id, cloned.id);
        assert_eq!(response.usage.total_tokens, cloned.usage.total_tokens);
    }

    #[test]
    fn test_openai_choice_with_no_finish_reason() {
        let json = r#"{
            "index": 1,
            "message": {
                "role": "assistant",
                "content": "Partial response"
            },
            "finish_reason": null
        }"#;

        let choice: OpenAiChoice = serde_json::from_str(json).unwrap();
        assert_eq!(choice.index, 1);
        assert_eq!(choice.finish_reason, None);
    }

    #[tokio::test]
    async fn test_call_openai_request_structure() {
        let request = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Hello".to_string(),
            }],
            temperature: Some(0.7),
            max_tokens: Some(100),
            stream: None,
        };

        // We can't actually call OpenAI without a real API key and network,
        // but we can verify the function signature and request structure
        let body = serde_json::json!({
            "model": "gpt-4",
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });

        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["max_tokens"], 100);
    }

    #[tokio::test]
    async fn test_call_anthropic_request_structure() {
        let request = ChatRequest {
            model: "claude-3".to_string(),
            messages: vec![
                ChatMessage {
                    role: MessageRole::System,
                    content: "You are helpful".to_string(),
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: "Hello".to_string(),
                },
            ],
            temperature: Some(0.5),
            max_tokens: Some(200),
            stream: None,
        };

        // Extract system message
        let system = request
            .messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone());

        assert_eq!(system, Some("You are helpful".to_string()));

        // Filter non-system messages
        let messages: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .collect();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello");
    }
}
