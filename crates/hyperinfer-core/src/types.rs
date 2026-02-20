//! Shared data types for HyperInfer
//!
//! Defines common structures used across the system.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A chat request to an LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

/// A single message in a chat conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

/// The role of a message in a chat
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// Configuration structure for the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_keys: HashMap<String, String>,
    pub routing_rules: Vec<RoutingRule>,
    pub quotas: HashMap<String, Quota>,
    pub model_aliases: HashMap<String, String>,
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
    pub budget: Option<f64>, // monthly budget in USD
}
