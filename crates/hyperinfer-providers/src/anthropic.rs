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
    let prompt_caching = request.prompt_caching == Some(true);

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
                if let Some(ref parts) = m.content_parts {
                    if !parts.is_empty() {
                        let blocks: Vec<serde_json::Value> = parts
                            .iter()
                            .map(|p| match p {
                                hyperinfer_core::ContentPart::Text { text } => {
                                    serde_json::json!({ "type": "text", "text": text })
                                }
                                hyperinfer_core::ContentPart::ImageUrl { image_url } => {
                                    if let Some((media_type, data)) =
                                        crate::parse_data_url(&image_url.url)
                                    {
                                        serde_json::json!({
                                            "type": "image",
                                            "source": {
                                                "type": "base64",
                                                "media_type": media_type,
                                                "data": data
                                            }
                                        })
                                    } else {
                                        serde_json::json!({
                                            "type": "image",
                                            "source": {
                                                "type": "url",
                                                "url": image_url.url
                                            }
                                        })
                                    }
                                }
                            })
                            .collect();
                        messages.push(serde_json::json!({
                            "role": "user",
                            "content": blocks,
                        }));
                    } else {
                        messages.push(serde_json::json!({
                            "role": "user",
                            "content": m.content,
                        }));
                    }
                } else {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": m.content,
                    }));
                }
                i += 1;
            }
        }
    }

    // Prompt caching: attach cache_control to last content block of last non-system message
    if prompt_caching {
        if let Some(last_msg) = messages.last_mut() {
            if let Some(content) = last_msg.get_mut("content") {
                if content.is_string() {
                    let text = content.as_str().unwrap().to_string();
                    *content = serde_json::json!([{
                        "type": "text",
                        "text": text,
                        "cache_control": {"type": "ephemeral"}
                    }]);
                } else if let Some(arr) = content.as_array_mut() {
                    if let Some(last_block) = arr.last_mut() {
                        last_block["cache_control"] = serde_json::json!({"type": "ephemeral"});
                    }
                }
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

    // Prompt caching: emit cache_control breakpoints on system and last tool
    if prompt_caching {
        // System prompt as content block with cache_control
        if let Some(s) = &system {
            body.insert(
                "system".to_string(),
                serde_json::json!([{
                    "type": "text",
                    "text": s,
                    "cache_control": {"type": "ephemeral"}
                }]),
            );
        }

        // Tools: attach cache_control to last tool definition
        if let Some(tools) = &request.tools {
            if !tools.is_empty() {
                let anthropic_tools: Vec<serde_json::Value> = tools
                    .iter()
                    .enumerate()
                    .map(|(idx, t)| {
                        let is_last = idx == tools.len() - 1;
                        if is_last {
                            serde_json::json!({
                                "name": t.name,
                                "description": t.description,
                                "input_schema": t.parameters,
                                "cache_control": {"type": "ephemeral"}
                            })
                        } else {
                            serde_json::json!({
                                "name": t.name,
                                "description": t.description,
                                "input_schema": t.parameters,
                            })
                        }
                    })
                    .collect();
                body.insert("tools".to_string(), serde_json::json!(anthropic_tools));
            }
        }
    } else {
        // Original behavior without prompt caching
        if let Some(s) = &system {
            body.insert("system".to_string(), serde_json::json!(s));
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
    }

    if let Some(t) = request.temperature {
        body.insert("temperature".to_string(), serde_json::json!(t));
    }
    if let Some(stop) = &request.stop {
        body.insert("stop_sequences".to_string(), serde_json::json!(stop));
    }

    // Translate OpenAI-style tool_choice to Anthropic format
    if let Some(ref tool_choice) = request.tool_choice {
        let anthropic_tool_choice = if tool_choice == &serde_json::json!("none") {
            serde_json::json!({ "type": "none" })
        } else if tool_choice == &serde_json::json!("auto") {
            serde_json::json!({ "type": "auto" })
        } else if tool_choice == &serde_json::json!("required") {
            serde_json::json!({ "type": "any" })
        } else if let Some(obj) = tool_choice.as_object() {
            if obj.get("type") == Some(&serde_json::json!("function")) {
                if let Some(name) = obj.get("function").and_then(|f| f.get("name")) {
                    serde_json::json!({ "type": "tool", "name": name })
                } else {
                    serde_json::json!({ "type": "auto" })
                }
            } else {
                serde_json::json!({ "type": "auto" })
            }
        } else {
            serde_json::json!({ "type": "auto" })
        };
        body.insert("tool_choice".to_string(), anthropic_tool_choice);
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

    async fn list_models(
        &self,
        api_key: &str,
    ) -> Result<Vec<hyperinfer_core::DiscoveredModel>, HyperInferError> {
        let url = format!("{}/v1/models", self.base_url.trim_end_matches('/'));
        let resp = self
            .http_client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(HyperInferError::ApiError { status, message });
        }

        let body: serde_json::Value = resp.json().await?;

        let mut models = Vec::new();
        if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
            for item in arr {
                let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
                    continue;
                };
                let display_name = item
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                models.push(hyperinfer_core::DiscoveredModel {
                    id: id.to_string(),
                    provider: "anthropic".to_string(),
                    display_name,
                    context_window: None,
                    pricing: None,
                });
            }
        }
        Ok(models)
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
            #[serde(default)]
            cache_read_input_tokens: Option<u32>,
            #[serde(default)]
            cache_creation_input_tokens: Option<u32>,
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
                    content_parts: None,
                },
                finish_reason: data.stop_reason,
            }],
            usage: Usage {
                input_tokens: data.usage.input_tokens,
                output_tokens: data.usage.output_tokens,
                cache_read_tokens: data.usage.cache_read_input_tokens,
                cache_creation_tokens: data.usage.cache_creation_input_tokens,
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
            let mut stream_cache_read_tokens: Option<u32> = None;
            let mut stream_cache_creation_tokens: Option<u32> = None;

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
                        #[serde(default)]
                        cache_read_input_tokens: Option<u32>,
                        #[serde(default)]
                        cache_creation_input_tokens: Option<u32>,
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
                                        stream_cache_read_tokens = u.cache_read_input_tokens;
                                        stream_cache_creation_tokens = u.cache_creation_input_tokens;
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
                                    cache_read_tokens: u
                                        .cache_read_input_tokens
                                        .or(stream_cache_read_tokens),
                                    cache_creation_tokens: u
                                        .cache_creation_input_tokens
                                        .or(stream_cache_creation_tokens),
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

    #[test]
    fn test_anthropic_tool_choice_translation() {
        let req_auto = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![ChatMessage::user("Hello")],
            tool_choice: Some(serde_json::json!("auto")),
            ..Default::default()
        };
        let (_s, _m, body_auto) = build_anthropic_request_body(&req_auto, false);
        assert_eq!(body_auto["tool_choice"]["type"], "auto");

        let req_required = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![ChatMessage::user("Hello")],
            tool_choice: Some(serde_json::json!("required")),
            ..Default::default()
        };
        let (_s, _m, body_required) = build_anthropic_request_body(&req_required, false);
        assert_eq!(body_required["tool_choice"]["type"], "any");

        let req_specific = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![ChatMessage::user("Hello")],
            tool_choice: Some(
                serde_json::json!({"type": "function", "function": {"name": "read_file"}}),
            ),
            ..Default::default()
        };
        let (_s, _m, body_specific) = build_anthropic_request_body(&req_specific, false);
        assert_eq!(body_specific["tool_choice"]["type"], "tool");
        assert_eq!(body_specific["tool_choice"]["name"], "read_file");

        let req_none = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![ChatMessage::user("Hello")],
            tool_choice: Some(serde_json::json!("none")),
            ..Default::default()
        };
        let (_s, _m, body_none) = build_anthropic_request_body(&req_none, false);
        assert_eq!(body_none["tool_choice"]["type"], "none");
    }

    #[test]
    fn test_anthropic_multimodal_content_parts() {
        let req = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                content_parts: Some(vec![
                    hyperinfer_core::ContentPart::Text {
                        text: "Describe this:".to_string(),
                    },
                    hyperinfer_core::ContentPart::ImageUrl {
                        image_url: hyperinfer_core::ImageUrl {
                            url: "https://example.com/img.png".to_string(),
                            detail: None,
                        },
                    },
                ]),
            }],
            ..Default::default()
        };
        let (_s, messages, _body) = build_anthropic_request_body(&req, false);
        let content = &messages[0]["content"];
        assert!(content.is_array());
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Describe this:");
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["type"], "url");
        assert_eq!(content[1]["source"]["url"], "https://example.com/img.png");
    }

    #[test]
    fn test_anthropic_base64_image() {
        let req = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                content_parts: Some(vec![hyperinfer_core::ContentPart::ImageUrl {
                    image_url: hyperinfer_core::ImageUrl {
                        url: "data:image/png;base64,SGVsbG8=".to_string(),
                        detail: None,
                    },
                }]),
            }],
            ..Default::default()
        };
        let (_s, messages, _body) = build_anthropic_request_body(&req, false);
        let content = &messages[0]["content"];
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["type"], "base64");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[0]["source"]["data"], "SGVsbG8=");
    }

    #[test]
    fn test_anthropic_prompt_caching_enabled() {
        let req = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![
                ChatMessage::system("You are a coding assistant."),
                ChatMessage::user("Read Cargo.toml"),
            ],
            tools: Some(vec![
                ToolDefinition {
                    name: "read_file".to_string(),
                    description: "Read a file".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": { "path": { "type": "string" } }
                    }),
                },
                ToolDefinition {
                    name: "write_file".to_string(),
                    description: "Write a file".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": { "path": { "type": "string" } }
                    }),
                },
            ]),
            prompt_caching: Some(true),
            ..Default::default()
        };

        let (_system, _messages, body) = build_anthropic_request_body(&req, false);

        // System should be a content block array with cache_control
        let system = &body["system"];
        assert!(system.is_array());
        assert_eq!(system[0]["type"], "text");
        assert_eq!(system[0]["text"], "You are a coding assistant.");
        assert_eq!(system[0]["cache_control"]["type"], "ephemeral");

        // Last tool should have cache_control
        assert_eq!(body["tools"][0].get("cache_control"), None);
        assert_eq!(body["tools"][1]["cache_control"]["type"], "ephemeral");

        // Last message content should have cache_control
        let last_msg = &body["messages"][0];
        let content = &last_msg["content"];
        assert!(content.is_array());
        assert_eq!(content[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn test_anthropic_prompt_caching_disabled() {
        let req = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![
                ChatMessage::system("You are a coding assistant."),
                ChatMessage::user("Read Cargo.toml"),
            ],
            tools: Some(vec![ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } }
                }),
            }]),
            prompt_caching: None,
            ..Default::default()
        };

        let (_system, _messages, body) = build_anthropic_request_body(&req, false);

        // System should be a plain string
        assert_eq!(body["system"], "You are a coding assistant.");

        // No cache_control on tools
        assert_eq!(body["tools"][0].get("cache_control"), None);

        // No cache_control on message content
        let last_msg = &body["messages"][0];
        assert_eq!(last_msg["content"], "Read Cargo.toml");
    }
}
