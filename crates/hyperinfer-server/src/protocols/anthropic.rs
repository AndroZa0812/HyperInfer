//! Anthropic Messages API protocol translation.
//!
//! Translates between Anthropic's Messages API format and HyperInfer's
//! internal ChatRequest/ChatResponse types.

use serde::{Deserialize, Serialize};

use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, MessageRole, ToolCall, ToolDefinition,
};

/// Anthropic Messages API request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicTool>>,
}

/// Anthropic message with role and content (string or content blocks).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: serde_json::Value,
}

/// Anthropic tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

fn anthropic_role_to_message_role(role: &str) -> MessageRole {
    match role {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "system" => MessageRole::System,
        "tool" => MessageRole::Tool,
        _ => MessageRole::User,
    }
}

fn extract_text_from_content(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let mut parts = Vec::new();
        for block in arr {
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                parts.push(text.to_string());
            }
        }
        return parts.join("");
    }
    content.to_string()
}

fn extract_tool_calls_from_content(content: &serde_json::Value) -> Option<Vec<ToolCall>> {
    if let Some(arr) = content.as_array() {
        let mut calls = Vec::new();
        for block in arr {
            if let Some("tool_use") = block.get("type").and_then(|v| v.as_str()) {
                if let (Some(id), Some(name), Some(input)) = (
                    block.get("id").and_then(|v| v.as_str()),
                    block.get("name").and_then(|v| v.as_str()),
                    block.get("input"),
                ) {
                    calls.push(ToolCall {
                        id: id.to_string(),
                        name: name.to_string(),
                        arguments: input.to_string(),
                    });
                }
            }
        }
        if !calls.is_empty() {
            return Some(calls);
        }
    }
    None
}

fn extract_tool_results_from_content(content: &serde_json::Value) -> Vec<(String, String)> {
    let mut results = Vec::new();
    if let Some(arr) = content.as_array() {
        for block in arr {
            if let Some("tool_result") = block.get("type").and_then(|v| v.as_str()) {
                if let Some(tool_use_id) = block.get("tool_use_id").and_then(|v| v.as_str()) {
                    let result_text = extract_text_from_content(
                        block.get("content").unwrap_or(&serde_json::Value::Null),
                    );
                    results.push((tool_use_id.to_string(), result_text));
                }
            }
        }
    }
    results
}

/// Convert Anthropic Messages request to HyperInfer ChatRequest.
pub fn to_chat_request(req: AnthropicMessagesRequest) -> ChatRequest {
    let mut messages = Vec::new();

    // Handle system message
    if let Some(system) = &req.system {
        let system_text = extract_text_from_content(system);
        if !system_text.is_empty() {
            messages.push(ChatMessage::system(system_text));
        }
    }

    // Convert messages
    for msg in req.messages {
        let role = anthropic_role_to_message_role(&msg.role);
        let text = extract_text_from_content(&msg.content);
        let tool_calls = extract_tool_calls_from_content(&msg.content);
        let tool_results = extract_tool_results_from_content(&msg.content);

        if !tool_results.is_empty() {
            for (tool_use_id, result) in tool_results {
                messages.push(ChatMessage::tool_result(tool_use_id, "", result));
            }
        } else if let Some(calls) = tool_calls {
            messages.push(ChatMessage::assistant_with_tools(text, calls));
        } else {
            match role {
                MessageRole::User => messages.push(ChatMessage::user(text)),
                MessageRole::Assistant => messages.push(ChatMessage::assistant(text)),
                MessageRole::System => messages.push(ChatMessage::system(text)),
                MessageRole::Tool => messages.push(ChatMessage::tool_result("", "", text)),
            }
        }
    }

    // Convert tools
    let tools = req.tools.map(|t| {
        t.into_iter()
            .map(|t| ToolDefinition {
                name: t.name,
                description: t.description.unwrap_or_default(),
                parameters: t.input_schema,
            })
            .collect()
    });

    ChatRequest {
        model: req.model,
        messages,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        stream: req.stream,
        stop: req.stop_sequences,
        tools,
        tool_choice: None,
        response_format: None,
    }
}

/// Convert HyperInfer ChatResponse to Anthropic Messages response JSON.
pub fn from_chat_response(resp: &ChatResponse, request_model: &str) -> serde_json::Value {
    let mut content_blocks = Vec::new();

    // Add text content
    if !resp.choices.is_empty() {
        let choice = &resp.choices[0];
        if !choice.message.content.is_empty() {
            content_blocks.push(serde_json::json!({
                "type": "text",
                "text": choice.message.content
            }));
        }

        // Add tool use blocks
        if let Some(calls) = &choice.message.tool_calls {
            for call in calls {
                let input: serde_json::Value =
                    serde_json::from_str(&call.arguments).unwrap_or_else(|_| serde_json::json!({}));
                content_blocks.push(serde_json::json!({
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.name,
                    "input": input
                }));
            }
        }
    }

    let stop_reason = if resp.choices.is_empty() {
        "stop"
    } else {
        match resp.choices[0].finish_reason.as_deref() {
            Some("stop") => "end_turn",
            Some("tool_calls") => "tool_use",
            Some("length") => "max_tokens",
            _ => "stop",
        }
    };

    let usage = serde_json::json!({
        "input_tokens": resp.usage.input_tokens,
        "output_tokens": resp.usage.output_tokens,
        "total_tokens": resp.usage.input_tokens + resp.usage.output_tokens
    });

    serde_json::json!({
        "id": resp.id,
        "type": "message",
        "role": "assistant",
        "model": request_model,
        "content": content_blocks,
        "stop_reason": stop_reason,
        "usage": usage
    })
}

/// Convert a ChatChunk to Anthropic SSE events.
pub fn chunk_to_sse_events(chunk: &ChatChunk, is_first: bool) -> Vec<(String, String)> {
    let mut events = Vec::new();

    if is_first {
        // message_start
        let start = serde_json::json!({
            "type": "message_start",
            "message": {
                "id": chunk.id,
                "type": "message",
                "role": "assistant",
                "model": chunk.model,
                "content": []
            }
        });
        events.push(("message_start".to_string(), start.to_string()));

        // content_block_start
        let block_start = serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {
                "type": "text",
                "text": ""
            }
        });
        events.push(("content_block_start".to_string(), block_start.to_string()));
    }

    // content_block_delta
    if !chunk.delta.is_empty() {
        let delta = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": chunk.delta
            }
        });
        events.push(("content_block_delta".to_string(), delta.to_string()));
    }

    // Final events
    if chunk.finish_reason.is_some() {
        // message_delta with usage
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

        let stop_reason = match chunk.finish_reason.as_deref() {
            Some("stop") => "end_turn",
            Some("tool_calls") => "tool_use",
            Some("length") => "max_tokens",
            _ => "stop",
        };

        let msg_delta = serde_json::json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": stop_reason
            },
            "usage": usage
        });
        events.push(("message_delta".to_string(), msg_delta.to_string()));

        // message_stop
        let msg_stop = serde_json::json!({
            "type": "message_stop"
        });
        events.push(("message_stop".to_string(), msg_stop.to_string()));
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_chat_request_basic() {
        let req = AnthropicMessagesRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            system: Some(serde_json::json!("You are helpful")),
            max_tokens: Some(100),
            temperature: Some(0.7),
            stream: None,
            stop_sequences: None,
            tools: None,
        };

        let chat_req = to_chat_request(req);
        assert_eq!(chat_req.model, "claude-sonnet-4-20250514");
        assert_eq!(chat_req.messages.len(), 2); // system + user
        assert_eq!(chat_req.messages[0].role, MessageRole::System);
        assert_eq!(chat_req.messages[1].role, MessageRole::User);
        assert_eq!(chat_req.messages[1].content, "Hello");
        assert_eq!(chat_req.max_tokens, Some(100));
        assert_eq!(chat_req.temperature, Some(0.7));
    }

    #[test]
    fn test_from_chat_response() {
        let resp = ChatResponse {
            id: "msg_123".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            choices: vec![hyperinfer_core::Choice {
                index: 0,
                message: ChatMessage::assistant("Hello back"),
                finish_reason: Some("stop".to_string()),
            }],
            usage: hyperinfer_core::Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
        };

        let anthropic_resp = from_chat_response(&resp, "claude-sonnet-4-20250514");
        assert_eq!(anthropic_resp["id"], "msg_123");
        assert_eq!(anthropic_resp["type"], "message");
        assert_eq!(anthropic_resp["role"], "assistant");
        assert_eq!(anthropic_resp["model"], "claude-sonnet-4-20250514");
        assert_eq!(anthropic_resp["stop_reason"], "end_turn");
        assert_eq!(anthropic_resp["usage"]["input_tokens"], 10);
        assert_eq!(anthropic_resp["usage"]["output_tokens"], 5);
        assert_eq!(anthropic_resp["content"][0]["type"], "text");
        assert_eq!(anthropic_resp["content"][0]["text"], "Hello back");
    }

    #[test]
    fn test_chunk_to_sse_events() {
        let chunk = ChatChunk {
            id: "msg_123".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            delta: "Hello".to_string(),
            finish_reason: None,
            usage: None,
            tool_calls: None,
        };

        let events = chunk_to_sse_events(&chunk, true);
        assert_eq!(events.len(), 3); // message_start, content_block_start, content_block_delta
        assert_eq!(events[0].0, "message_start");
        assert_eq!(events[1].0, "content_block_start");
        assert_eq!(events[2].0, "content_block_delta");

        // Final chunk
        let final_chunk = ChatChunk {
            id: "msg_123".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            delta: "".to_string(),
            finish_reason: Some("stop".to_string()),
            usage: Some(hyperinfer_core::Usage {
                input_tokens: 10,
                output_tokens: 5,
            }),
            tool_calls: None,
        };

        let final_events = chunk_to_sse_events(&final_chunk, false);
        assert_eq!(final_events.len(), 2); // message_delta, message_stop
        assert_eq!(final_events[0].0, "message_delta");
        assert_eq!(final_events[1].0, "message_stop");
    }
}
