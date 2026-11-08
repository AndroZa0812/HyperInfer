//! Shared data types for HyperInfer
//!
//! Defines common structures used across the system.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// A tool (function) definition exposed to an LLM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the tool's input parameters.
    pub parameters: serde_json::Value,
}

/// A tool (function) invocation requested by an LLM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// JSON-serialized arguments object (e.g. `{"path": "Cargo.toml"}`).
    pub arguments: String,
}

/// A chat request to an LLM provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatRequest {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    /// When `true` the provider is asked to stream token chunks via SSE.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Stop sequences: generation halts when any of these strings is produced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Tools available for the model to call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

/// A single streamed token delta from a provider SSE event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatChunk {
    /// Unique ID for this streaming response (matches across chunks).
    #[serde(default)]
    pub id: String,
    /// The model that produced this chunk.
    #[serde(default)]
    pub model: String,
    /// Incremental content delta for this chunk (may be empty on final chunk).
    #[serde(default)]
    pub delta: String,
    /// Set to `"stop"` (or similar) on the last chunk, `None` otherwise.
    pub finish_reason: Option<String>,
    /// Token usage — only populated on the final chunk (OpenAI `usage` field
    /// with `stream_options: {include_usage: true}`, or Anthropic `message_delta`).
    pub usage: Option<Usage>,
    /// Tool calls emitted during streaming (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatRequest {
    pub fn validate(&self) -> Result<(), crate::HyperInferError> {
        if self.model.is_empty() {
            return Err(crate::HyperInferError::Config(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "model cannot be empty",
            )));
        }
        if self.messages.is_empty() {
            return Err(crate::HyperInferError::Config(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "messages cannot be empty",
            )));
        }
        Ok(())
    }
}

/// A single message in a chat conversation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    #[serde(default)]
    pub content: String,
    /// Tool calls requested by an Assistant message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// ID of the `ToolCall` that a `MessageRole::Tool` message is responding to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional tool name for `MessageRole::Tool` messages (used by Gemini `functionResponse` and OpenAI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
            name: None,
        }
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
        }
    }
}

/// The role of a message in a chat
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A token bucket for rate limiting
#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub capacity: u64,
    pub tokens: u64,
    pub refill_rate: u64, // tokens per second
    pub last_refill: Instant,
}

/// Configuration structure for the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip_serializing, default)]
    pub api_keys: HashMap<String, String>,
    pub routing_rules: Vec<RoutingRule>,
    pub quotas: HashMap<String, Quota>,
    pub model_aliases: HashMap<String, String>,
    #[serde(default)]
    pub default_provider: Option<Provider>,
}

/// A routing rule for LLM providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub name: String,
    pub priority: u32,
    pub fallback_models: Vec<String>,
}

/// Quota configuration for a resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quota {
    pub max_requests_per_minute: Option<u64>,
    pub max_tokens_per_minute: Option<u64>,
    pub budget_cents: Option<u64>, // monthly budget in cents (USD)
}

/// Provider enumeration for LLM services
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    OpenAI,
    Anthropic,
    Gemini,
    Antigravity,
    #[serde(other)]
    Other,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::OpenAI => write!(f, "openai"),
            Provider::Anthropic => write!(f, "anthropic"),
            Provider::Gemini => write!(f, "gemini"),
            Provider::Antigravity => write!(f, "antigravity"),
            Provider::Other => write!(f, "other"),
        }
    }
}

/// Token usage reported on the final SSE chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
}

/// A usage record for telemetry (stored in Redis Stream and PostgreSQL)
///
/// All timestamps are in milliseconds since Unix epoch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub key: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub response_time_ms: u64,
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
}

/// A choice in a chat response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

/// A chat response from an LLM provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Usage,
}

/// An API deployment for a model (managed via POST /v1/deployments)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing)]
    pub api_key_ref: String,
    pub base_url: String,
    pub is_active: bool,
    pub weight: u32,
    pub priority: u32,
    pub max_tpm: Option<u32>,
    pub max_rpm: Option<u32>,
    pub cost_per_1k_input_tokens: Option<f64>,
    pub cost_per_1k_output_tokens: Option<f64>,
    #[serde(skip_serializing)]
    pub metadata: serde_json::Value,
    pub sort_order: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Input payload for creating a deployment
#[derive(Debug, Clone, Deserialize)]
pub struct CreateDeploymentRequest {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub api_key_ref: Option<String>,
    pub base_url: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default)]
    pub priority: i32,
    pub max_tpm: Option<i32>,
    pub max_rpm: Option<i32>,
    pub cost_per_1k_input_tokens: Option<f64>,
    pub cost_per_1k_output_tokens: Option<f64>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub sort_order: Option<i32>,
}

impl CreateDeploymentRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.weight < 0 {
            return Err("weight must be non-negative".to_string());
        }
        if self.priority < 0 {
            return Err("priority must be non-negative".to_string());
        }
        if let Some(sort_order) = self.sort_order {
            if sort_order < 0 {
                return Err("sort_order must be non-negative".to_string());
            }
        }
        Ok(())
    }
}

/// Routing configuration (singleton row in routing_config table)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub strategy: String,
    pub strategy_params: serde_json::Value,
    pub fallback_config: serde_json::Value,
    pub routing_groups: serde_json::Value,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Input payload for updating routing config
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRoutingConfigRequest {
    pub strategy: Option<String>,
    pub strategy_params: Option<serde_json::Value>,
    pub fallback_config: Option<serde_json::Value>,
    pub routing_groups: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}
fn default_weight() -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_validate_empty_model() {
        let request = ChatRequest {
            model: "".to_string(),
            messages: vec![ChatMessage::user("test")],
            temperature: None,
            max_tokens: None,
            stream: None,
            stop: None,
            tools: None,
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_chat_request_validate_empty_messages() {
        let request = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            stream: None,
            stop: None,
            tools: None,
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_chat_request_validate_success() {
        let request = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage::user("Hello")],
            temperature: Some(0.7),
            max_tokens: Some(100),
            stream: None,
            stop: None,
            tools: None,
        };

        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(Provider::OpenAI.to_string(), "openai");
        assert_eq!(Provider::Anthropic.to_string(), "anthropic");
        assert_eq!(Provider::Gemini.to_string(), "gemini");
        assert_eq!(Provider::Antigravity.to_string(), "antigravity");
        assert_eq!(Provider::Other.to_string(), "other");
    }

    #[test]
    fn test_message_role_serialization() {
        let role = MessageRole::User;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"user\"");

        let role = MessageRole::Assistant;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"assistant\"");

        let role = MessageRole::System;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"system\"");

        let role = MessageRole::Tool;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"tool\"");
    }

    #[test]
    fn test_chat_message_serialization() {
        let message = ChatMessage::user("Hello");

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(message, deserialized);
    }

    #[test]
    fn test_tool_call_and_tool_result_serialization() {
        let tc = ToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: r#"{"path":"Cargo.toml"}"#.to_string(),
        };
        let assistant_msg = ChatMessage::assistant_with_tools("", vec![tc.clone()]);
        let json = serde_json::to_string(&assistant_msg).unwrap();
        let roundtrip: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.tool_calls, Some(vec![tc]));

        let tool_msg = ChatMessage::tool_result("call_1", "read_file", "[package]\nname = \"x\"");
        assert_eq!(tool_msg.role, MessageRole::Tool);
        assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(tool_msg.name.as_deref(), Some("read_file"));
    }

    #[test]
    fn test_chat_request_default() {
        let request = ChatRequest::default();
        assert_eq!(request.model, "");
        assert_eq!(request.messages.len(), 0);
        assert_eq!(request.temperature, None);
        assert_eq!(request.max_tokens, None);
    }

    #[test]
    fn test_usage_default() {
        let usage = Usage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    #[test]
    fn test_chat_response_default() {
        let response = ChatResponse::default();
        assert_eq!(response.id, "");
        assert_eq!(response.model, "");
        assert_eq!(response.choices.len(), 0);
        assert_eq!(response.usage.input_tokens, 0);
    }

    #[test]
    fn test_provider_deserialization() {
        let json = "\"openai\"";
        let provider: Provider = serde_json::from_str(json).unwrap();
        assert_eq!(provider, Provider::OpenAI);

        let json = "\"anthropic\"";
        let provider: Provider = serde_json::from_str(json).unwrap();
        assert_eq!(provider, Provider::Anthropic);

        let json = "\"gemini\"";
        let provider: Provider = serde_json::from_str(json).unwrap();
        assert_eq!(provider, Provider::Gemini);

        let json = "\"antigravity\"";
        let provider: Provider = serde_json::from_str(json).unwrap();
        assert_eq!(provider, Provider::Antigravity);

        let json = "\"unknown\"";
        let provider: Provider = serde_json::from_str(json).unwrap();
        assert_eq!(provider, Provider::Other);
    }

    #[test]
    fn test_config_serialization() {
        let mut config = Config {
            api_keys: HashMap::new(),
            routing_rules: vec![],
            quotas: HashMap::new(),
            model_aliases: HashMap::new(),
            default_provider: Some(Provider::OpenAI),
        };

        config
            .api_keys
            .insert("openai".to_string(), "sk-test".to_string());

        let json = serde_json::to_string(&config).unwrap();
        // api_keys should be skipped during serialization
        assert!(!json.contains("api_keys"));
        assert!(json.contains("routing_rules"));
    }

    #[test]
    fn test_quota_with_all_fields() {
        let quota = Quota {
            max_requests_per_minute: Some(100),
            max_tokens_per_minute: Some(10000),
            budget_cents: Some(5000),
        };

        assert_eq!(quota.max_requests_per_minute, Some(100));
        assert_eq!(quota.max_tokens_per_minute, Some(10000));
        assert_eq!(quota.budget_cents, Some(5000));
    }

    #[test]
    fn test_choice_structure() {
        let choice = Choice {
            index: 0,
            message: ChatMessage::assistant("Response"),
            finish_reason: Some("stop".to_string()),
        };

        assert_eq!(choice.index, 0);
        assert_eq!(choice.message.role, MessageRole::Assistant);
        assert_eq!(choice.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_usage_record_creation() {
        let record = UsageRecord {
            key: "test-key".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            response_time_ms: 250,
            timestamp: 1700000000000,
            msg_id: None,
        };

        assert_eq!(record.key, "test-key");
        assert_eq!(record.model, "gpt-4");
        assert_eq!(record.input_tokens, 100);
        assert_eq!(record.output_tokens, 50);
        assert_eq!(record.response_time_ms, 250);
        assert_eq!(record.timestamp, 1700000000000);
    }

    #[test]
    fn test_usage_record_serialization() {
        let record = UsageRecord {
            key: "test-key".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            response_time_ms: 250,
            timestamp: 1700000000000,
            msg_id: None,
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: UsageRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_usage_record_zero_values() {
        let record = UsageRecord {
            key: "test-key".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 0,
            output_tokens: 0,
            response_time_ms: 0,
            timestamp: 0,
            msg_id: None,
        };

        assert_eq!(record.input_tokens, 0);
        assert_eq!(record.output_tokens, 0);
        assert_eq!(record.response_time_ms, 0);
        assert_eq!(record.timestamp, 0);
    }

    #[test]
    fn test_usage_record_max_values() {
        let record = UsageRecord {
            key: "test-key".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: u32::MAX,
            output_tokens: u32::MAX,
            response_time_ms: u64::MAX,
            timestamp: u64::MAX,
            msg_id: None,
        };

        assert_eq!(record.input_tokens, u32::MAX);
        assert_eq!(record.output_tokens, u32::MAX);
        assert_eq!(record.response_time_ms, u64::MAX);
        assert_eq!(record.timestamp, u64::MAX);
    }

    #[test]
    fn test_usage_record_empty_strings() {
        let record = UsageRecord {
            key: "".to_string(),
            model: "".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            response_time_ms: 250,
            timestamp: 1700000000000,
            msg_id: None,
        };

        assert_eq!(record.key, "");
        assert_eq!(record.model, "");
    }

    #[test]
    fn test_usage_record_special_characters() {
        let record = UsageRecord {
            key: "test-key-!@#$%".to_string(),
            model: "gpt-4-turbo-preview".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            response_time_ms: 250,
            timestamp: 1700000000000,
            msg_id: None,
        };

        assert_eq!(record.key, "test-key-!@#$%");
        assert_eq!(record.model, "gpt-4-turbo-preview");
    }

    #[test]
    fn test_usage_record_unicode() {
        let record = UsageRecord {
            key: "test-key-🔑".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            response_time_ms: 250,
            timestamp: 1700000000000,
            msg_id: None,
        };

        assert_eq!(record.key, "test-key-🔑");

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: UsageRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_usage_record_long_strings() {
        let long_key = "a".repeat(10000);
        let long_model = "b".repeat(10000);
        let record = UsageRecord {
            key: long_key.clone(),
            model: long_model.clone(),
            input_tokens: 100,
            output_tokens: 50,
            response_time_ms: 250,
            timestamp: 1700000000000,
            msg_id: None,
        };

        assert_eq!(record.key.len(), 10000);
        assert_eq!(record.model.len(), 10000);
    }

    #[test]
    fn test_usage_record_clone() {
        let record = UsageRecord {
            key: "test-key".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            response_time_ms: 250,
            timestamp: 1700000000000,
            msg_id: None,
        };

        let cloned = record.clone();
        assert_eq!(record, cloned);
    }

    #[test]
    fn test_usage_record_debug() {
        let record = UsageRecord {
            key: "test-key".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            response_time_ms: 250,
            timestamp: 1700000000000,
            msg_id: None,
        };

        let debug_str = format!("{:?}", record);
        assert!(debug_str.contains("test-key"));
        assert!(debug_str.contains("gpt-4"));
        assert!(debug_str.contains("100"));
        assert!(debug_str.contains("50"));
    }
}
