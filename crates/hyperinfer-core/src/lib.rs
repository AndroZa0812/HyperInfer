//! Core types, traits, and error handling for HyperInfer
//!
//! This crate contains shared data structures, traits, and error definitions
//! used across the entire HyperInfer monorepo.

pub mod error;
pub mod rate_limiting;
pub mod redis;
pub mod telemetry_consumer;
pub mod traits;
pub mod types;

pub use error::{ConfigError, DbError, HyperInferError};
pub use rate_limiting::RateLimiter;
pub use redis::PolicyUpdate;
pub use telemetry_consumer::TelemetryConsumer;
pub use traits::{ApiKey, ConfigStore, Database, ModelAlias, Quota, Team, UsageLog, User};
pub use types::{
    ChatMessage, ChatRequest, ChatResponse, Choice, Config, MessageRole, Provider, RoutingRule,
    Usage, UsageRecord,
};
