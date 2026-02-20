//! Core types, traits, and error handling for HyperInfer
//!
//! This crate contains shared data structures, traits, and error definitions
//! used across the entire HyperInfer monorepo.

pub mod error;
pub mod types;
pub mod redis;
pub mod rate_limiting;

// Re-exports for convenient access
pub use error::HyperInferError;
pub use types::{ChatRequest, TokenBucket, Config};