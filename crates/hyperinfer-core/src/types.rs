//! Shared data types for HyperInfer
//!
//! Defines common structures used across the system.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// A chat request to an LLM provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatRequest {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
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
    pub content: String,
}

/// The role of a message in a chat
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
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
    #[serde(other)]
    Other,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::OpenAI => write!(f, "openai"),
            Provider::Anthropic => write!(f, "anthropic"),
            Provider::Other => write!(f, "other"),
        }
    }
}

/// Usage statistics for a request
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_validate_empty_model() {
        let request = ChatRequest {
            model: "".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "test".to_string(),
            }],
            temperature: None,
            max_tokens: None,
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
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_chat_request_validate_success() {
        let request = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Hello".to_string(),
            }],
            temperature: Some(0.7),
            max_tokens: Some(100),
        };

        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(Provider::OpenAI.to_string(), "openai");
        assert_eq!(Provider::Anthropic.to_string(), "anthropic");
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
    }

    #[test]
    fn test_chat_message_serialization() {
        let message = ChatMessage {
            role: MessageRole::User,
            content: "Hello".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(message, deserialized);
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
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: "Response".to_string(),
            },
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
        };

        let debug_str = format!("{:?}", record);
        assert!(debug_str.contains("test-key"));
        assert!(debug_str.contains("gpt-4"));
        assert!(debug_str.contains("100"));
        assert!(debug_str.contains("50"));
    }
}
