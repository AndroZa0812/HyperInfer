//! Core types, traits, and error handling for HyperInfer
//!
//! This crate contains shared data structures, traits, and error definitions
//! used across the entire HyperInfer monorepo.

#![allow(clippy::double_must_use)]

pub mod error;
pub mod rate_limiting;
pub mod redis;
pub mod telemetry_consumer;
pub mod traits;
pub mod types;

pub use error::{ConfigError, DbError, HyperInferError};
pub use rate_limiting::{RateLimiter, USAGE_REQUESTS_KEY_PREFIX, USAGE_TOKENS_KEY_PREFIX};
pub use redis::PolicyUpdate;
pub use telemetry_consumer::TelemetryConsumer;
pub use traits::{
    ApiKey, ConfigStore, Database, ModelAlias, NewUsageLog, Quota, Team, UsageLog, User,
};
pub use types::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Choice, Config, CreateDeploymentRequest,
    Deployment, MessageRole, Provider, RoutingConfig, RoutingRule, ToolCall, ToolDefinition,
    UpdateRoutingConfigRequest, Usage, UsageRecord,
};

/// SHA-256 hash of a key, returned as a hex string.
pub fn hash_key(key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Truncated hash suffix of a key for safe logging (last 8 hex chars).
pub fn key_id(key: &str) -> String {
    let hash = hash_key(key);
    if hash.len() >= 8 {
        format!("...{}", &hash[hash.len() - 8..])
    } else {
        hash
    }
}
