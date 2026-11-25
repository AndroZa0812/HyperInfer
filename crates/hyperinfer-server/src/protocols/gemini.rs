//! Google Gemini API protocol translation.
//!
//! Translates between Gemini's :generateContent / :streamGenerateContent
//! format and HyperInfer's internal ChatRequest/ChatResponse types.

use serde::{Deserialize, Serialize};

use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, MessageRole, ToolCall, ToolDefinition,
};

/// Gemini content part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_call: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_response: Option<serde_json::Value>,
}

/// Gemini content message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub parts: Vec<GeminiPart>,
}

/// Gemini generation config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenerationConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
}

/// Gemini generateContent request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiGenerateRequest {
    pub contents: Vec<GeminiContent>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "systemInstruction",
        alias = "system_instruction"
    )]
    pub system_instruction: Option<GeminiContent>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "generationConfig",
        alias = "generation_config"
    )]
    pub generation_config: Option<GeminiGenerationConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
}

fn gemini_role_to_message_role(role: &str) -> MessageRole {
    match role {
        "user" => MessageRole::User,
        "model" => MessageRole::Assistant,
        "system" => MessageRole::System,
        "tool" => MessageRole::Tool,
        _ => MessageRole::User,
    }
}

fn extract_text_from_parts(parts: &[GeminiPart]) -> String {
    let mut result = String::new();
    for part in parts {
        if let Some(text) = &part.text {
            result.push_str(text);
        }
    }
    result
}

fn extract_tool_calls_from_parts(parts: &[GeminiPart]) -> Option<Vec<ToolCall>> {
    let mut calls = Vec::new();
    for part in parts {
        if let Some(fc) = &part.function_call {
            if let (Some(name), Some(args)) =
                (fc.get("name").and_then(|v| v.as_str()), fc.get("args"))
            {
                calls.push(ToolCall {
                    id: format!("call_{}", calls.len()),
                    name: name.to_string(),
                    arguments: args.to_string(),
                });
            }
        }
    }
    if calls.is_empty() {
        None
    } else {
        Some(calls)
    }
}

fn extract_tools(tools: &[serde_json::Value]) -> Vec<ToolDefinition> {
    let mut result = Vec::new();
    for tool in tools {
        let decls = tool
            .get("functionDeclarations")
            .or_else(|| tool.get("function_declarations"));
        if let Some(decls) = decls.and_then(|v| v.as_array()) {
            for decl in decls {
                if let (Some(name), Some(params)) = (
                    decl.get("name").and_then(|v| v.as_str()),
                    decl.get("parameters"),
                ) {
                    let desc = decl
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
        }
    }
    result
}

/// Convert Gemini generateContent request to HyperInfer ChatRequest.
pub fn to_chat_request(model: &str, req: GeminiGenerateRequest) -> ChatRequest {
    let mut messages = Vec::new();

    // Handle system instruction
    if let Some(sys) = &req.system_instruction {
        let text = extract_text_from_parts(&sys.parts);
        if !text.is_empty() {
            messages.push(ChatMessage::system(text));
        }
    }

    // Convert contents
    for content in req.contents {
        let role_str = content.role.as_deref().unwrap_or("user");
        let role = gemini_role_to_message_role(role_str);
        let text = extract_text_from_parts(&content.parts);
        let tool_calls = extract_tool_calls_from_parts(&content.parts);

        if let Some(calls) = tool_calls {
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

    let tools = req.tools.as_ref().map(|t| extract_tools(t));
    let config = req.generation_config;

    ChatRequest {
        model: model.to_string(),
        messages,
        temperature: config.as_ref().and_then(|c| c.temperature),
        max_tokens: config.as_ref().and_then(|c| c.max_output_tokens),
        stream: None,
        stop: config.and_then(|c| c.stop_sequences),
        tools,
        tool_choice: None,
        response_format: None,
    }
}

/// Convert HyperInfer ChatResponse to Gemini GenerateContentResponse JSON.
pub fn from_chat_response(resp: &ChatResponse) -> serde_json::Value {
    let mut parts = Vec::new();

    if !resp.choices.is_empty() {
        let choice = &resp.choices[0];
        if !choice.message.content.is_empty() {
            parts.push(serde_json::json!({ "text": choice.message.content }));
        }
        if let Some(calls) = &choice.message.tool_calls {
            for call in calls {
                let args: serde_json::Value =
                    serde_json::from_str(&call.arguments).unwrap_or_else(|_| serde_json::json!({}));
                parts.push(serde_json::json!({
                    "functionCall": {
                        "name": call.name,
                        "args": args
                    }
                }));
            }
        }
    }

    let finish_reason = if resp.choices.is_empty() {
        "STOP"
    } else {
        match resp.choices[0].finish_reason.as_deref() {
            Some("stop") => "STOP",
            Some("tool_calls") => "TOOL_CALL",
            Some("length") => "MAX_TOKENS",
            _ => "STOP",
        }
    };

    let usage = serde_json::json!({
        "promptTokenCount": resp.usage.input_tokens,
        "candidatesTokenCount": resp.usage.output_tokens,
        "totalTokenCount": resp.usage.input_tokens + resp.usage.output_tokens
    });

    serde_json::json!({
        "candidates": [{
            "content": {
                "role": "model",
                "parts": parts
            },
            "finishReason": finish_reason,
            "index": 0
        }],
        "usageMetadata": usage
    })
}

/// Convert a ChatChunk to a Gemini streaming JSON chunk.
pub fn chunk_to_sse_json(chunk: &ChatChunk) -> String {
    let mut parts = Vec::new();
    if !chunk.delta.is_empty() {
        parts.push(serde_json::json!({ "text": chunk.delta }));
    }

    let finish_reason = chunk.finish_reason.as_deref().map(|r| match r {
        "stop" => "STOP",
        "tool_calls" => "TOOL_CALL",
        "length" => "MAX_TOKENS",
        _ => "STOP",
    });

    let mut candidate = serde_json::json!({
        "content": {
            "role": "model",
            "parts": parts
        },
        "index": 0
    });

    if let Some(fr) = finish_reason {
        candidate["finishReason"] = serde_json::json!(fr);
    }

    let mut result = serde_json::json!({
        "candidates": [candidate]
    });

    if let Some(usage) = &chunk.usage {
        result["usageMetadata"] = serde_json::json!({
            "promptTokenCount": usage.input_tokens,
            "candidatesTokenCount": usage.output_tokens,
            "totalTokenCount": usage.input_tokens + usage.output_tokens
        });
    }

    result.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_chat_request_with_system_instruction() {
        let req = GeminiGenerateRequest {
            contents: vec![GeminiContent {
                role: Some("user".to_string()),
                parts: vec![GeminiPart {
                    text: Some("Hello".to_string()),
                    function_call: None,
                    function_response: None,
                }],
            }],
            system_instruction: Some(GeminiContent {
                role: None,
                parts: vec![GeminiPart {
                    text: Some("You are helpful".to_string()),
                    function_call: None,
                    function_response: None,
                }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                temperature: Some(0.7),
                max_output_tokens: Some(100),
                stop_sequences: None,
            }),
            tools: None,
        };

        let chat_req = to_chat_request("gemini-1.5-pro", req);
        assert_eq!(chat_req.model, "gemini-1.5-pro");
        assert_eq!(chat_req.messages.len(), 2);
        assert_eq!(chat_req.messages[0].role, MessageRole::System);
        assert_eq!(chat_req.messages[1].role, MessageRole::User);
        assert_eq!(chat_req.messages[1].content, "Hello");
        assert_eq!(chat_req.temperature, Some(0.7));
        assert_eq!(chat_req.max_tokens, Some(100));
    }

    #[test]
    fn test_gemini_model_role_maps_to_assistant() {
        let req = GeminiGenerateRequest {
            contents: vec![
                GeminiContent {
                    role: Some("user".to_string()),
                    parts: vec![GeminiPart {
                        text: Some("Hi".to_string()),
                        function_call: None,
                        function_response: None,
                    }],
                },
                GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart {
                        text: Some("Hello!".to_string()),
                        function_call: None,
                        function_response: None,
                    }],
                },
            ],
            system_instruction: None,
            generation_config: None,
            tools: None,
        };

        let chat_req = to_chat_request("gemini-1.5-pro", req);
        assert_eq!(chat_req.messages.len(), 2);
        assert_eq!(chat_req.messages[0].role, MessageRole::User);
        assert_eq!(chat_req.messages[1].role, MessageRole::Assistant);
    }

    #[test]
    fn test_from_chat_response() {
        let resp = ChatResponse {
            id: "resp_123".to_string(),
            model: "gemini-1.5-pro".to_string(),
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

        let gemini_resp = from_chat_response(&resp);
        assert_eq!(gemini_resp["candidates"][0]["content"]["role"], "model");
        assert_eq!(
            gemini_resp["candidates"][0]["content"]["parts"][0]["text"],
            "Hello back"
        );
        assert_eq!(gemini_resp["candidates"][0]["finishReason"], "STOP");
        assert_eq!(gemini_resp["usageMetadata"]["promptTokenCount"], 10);
        assert_eq!(gemini_resp["usageMetadata"]["candidatesTokenCount"], 5);
    }
}
