use super::provider_trait::LlmProvider;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Choice, HyperInferError, MessageRole,
    ToolCall, Usage,
};
use reqwest::Client;
use std::pin::Pin;

pub struct OpenAiProvider {
    http_client: Client,
    name: String,
    base_url: String,
}

impl OpenAiProvider {
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_name_and_base_url("openai", "https://api.openai.com")
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Result<Self, reqwest::Error> {
        Self::with_name_and_base_url("openai", base_url)
    }

    pub fn with_name_and_base_url(
        name: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self, reqwest::Error> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Ok(Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            name: name.into(),
            base_url,
        })
    }
}

// Clone is required by LlmProvider supertrait. The HTTP client is cheap to clone.
impl Clone for OpenAiProvider {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            name: self.name.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

pub(crate) fn chat_request_to_openai_body(request: &ChatRequest) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), serde_json::json!(request.model));

    let messages: Vec<serde_json::Value> = request
        .messages
        .iter()
        .map(|m| {
            let mut msg = serde_json::Map::new();
            msg.insert("role".to_string(), serde_json::json!(m.role));
            if let Some(ref calls) = m.tool_calls {
                if m.content.is_empty() {
                    msg.insert("content".to_string(), serde_json::Value::Null);
                } else {
                    msg.insert("content".to_string(), serde_json::json!(m.content));
                }
                let oai_calls: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments,
                            }
                        })
                    })
                    .collect();
                msg.insert("tool_calls".to_string(), serde_json::json!(oai_calls));
            } else {
                msg.insert("content".to_string(), serde_json::json!(m.content));
            }
            if let Some(ref tool_call_id) = m.tool_call_id {
                msg.insert("tool_call_id".to_string(), serde_json::json!(tool_call_id));
            }
            if let Some(ref name) = m.name {
                msg.insert("name".to_string(), serde_json::json!(name));
            }
            serde_json::Value::Object(msg)
        })
        .collect();

    body.insert("messages".to_string(), serde_json::json!(messages));
    if let Some(temperature) = request.temperature {
        body.insert("temperature".to_string(), serde_json::json!(temperature));
    }
    if let Some(max_tokens) = request.max_tokens {
        body.insert("max_tokens".to_string(), serde_json::json!(max_tokens));
    }
    if let Some(stop) = &request.stop {
        body.insert("stop".to_string(), serde_json::json!(stop));
    }
    if let Some(tools) = &request.tools {
        if !tools.is_empty() {
            let oai_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body.insert("tools".to_string(), serde_json::json!(oai_tools));
        }
    }
    serde_json::Value::Object(body)
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn chat(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Result<ChatResponse, HyperInferError> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = chat_request_to_openai_body(request);

        let mut req_builder = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json");
        if !api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder.json(&body).send().await?;

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
            #[serde(default)]
            id: String,
            choices: Vec<OpenAiChoice>,
            #[serde(default)]
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
            #[serde(default)]
            content: Option<String>,
            #[serde(default)]
            tool_calls: Option<Vec<OpenAiToolCall>>,
        }

        #[derive(serde::Deserialize)]
        struct OpenAiToolCall {
            id: String,
            function: OpenAiFunctionCall,
        }

        #[derive(serde::Deserialize)]
        struct OpenAiFunctionCall {
            name: String,
            arguments: String,
        }

        #[derive(serde::Deserialize, Default)]
        #[allow(dead_code)]
        struct UsageDetail {
            #[serde(default)]
            prompt_tokens: u32,
            #[serde(default)]
            completion_tokens: u32,
            #[serde(default)]
            total_tokens: u32,
        }

        let data: OpenAiResponse = response.json().await?;

        Ok(ChatResponse {
            id: data.id,
            model: request.model.clone(),
            choices: data
                .choices
                .into_iter()
                .map(|c| {
                    let tool_calls = c.message.tool_calls.map(|calls| {
                        calls
                            .into_iter()
                            .map(|tc| ToolCall {
                                id: tc.id,
                                name: tc.function.name,
                                arguments: tc.function.arguments,
                            })
                            .collect()
                    });
                    Choice {
                        index: c.index,
                        message: ChatMessage {
                            role: match c.message.role.as_str() {
                                "assistant" => MessageRole::Assistant,
                                "user" => MessageRole::User,
                                "system" => MessageRole::System,
                                "tool" => MessageRole::Tool,
                                other => {
                                    tracing::warn!(
                                        "Unknown OpenAI role '{}', defaulting to Assistant",
                                        other
                                    );
                                    MessageRole::Assistant
                                }
                            },
                            content: c.message.content.unwrap_or_default(),
                            tool_calls,
                            tool_call_id: None,
                            name: None,
                        },
                        finish_reason: c.finish_reason,
                    }
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
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + 'static>> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let mut body_val = chat_request_to_openai_body(request);
        if let Some(body) = body_val.as_object_mut() {
            body.insert("stream".to_string(), serde_json::json!(true));
            body.insert(
                "stream_options".to_string(),
                serde_json::json!({ "include_usage": true }),
            );
        }
        let body = body_val;
        let client = self.http_client.clone();
        let api_key = api_key.to_string();

        let stream = async_stream::try_stream! {
            let mut req_builder = client
                .post(&url)
                .header("Content-Type", "application/json");
            if !api_key.is_empty() {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
            }
            let response = req_builder
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
                        content: Option<String>,
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
                                .and_then(|c| c.delta.content.clone())
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
                                tool_calls: None,
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
    use hyperinfer_core::ToolDefinition;

    #[test]
    fn test_openai_provider_name() {
        let provider = OpenAiProvider::new().unwrap();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_provider_base_url() {
        let provider = OpenAiProvider::new().unwrap();
        assert_eq!(provider.base_url(), "https://api.openai.com");

        let custom =
            OpenAiProvider::with_name_and_base_url("ollama", "http://localhost:11434/").unwrap();
        assert_eq!(custom.name(), "ollama");
        assert_eq!(custom.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_openai_provider_supports_streaming() {
        let provider = OpenAiProvider::new().unwrap();
        assert!(provider.supports_streaming());
    }

    #[test]
    fn test_openai_tool_request_translation() {
        let req = ChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![
                ChatMessage::user("Read Cargo.toml"),
                ChatMessage::assistant_with_tools(
                    "",
                    vec![ToolCall {
                        id: "call_abc".to_string(),
                        name: "read_file".to_string(),
                        arguments: r#"{"path":"Cargo.toml"}"#.to_string(),
                    }],
                ),
                ChatMessage::tool_result("call_abc", "read_file", "[workspace]"),
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

        let body = chat_request_to_openai_body(&req);
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
        assert_eq!(body["messages"][1]["tool_calls"][0]["id"], "call_abc");
        assert_eq!(body["messages"][2]["role"], "tool");
        assert_eq!(body["messages"][2]["tool_call_id"], "call_abc");
    }
}
