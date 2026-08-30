//! Error handling for HyperInfer
//!
//! Defines the standard error type used throughout the system.

use thiserror::Error;

/// The main error type for HyperInfer
#[derive(Error, Debug)]
pub enum HyperInferError {
    #[error("Configuration error")]
    Config(#[from] std::io::Error),

    #[error("Rate limiting error: {0}")]
    RateLimit(String),

    #[error("HTTP request failed")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    #[error("SSE parse error: {message}")]
    StreamParse { message: String, raw: String },

    #[error("Database error")]
    Database(#[from] sqlx::Error),

    #[error("Redis error")]
    Redis(#[from] redis::RedisError),

    #[error("Streaming not supported by provider")]
    UnsupportedStreaming(String),
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error")]
    Sqlx(#[from] sqlx::Error),
    #[error("Invalid UUID")]
    InvalidUuid,
    #[error("Not found")]
    NotFound,
    #[error("Unique constraint violation")]
    UniqueViolation(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Redis error")]
    Redis(#[from] redis::RedisError),
    #[error("Serialization error")]
    Serialization(#[from] serde_json::Error),
    #[error("Configuration error")]
    Other(String),
}
