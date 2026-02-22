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

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    #[error("Database error")]
    Database(#[from] sqlx::Error),

    #[error("Redis error")]
    Redis(#[from] redis::RedisError),
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Invalid UUID: {0}")]
    InvalidUuid(String),
    #[error("Not found")]
    NotFound,
    #[error("Unique constraint violation: {0}")]
    UniqueViolation(String),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Configuration error: {0}")]
    Other(String),
}
