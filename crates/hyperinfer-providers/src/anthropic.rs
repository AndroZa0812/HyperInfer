use super::provider_trait::LlmProvider;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Choice, HyperInferError, MessageRole,
    ToolCall, Usage,
};
use reqwest::Client;
use std::pin::Pin;

pub struct AnthropicProvider {
    http_client: Client,
    base_url: String,
}

pub(crate) fn build_anthropic_request_body(
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

    let mut messages: Vec<serde_json::Value> = Vec::new();
    let non_system: Vec<&ChatMessage> = request
        .messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .collect();

    let mut i = 0;
    while i < non_system.len() {
        let m = non_system[i];
        match m.role {
            MessageRole::Tool => {
                // Coalesce consecutive tool results into a single `user` message with `tool_result` blocks.
                let mut blocks = Vec::new();
                while i < non_system.len() && non_system[i].role == MessageRole::Tool {
                    let tm = non_system[i];
                    blocks.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tm.tool_call_id.clone().unwrap_or_default(),
                        "content": tm.content,
                    }));
                    i += 1;
                }
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": blocks,
                }));
            }
            MessageRole::Assistant => {
                if let Some(ref calls) = m.tool_calls {
                    let mut blocks = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(serde_json::json!({
                            "type": "text",
                            "text": m.content,
                        }));
                    }
                    for tc in calls {
                        let input: serde_json::Value = serde_json::from_str(&tc.arguments)
                            .unwrap_or_else(|_| serde_json::json!({}));
                        blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": input,
                        }));
                    }
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": blocks,
                    }));
                } else {
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": m.content,
                    }));
                }
                i += 1;
            }
            _ => {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": m.content,
                }));
                i += 1;
            }
        }
    }

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
    if let Some(tools) = &request.tools {
        if !tools.is_empty() {
            let anthropic_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters,
                    })
                })
                .collect();
            body.insert("tools".to_string(), serde_json::json!(anthropic_tools));
        }
    }

    (system, messages, body)
}

impl AnthropicProvider {
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_base_url("https://api.anthropic.com")
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }
}

// Clone is required by LlmProvider supertrait. The HTTP client is cheap to clone.
impl Clone for AnthropicProvider {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn base_url(&self) -> &str {
        &self.base_url
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
            #[serde(default, rename = "type")]
            block_type: String,
            text: Option<String>,
            id: Option<String>,
            name: Option<String>,
            input: Option<serde_json::Value>,
        }

        #[derive(serde::Deserialize)]
        struct AnthropicUsageDetail {
            input_tokens: u32,
            output_tokens: u32,
        }

        let data: AnthropicResponse = response.json().await?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();
        for b in data.content {
            if b.block_type == "tool_use" {
                if let (Some(id), Some(name)) = (b.id, b.name) {
                    let arguments = b
                        .input
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "{}".to_string());
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
            } else if let Some(text) = b.text {
                text_parts.push(text);
            }
        }

        let content = text_parts.join("\n");

        Ok(ChatResponse {
            id: data.id,
            model: request.model.clone(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: MessageRole::Assistant,
                    content,
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    tool_call_id: None,
                    name: None,
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
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + 'static>> {
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
                                            tool_calls: None,
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
                                    tool_calls: None,
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
    use hyperinfer_core::ToolDefinition;

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

    #[test]
    fn test_anthropic_tool_request_translation() {
        let req = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![
                ChatMessage::system("You are a coding assistant."),
                ChatMessage::user("Read Cargo.toml"),
                ChatMessage::assistant_with_tools(
                    "Checking file",
                    vec![ToolCall {
                        id: "toolu_01".to_string(),
                        name: "read_file".to_string(),
                        arguments: r#"{"path":"Cargo.toml"}"#.to_string(),
                    }],
                ),
                ChatMessage::tool_result("toolu_01", "read_file", "[workspace]"),
            ],
            tools: Some(vec![ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } }
                }),
            }]),
            ..Default::default()
        };

        let (_system, messages, body) = build_anthropic_request_body(&req, false);
        assert_eq!(body["system"], "You are a coding assistant.");
        assert_eq!(body["tools"][0]["name"], "read_file");
        assert!(body["tools"][0].get("input_schema").is_some());
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][1]["type"], "tool_use");
        assert_eq!(messages[1]["content"][1]["id"], "toolu_01");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
        assert_eq!(messages[2]["content"][0]["tool_use_id"], "toolu_01");
    }
}
