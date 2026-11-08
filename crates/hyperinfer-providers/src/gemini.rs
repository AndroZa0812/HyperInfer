use super::provider_trait::LlmProvider;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Choice, HyperInferError, MessageRole,
    ToolCall, Usage,
};
use reqwest::Client;
use std::pin::Pin;

/// Google Gemini provider (`generativelanguage.googleapis.com`).
pub struct GeminiProvider {
    http_client: Client,
    name: String,
    base_url: String,
    /// When true, uses the Cloud Code / Antigravity `/v1internal:generateContent`
    /// envelope and Bearer OAuth header conventions.
    antigravity_mode: bool,
}

impl GeminiProvider {
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_base_url("https://generativelanguage.googleapis.com")
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            name: "gemini".to_string(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            antigravity_mode: false,
        })
    }

    /// Construct the Antigravity provider (`cloudcode-pa.googleapis.com`).
    pub fn new_antigravity() -> Result<Self, reqwest::Error> {
        Self::antigravity_with_base_url("https://cloudcode-pa.googleapis.com")
    }

    pub fn antigravity_with_base_url(base_url: impl Into<String>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            name: "antigravity".to_string(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            antigravity_mode: true,
        })
    }
}

impl Clone for GeminiProvider {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            name: self.name.clone(),
            base_url: self.base_url.clone(),
            antigravity_mode: self.antigravity_mode,
        }
    }
}

pub(crate) fn build_gemini_request_body(request: &ChatRequest) -> serde_json::Value {
    let system_messages: Vec<&str> = request
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .map(|m| m.content.as_str())
        .collect();

    let non_system: Vec<&ChatMessage> = request
        .messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .collect();

    let mut contents: Vec<serde_json::Value> = Vec::new();
    let mut i = 0;
    while i < non_system.len() {
        let m = non_system[i];
        match m.role {
            MessageRole::Tool => {
                let mut parts = Vec::new();
                while i < non_system.len() && non_system[i].role == MessageRole::Tool {
                    let tm = non_system[i];
                    let fn_name = tm
                        .name
                        .clone()
                        .or_else(|| tm.tool_call_id.clone())
                        .unwrap_or_else(|| "tool".to_string());
                    parts.push(serde_json::json!({
                        "functionResponse": {
                            "name": fn_name,
                            "response": {
                                "result": tm.content,
                            }
                        }
                    }));
                    i += 1;
                }
                contents.push(serde_json::json!({
                    "role": "user",
                    "parts": parts,
                }));
            }
            MessageRole::Assistant => {
                let mut parts = Vec::new();
                if !m.content.is_empty() {
                    parts.push(serde_json::json!({ "text": m.content }));
                }
                if let Some(ref calls) = m.tool_calls {
                    for tc in calls {
                        let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                            .unwrap_or_else(|_| serde_json::json!({}));
                        parts.push(serde_json::json!({
                            "functionCall": {
                                "name": tc.name,
                                "args": args,
                            }
                        }));
                    }
                }
                contents.push(serde_json::json!({
                    "role": "model",
                    "parts": parts,
                }));
                i += 1;
            }
            _ => {
                contents.push(serde_json::json!({
                    "role": "user",
                    "parts": [{ "text": m.content }],
                }));
                i += 1;
            }
        }
    }

    let mut body = serde_json::Map::new();
    body.insert("contents".to_string(), serde_json::json!(contents));

    if !system_messages.is_empty() {
        body.insert(
            "systemInstruction".to_string(),
            serde_json::json!({
                "parts": [{ "text": system_messages.join("\n") }]
            }),
        );
    }

    let mut gen_config = serde_json::Map::new();
    if let Some(temp) = request.temperature {
        gen_config.insert("temperature".to_string(), serde_json::json!(temp));
    }
    if let Some(max_tokens) = request.max_tokens {
        gen_config.insert("maxOutputTokens".to_string(), serde_json::json!(max_tokens));
    }
    if let Some(ref stop) = request.stop {
        gen_config.insert("stopSequences".to_string(), serde_json::json!(stop));
    }
    if !gen_config.is_empty() {
        body.insert(
            "generationConfig".to_string(),
            serde_json::Value::Object(gen_config),
        );
    }

    if let Some(ref tools) = request.tools {
        if !tools.is_empty() {
            let declarations: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    })
                })
                .collect();
            body.insert(
                "tools".to_string(),
                serde_json::json!([{ "functionDeclarations": declarations }]),
            );
        }
    }

    serde_json::Value::Object(body)
}

pub(crate) fn wrap_request_body(
    request: &ChatRequest,
    antigravity_mode: bool,
) -> serde_json::Value {
    let inner = build_gemini_request_body(request);
    if antigravity_mode {
        serde_json::json!({
            "model": request.model,
            "request": inner,
        })
    } else {
        inner
    }
}

#[derive(serde::Deserialize, Default)]
#[allow(non_snake_case)]
struct GeminiGenerateResponse {
    #[serde(default)]
    responseId: Option<String>,
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usageMetadata: Option<GeminiUsageMetadata>,
    /// Antigravity `/v1internal:generateContent` wraps the Gemini payload under `response`.
    #[serde(default)]
    response: Option<Box<GeminiGenerateResponse>>,
}

#[derive(serde::Deserialize, Default)]
#[allow(non_snake_case)]
struct GeminiCandidate {
    #[serde(default)]
    content: Option<GeminiContent>,
    #[serde(default)]
    finishReason: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(serde::Deserialize, Default)]
#[allow(non_snake_case)]
struct GeminiPart {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    functionCall: Option<GeminiFunctionCall>,
}

#[derive(serde::Deserialize)]
struct GeminiFunctionCall {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    args: serde_json::Value,
}

#[derive(serde::Deserialize, Default)]
#[allow(non_snake_case)]
struct GeminiUsageMetadata {
    #[serde(default)]
    promptTokenCount: u32,
    #[serde(default)]
    candidatesTokenCount: u32,
}

pub(crate) fn parse_gemini_response(
    raw_value: serde_json::Value,
    model: &str,
) -> Result<ChatResponse, HyperInferError> {
    let raw_str = raw_value.to_string();
    let mut parsed: GeminiGenerateResponse =
        serde_json::from_value(raw_value).map_err(|e| HyperInferError::StreamParse {
            message: e.to_string(),
            raw: raw_str,
        })?;
    let data = parsed.response.take().map(|b| *b).unwrap_or(parsed);

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut finish_reason = None;

    if let Some(candidate) = data.candidates.into_iter().next() {
        finish_reason = candidate.finishReason;
        if let Some(content) = candidate.content {
            for (idx, part) in content.parts.into_iter().enumerate() {
                if let Some(fc) = part.functionCall {
                    let id = fc.id.unwrap_or_else(|| format!("call_{}_{}", fc.name, idx));
                    let arguments = if fc.args.is_null() {
                        "{}".to_string()
                    } else {
                        fc.args.to_string()
                    };
                    tool_calls.push(ToolCall {
                        id,
                        name: fc.name,
                        arguments,
                    });
                } else if let Some(t) = part.text {
                    text_parts.push(t);
                }
            }
        }
    }

    let usage = data
        .usageMetadata
        .map(|u| Usage {
            input_tokens: u.promptTokenCount,
            output_tokens: u.candidatesTokenCount,
        })
        .unwrap_or_default();

    Ok(ChatResponse {
        id: data.responseId.unwrap_or_else(|| "gemini-resp".to_string()),
        model: model.to_string(),
        choices: vec![Choice {
            index: 0,
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: text_parts.join(""),
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id: None,
                name: None,
            },
            finish_reason,
        }],
        usage,
    })
}

#[async_trait]
impl LlmProvider for GeminiProvider {
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
        let url = if self.antigravity_mode {
            format!("{}/v1internal:generateContent", self.base_url)
        } else {
            format!(
                "{}/v1beta/models/{}:generateContent",
                self.base_url, request.model
            )
        };

        let body = wrap_request_body(request, self.antigravity_mode);

        let mut req_builder = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json");

        if self.antigravity_mode || api_key.starts_with("ya29.") {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        } else if !api_key.is_empty() {
            req_builder = req_builder.header("x-goog-api-key", api_key);
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

        let raw_json: serde_json::Value = response.json().await?;
        parse_gemini_response(raw_json, &request.model)
    }

    fn stream(
        &self,
        request: &ChatRequest,
        api_key: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + 'static>> {
        let url = if self.antigravity_mode {
            format!("{}/v1internal:streamGenerateContent?alt=sse", self.base_url)
        } else {
            format!(
                "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
                self.base_url, request.model
            )
        };

        let body = wrap_request_body(request, self.antigravity_mode);
        let client = self.http_client.clone();
        let api_key = api_key.to_string();
        let model = request.model.clone();
        let antigravity_mode = self.antigravity_mode;

        let stream = async_stream::try_stream! {
            let mut req_builder = client
                .post(&url)
                .header("Content-Type", "application/json");

            if antigravity_mode || api_key.starts_with("ya29.") {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
            } else if !api_key.is_empty() {
                req_builder = req_builder.header("x-goog-api-key", api_key);
            }

            let response = req_builder.json(&body).send().await?;

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
                    let data = if let Some(d) = line.strip_prefix("data: ") {
                        d.to_owned()
                    } else {
                        continue;
                    };
                    if data == "[DONE]" {
                        return;
                    }

                    match serde_json::from_str::<serde_json::Value>(&data) {
                        Ok(val) => {
                            let chat_resp = parse_gemini_response(val, &model)?;
                            let first = chat_resp.choices.into_iter().next();
                            let (delta, finish_reason, tool_calls) = match first {
                                Some(c) => (c.message.content, c.finish_reason, c.message.tool_calls),
                                None => (String::new(), None, None),
                            };
                            let usage = if chat_resp.usage.input_tokens > 0
                                || chat_resp.usage.output_tokens > 0
                            {
                                Some(chat_resp.usage)
                            } else {
                                None
                            };
                            yield ChatChunk {
                                id: chat_resp.id,
                                model: model.clone(),
                                delta,
                                finish_reason,
                                usage,
                                tool_calls,
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
    fn test_gemini_and_antigravity_provider_metadata() {
        let gemini = GeminiProvider::new().unwrap();
        assert_eq!(gemini.name(), "gemini");
        assert_eq!(
            gemini.base_url(),
            "https://generativelanguage.googleapis.com"
        );
        assert!(gemini.supports_streaming());

        let agy = GeminiProvider::new_antigravity().unwrap();
        assert_eq!(agy.name(), "antigravity");
        assert_eq!(agy.base_url(), "https://cloudcode-pa.googleapis.com");
        assert!(agy.supports_streaming());
    }

    #[test]
    fn test_gemini_and_antigravity_tool_request_translation() {
        let req = ChatRequest {
            model: "gemini-2.5-pro".to_string(),
            messages: vec![
                ChatMessage::system("Be concise."),
                ChatMessage::user("Read Cargo.toml"),
                ChatMessage::assistant_with_tools(
                    "",
                    vec![ToolCall {
                        id: "call_1".to_string(),
                        name: "read_file".to_string(),
                        arguments: r#"{"path":"Cargo.toml"}"#.to_string(),
                    }],
                ),
                ChatMessage::tool_result("call_1", "read_file", "[workspace]"),
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

        let gemini_body = wrap_request_body(&req, false);
        assert_eq!(
            gemini_body["systemInstruction"]["parts"][0]["text"],
            "Be concise."
        );
        assert_eq!(
            gemini_body["tools"][0]["functionDeclarations"][0]["name"],
            "read_file"
        );
        assert_eq!(gemini_body["contents"][1]["role"], "model");
        assert_eq!(
            gemini_body["contents"][1]["parts"][0]["functionCall"]["name"],
            "read_file"
        );
        assert_eq!(gemini_body["contents"][2]["role"], "user");
        assert_eq!(
            gemini_body["contents"][2]["parts"][0]["functionResponse"]["name"],
            "read_file"
        );

        let agy_body = wrap_request_body(&req, true);
        assert_eq!(agy_body["model"], "gemini-2.5-pro");
        assert!(agy_body.get("request").is_some());
    }

    #[test]
    fn test_parse_gemini_and_antigravity_tool_response() {
        // Test Antigravity enveloped response with functionCall
        let agy_json = serde_json::json!({
            "response": {
                "responseId": "resp_123",
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [
                            { "text": "I will read the file." },
                            {
                                "functionCall": {
                                    "name": "read_file",
                                    "args": { "path": "src/lib.rs" }
                                }
                            }
                        ]
                    },
                    "finishReason": "STOP"
                }],
                "usageMetadata": {
                    "promptTokenCount": 42,
                    "candidatesTokenCount": 15
                }
            }
        });

        let parsed = parse_gemini_response(agy_json, "gemini-2.5-pro").unwrap();
        assert_eq!(parsed.id, "resp_123");
        assert_eq!(parsed.usage.input_tokens, 42);
        assert_eq!(parsed.usage.output_tokens, 15);
        let msg = &parsed.choices[0].message;
        assert_eq!(msg.content, "I will read the file.");
        let calls = msg.tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].arguments, r#"{"path":"src/lib.rs"}"#);
    }
}
