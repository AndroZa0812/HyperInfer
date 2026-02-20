//! Error handling for HyperInfer
//!
//! Defines the standard error type used throughout the system.

use thiserror::Error;

/// The main error type for HyperInfer
#[derive(Error, Debug)]
pub enum HyperInferError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Rate limiting error: {0}")]
    RateLimit(String),

    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Redis error: {0}")]
    Redis(String),
}
