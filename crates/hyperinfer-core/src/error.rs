//! Error handling for HyperInfer
//!
//! Defines the standard error type used throughout the system.

use thiserror::Error;

/// The main error type for HyperInfer
#[derive(Error, Debug)]
pub enum HyperInferError {
    #[error("Configuration error: {0}")]
    Config(#[from] std::io::Error),

    #[error("Rate limiting error: {0}")]
    RateLimit(String),

    #[error("HTTP request failed")]
    Http(#[from] reqwest::Error),

    #[error("Database error")]
    Database(#[from] sqlx::Error),

    #[error("Redis error")]
    Redis(#[from] redis::RedisError),
}
