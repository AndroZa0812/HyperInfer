use thiserror::Error;

#[derive(Error, Debug)]
pub enum RoutingError {
    #[error("no deployments available for model: {0}")]
    NoDeployments(String),

    #[error("all deployments failed for model: {0}")]
    AllDeploymentsFailed(String),

    #[error("max attempts exceeded ({attempts} attempts)")]
    MaxAttemptsExceeded { attempts: u32 },

    #[error("global routing timeout ({timeout_ms}ms)")]
    GlobalTimeout { timeout_ms: u64 },

    #[error("strategy error: {0}")]
    StrategyError(String),

    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("provider error: status={status}, message={message}")]
    ProviderError { status: u16, message: String },

    #[error("executor panicked during request processing")]
    ExecutorPanic,
}
