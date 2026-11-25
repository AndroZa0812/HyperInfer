//! OpenAI Responses API protocol translation.
//!
//! Translates between OpenAI's /v1/responses API format and HyperInfer's
//! internal ChatRequest/ChatResponse types.

use serde::{Deserialize, Serialize};

use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, MessageRole, ToolDefinition,
};

/// OpenAI Responses API request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesRequest {
    pub model: String,
    pub input: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
}

fn extract_text_from_input_item(item: &serde_json::Value) -> String {
    if let Some(s) = item.get("content").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(arr) = item.get("content").and_then(|v| v.as_array()) {
        let mut parts = Vec::new();
        for part in arr {
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                parts.push(text.to_string());
            }
        }
        return parts.join("");
    }
    item.to_string()
}

fn role_from_string(role: &str) -> MessageRole {
    match role {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "system" => MessageRole::System,
        "tool" => MessageRole::Tool,
        _ => MessageRole::User,
    }
}

fn extract_tools(tools: &[serde_json::Value]) -> Vec<ToolDefinition> {
    let mut result = Vec::new();
    for tool in tools {
        let target = tool.get("function").unwrap_or(tool);
        if let (Some(name), Some(params)) = (
            target.get("name").and_then(|v| v.as_str()),
            target.get("parameters"),
        ) {
            let desc = target
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            result.push(ToolDefinition {
                name: name.to_string(),
                description: desc,
                parameters: params.clone(),
            });
        }
    }
    result
}

/// Convert OpenAI Responses request to HyperInfer ChatRequest.
pub fn to_chat_request(req: ResponsesRequest) -> ChatRequest {
    let mut messages = Vec::new();

    if let Some(instructions) = &req.instructions {
        if !instructions.is_empty() {
            messages.push(ChatMessage::system(instructions.clone()));
        }
    }

    if let Some(s) = req.input.as_str() {
        messages.push(ChatMessage::user(s.to_string()));
    } else if let Some(arr) = req.input.as_array() {
        for item in arr {
            match item.get("type").and_then(|v| v.as_str()) {
                Some("function_call_output") => {
                    let call_id = item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let output = match item.get("output") {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                        None => String::new(),
                    };
                    messages.push(ChatMessage::tool_result(call_id, "", output));
                }
                Some("function_call") => {
                    let call_id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = match item.get("arguments") {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                        None => "{}".to_string(),
                    };
                    messages.push(ChatMessage::assistant_with_tools(
                        "",
                        vec![hyperinfer_core::ToolCall {
                            id: call_id,
                            name,
                            arguments,
                        }],
                    ));
                }
                _ => {
                    let role_str = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                    let role = role_from_string(role_str);
                    let text = extract_text_from_input_item(item);
                    match role {
                        MessageRole::User => messages.push(ChatMessage::user(text)),
                        MessageRole::Assistant => messages.push(ChatMessage::assistant(text)),
                        MessageRole::System => messages.push(ChatMessage::system(text)),
                        MessageRole::Tool => messages.push(ChatMessage::tool_result("", "", text)),
                    }
                }
            }
        }
    }

    let tools = req.tools.as_ref().map(|t| extract_tools(t));

    ChatRequest {
        model: req.model,
        messages,
        temperature: req.temperature,
        max_tokens: req.max_output_tokens,
        stream: req.stream,
        stop: None,
        tools,
        tool_choice: None,
        response_format: None,
        prompt_caching: None,
    }
}

/// Convert HyperInfer ChatResponse to OpenAI Responses API JSON.
pub fn from_chat_response(resp: &ChatResponse, request_model: &str) -> serde_json::Value {
    let mut output = Vec::new();
    let output_text = if resp.choices.is_empty() {
        String::new()
    } else {
        resp.choices[0].message.content.clone()
    };

    if !resp.choices.is_empty() {
        let choice = &resp.choices[0];
        if !choice.message.content.is_empty() {
            output.push(serde_json::json!({
                "type": "message",
                "role": "assistant",
                "status": "completed",
                "content": [{ "type": "output_text", "text": choice.message.content }]
            }));
        }
        if let Some(calls) = &choice.message.tool_calls {
            for call in calls {
                output.push(serde_json::json!({
                    "type": "function_call",
                    "call_id": call.id,
                    "name": call.name,
                    "arguments": call.arguments
                }));
            }
        }
    }

    let usage = serde_json::json!({
        "input_tokens": resp.usage.input_tokens,
        "output_tokens": resp.usage.output_tokens,
        "total_tokens": resp.usage.input_tokens + resp.usage.output_tokens
    });

    serde_json::json!({
        "id": resp.id,
        "object": "response",
        "status": "completed",
        "model": request_model,
        "output": output,
        "output_text": output_text,
        "usage": usage
    })
}

/// Convert a ChatChunk to OpenAI Responses SSE events.
pub fn chunk_to_sse_events(chunk: &ChatChunk, is_first: bool) -> Vec<(String, String)> {
    let mut events = Vec::new();

    if is_first {
        let created = serde_json::json!({
            "type": "response.created",
            "response": {
                "id": chunk.id,
                "object": "response",
                "status": "in_progress",
                "model": chunk.model,
                "output": []
            }
        });
        events.push(("response.created".to_string(), created.to_string()));
    }

    if !chunk.delta.is_empty() {
        let delta = serde_json::json!({
            "type": "response.output_text.delta",
            "delta": chunk.delta
        });
        events.push(("response.output_text.delta".to_string(), delta.to_string()));
    }

    if let Some(calls) = &chunk.tool_calls {
        for call in calls {
            let item = serde_json::json!({
                "type": "response.output_item.added",
                "item": {
                    "type": "function_call",
                    "call_id": call.id,
                    "name": call.name,
                    "arguments": call.arguments
                }
            });
            events.push(("response.output_item.added".to_string(), item.to_string()));
        }
    }

    if chunk.finish_reason.is_some() {
        let usage = chunk
            .usage
            .as_ref()
            .map(|u| {
                serde_json::json!({
                    "input_tokens": u.input_tokens,
                    "output_tokens": u.output_tokens,
                    "total_tokens": u.input_tokens + u.output_tokens
                })
            })
            .unwrap_or_else(|| {
                serde_json::json!({
                    "input_tokens": 0,
                    "output_tokens": 0,
                    "total_tokens": 0
                })
            });

        let completed = serde_json::json!({
            "type": "response.completed",
            "response": {
                "id": chunk.id,
                "object": "response",
                "status": "completed",
                "model": chunk.model,
                "output": [],
                "usage": usage
            }
        });
        events.push(("response.completed".to_string(), completed.to_string()));
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_chat_request_string_input() {
        let req = ResponsesRequest {
            model: "gpt-4o".to_string(),
            input: serde_json::json!("Hello"),
            instructions: Some("You are helpful".to_string()),
            max_output_tokens: Some(100),
            temperature: Some(0.7),
            stream: None,
            tools: None,
        };

        let chat_req = to_chat_request(req);
        assert_eq!(chat_req.model, "gpt-4o");
        assert_eq!(chat_req.messages.len(), 2);
        assert_eq!(chat_req.messages[0].role, MessageRole::System);
        assert_eq!(chat_req.messages[1].role, MessageRole::User);
        assert_eq!(chat_req.messages[1].content, "Hello");
        assert_eq!(chat_req.max_tokens, Some(100));
    }

    #[test]
    fn test_to_chat_request_array_input() {
        let req = ResponsesRequest {
            model: "gpt-4o".to_string(),
            input: serde_json::json!([
                { "role": "user", "content": "Hi" },
                { "role": "assistant", "content": "Hello!" }
            ]),
            instructions: None,
            max_output_tokens: None,
            temperature: None,
            stream: None,
            tools: None,
        };

        let chat_req = to_chat_request(req);
        assert_eq!(chat_req.messages.len(), 2);
        assert_eq!(chat_req.messages[0].role, MessageRole::User);
        assert_eq!(chat_req.messages[1].role, MessageRole::Assistant);
    }

    #[test]
    fn test_from_chat_response() {
        let resp = ChatResponse {
            id: "resp_123".to_string(),
            model: "gpt-4o".to_string(),
            choices: vec![hyperinfer_core::Choice {
                index: 0,
                message: ChatMessage::assistant("Hello back"),
                finish_reason: Some("stop".to_string()),
            }],
            usage: hyperinfer_core::Usage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            },
        };

        let responses_resp = from_chat_response(&resp, "gpt-4o");
        assert_eq!(responses_resp["id"], "resp_123");
        assert_eq!(responses_resp["object"], "response");
        assert_eq!(responses_resp["status"], "completed");
        assert_eq!(responses_resp["output"][0]["type"], "message");
        assert_eq!(
            responses_resp["output"][0]["content"][0]["type"],
            "output_text"
        );
        assert_eq!(
            responses_resp["output"][0]["content"][0]["text"],
            "Hello back"
        );
        assert_eq!(responses_resp["usage"]["input_tokens"], 10);
    }
}
