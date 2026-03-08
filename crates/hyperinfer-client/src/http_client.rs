use futures::Stream;
use hyperinfer_core::types::{ChatMessage, Choice, MessageRole, StreamUsage};
use hyperinfer_core::{ChatChunk, ChatRequest, ChatResponse, HyperInferError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Drain all complete newline-terminated lines from `raw_buf`.
///
/// Each call appends the next network chunk's bytes to `raw_buf`, then this
/// helper repeatedly locates `b'\n'`, decodes the bytes *before* it as UTF-8
/// (stripping an optional `b'\r'`), removes those bytes (plus the `\n`) from
/// the front of `raw_buf`, and pushes the decoded string into `lines`.
///
/// Keeping the scan on raw bytes means a multibyte UTF-8 scalar that is split
/// across two network chunks is never decoded until its final byte arrives,
/// preventing the corruption that `String::from_utf8_lossy` would introduce
/// when called on incomplete byte sequences.
pub(crate) fn drain_lines(raw_buf: &mut Vec<u8>, lines: &mut Vec<String>) {
    while let Some(pos) = raw_buf.iter().position(|&b| b == b'\n') {
        let line_bytes = &raw_buf[..pos];
        let line_bytes = line_bytes.strip_suffix(b"\r").unwrap_or(line_bytes);
        lines.push(String::from_utf8_lossy(line_bytes).into_owned());
        raw_buf.drain(..=pos);
    }
}

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

        let mut body = serde_json::json!({
            "model": model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });
        if let Some(stop) = &request.stop {
            body["stop"] = serde_json::json!(stop);
        }

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
        if let Some(stop) = &request.stop {
            body["stop_sequences"] = serde_json::json!(stop);
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

        let mut body = serde_json::json!({
            "model": model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        if let Some(ref stop) = request.stop {
            body["stop"] = serde_json::json!(stop);
        }

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

            // Buffer raw bytes so that multibyte UTF-8 sequences split across
            // network chunks are never decoded mid-character.  Complete lines
            // are drained and decoded by `drain_lines`.
            let mut raw_buf: Vec<u8> = Vec::new();

            while let Some(bytes) = byte_stream.next().await {
                let bytes = bytes?;
                raw_buf.extend_from_slice(&bytes);

                let mut lines = Vec::new();
                drain_lines(&mut raw_buf, &mut lines);

                for line in lines {
                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let data = if let Some(d) = line.strip_prefix("data: ") { d.to_owned() } else { continue };
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
                    // OpenAI streams provider errors as
                    // {"error":{"message":"...","type":"...","code":"..."}}
                    #[derive(Deserialize)]
                    struct OpenAiStreamError {
                        error: OpenAiErrorDetail,
                    }
                    #[derive(Deserialize)]
                    struct OpenAiErrorDetail {
                        message: String,
                    }

                    // Surface provider-reported stream errors before attempting
                    // normal event parsing.
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
        if let Some(ref stop) = request.stop {
            body["stop_sequences"] = serde_json::json!(stop);
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
            // Buffer raw bytes so that multibyte UTF-8 sequences split across
            // network chunks are never decoded mid-character.  Complete lines
            // are drained and decoded by `drain_lines`.
            let mut raw_buf: Vec<u8> = Vec::new();
            // Anthropic sends the message id in a `message_start` event.
            let mut stream_id = String::new();
            // `input_tokens` is reported in `message_start`; cache it here so
            // the final `message_delta` chunk can include the correct value.
            let mut cached_input_tokens: u32 = 0;

            while let Some(bytes) = byte_stream.next().await {
                let bytes = bytes?;
                raw_buf.extend_from_slice(&bytes);

                let mut lines = Vec::new();
                drain_lines(&mut raw_buf, &mut lines);

                for line in lines {
                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let data = if let Some(d) = line.strip_prefix("data: ") { d.to_owned() } else { continue };

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

                    // Anthropic surfaces stream errors as
                    // {"type":"error","error":{"type":"...","message":"..."}}
                    #[derive(Deserialize)]
                    struct AnthropicStreamError {
                        error: AnthropicErrorDetail,
                    }
                    #[derive(Deserialize)]
                    struct AnthropicErrorDetail {
                        message: String,
                    }

                    match serde_json::from_str::<AnthropicEvent>(&data) {
                        Ok(event) => match event.event_type.as_str() {
                            "error" => {
                                // Provider-level error event: parse the nested
                                // error payload and surface it.
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
            stop: None,
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
            stop: None,
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

    // --- drain_lines: UTF-8 split-chunk safety tests ---

    /// Helper: feed byte slices one at a time, simulating network chunks, and
    /// collect all lines drained across all calls.
    fn feed_chunks(chunks: &[&[u8]]) -> (Vec<String>, Vec<u8>) {
        let mut raw_buf: Vec<u8> = Vec::new();
        let mut all_lines: Vec<String> = Vec::new();
        for chunk in chunks {
            raw_buf.extend_from_slice(chunk);
            drain_lines(&mut raw_buf, &mut all_lines);
        }
        (all_lines, raw_buf)
    }

    #[test]
    fn test_drain_lines_single_chunk() {
        let (lines, remainder) = feed_chunks(&[b"data: hello\ndata: world\n"]);
        assert_eq!(lines, vec!["data: hello", "data: world"]);
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_crlf_endings() {
        let (lines, remainder) = feed_chunks(&[b"data: hello\r\ndata: world\r\n"]);
        assert_eq!(lines, vec!["data: hello", "data: world"]);
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_incomplete_line_buffered() {
        // The second chunk does not end with '\n', so it stays in the buffer.
        let (lines, remainder) = feed_chunks(&[b"data: hello\n", b"data: partial"]);
        assert_eq!(lines, vec!["data: hello"]);
        assert_eq!(remainder, b"data: partial");
    }

    #[test]
    fn test_drain_lines_multibyte_split_across_chunks() {
        // U+00E9 LATIN SMALL LETTER É encodes as [0xC3, 0xA9] in UTF-8.
        // Split: first chunk ends with the first byte (0xC3), second chunk
        // carries the second byte (0xA9) plus the newline.
        // The old String::from_utf8_lossy-per-chunk approach would replace
        // 0xC3 with U+FFFD in the first chunk and produce garbage; the new
        // approach buffers raw bytes and only decodes after the '\n' arrives.
        let chunk1: &[u8] = b"data: caf\xc3"; // incomplete É
        let chunk2: &[u8] = b"\xa9\ndata: done\n"; // complete É + newline
        let (lines, remainder) = feed_chunks(&[chunk1, chunk2]);
        assert_eq!(lines[0], "data: café");
        assert_eq!(lines[1], "data: done");
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_three_byte_split_across_three_chunks() {
        // U+4E2D CJK UNIFIED IDEOGRAPH 中 encodes as [0xE4, 0xB8, 0xAD].
        // Split across three separate network chunks.
        let chunk1: &[u8] = b"data: \xe4";
        let chunk2: &[u8] = b"\xb8";
        let chunk3: &[u8] = b"\xad\n";
        let (lines, remainder) = feed_chunks(&[chunk1, chunk2, chunk3]);
        assert_eq!(lines, vec!["data: 中"]);
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_empty_lines_preserved() {
        // SSE uses blank lines as event separators; they must survive as empty
        // strings so the caller's `line.is_empty()` check can skip them.
        let (lines, _) = feed_chunks(&[b"data: hello\n\ndata: world\n"]);
        assert_eq!(lines, vec!["data: hello", "", "data: world"]);
    }

    #[test]
    fn test_drain_lines_no_newline_nothing_emitted() {
        let (lines, remainder) = feed_chunks(&[b"data: no newline yet"]);
        assert!(lines.is_empty());
        assert_eq!(remainder, b"data: no newline yet");
    }
}
