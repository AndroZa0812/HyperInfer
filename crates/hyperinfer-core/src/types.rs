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
    #[serde(skip_serializing)]
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
