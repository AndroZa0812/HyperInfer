//! Core types, traits, and error handling for HyperInfer
//!
//! This crate contains shared data structures, traits, and error definitions
//! used across the entire HyperInfer monorepo.

pub mod error;
pub mod rate_limiting;
pub mod redis;
pub mod types;

// Re-exports for convenient access
pub use error::HyperInferError;
pub use rate_limiting::RateLimiter;
pub use types::{
    ChatMessage, ChatRequest, ChatResponse, Config, MessageRole, Provider, Quota, RoutingRule,
    Usage,
};
