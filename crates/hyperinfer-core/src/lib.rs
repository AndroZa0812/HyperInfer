//! Core types, traits, and error handling for HyperInfer
//!
//! This crate contains shared data structures, traits, and error definitions
//! used across the entire HyperInfer monorepo.

#![allow(clippy::double_must_use)]

pub mod catalog;
pub mod cost;
pub mod error;
pub mod plugin;
pub mod rate_limiting;
pub mod redaction;
pub mod redis;
pub mod telemetry_consumer;
pub mod traits;
pub mod types;

pub use catalog::{
    default_cache_path, DiscoveredModel, ModelCatalog, ModelPricing, ModelSpec,
    DEFAULT_CACHE_TTL_SECS, DEFAULT_CATALOG_URL,
};
pub use cost::{CostCalculator, TokenCostWeights, WeightedUsage};
pub use error::{ConfigError, DbError, HyperInferError};
pub use plugin::{GatewayPlugin, PluginAction, PluginContext, PluginPipeline};
pub use rate_limiting::{RateLimiter, USAGE_REQUESTS_KEY_PREFIX, USAGE_TOKENS_KEY_PREFIX};
pub use redaction::{RedactionConfig, RedactionPlugin};
pub use redis::{MemoryEventAction, MemoryUpdateEvent, PolicyUpdate, MEMORY_EVENTS_CHANNEL};
pub use telemetry_consumer::TelemetryConsumer;
pub use traits::{
    ApiKey, ConfigStore, CreateMemoryRequest, Database, ListMemoriesFilter, ListUsageLogsFilter,
    Memory, MemorySearchResult, ModelAlias, NewUsageLog, Quota, SearchMemoriesRequest, Team,
    UpdateMemoryRequest, UsageLog, UsageLogsPage, UsageSummary, User,
};
pub use types::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Choice, Config, ContentPart,
    CreateDeploymentRequest, Deployment, ImageUrl, MessageRole, Provider, RoutingConfig,
    RoutingRule, ToolCall, ToolDefinition, UpdateRoutingConfigRequest, Usage, UsageRecord,
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
