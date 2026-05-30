pub mod deployment;
pub mod engine;
pub mod error;
pub mod fallback;
pub mod state;
pub mod strategy;

pub use deployment::{Deployment, DeploymentPool};
pub use engine::{GlobalLimits, RouterEngine, RoutingResult};
pub use error::RoutingError;
pub use fallback::{ErrorKind, FallbackConfig};
pub use state::{RedisConfig, RedisRoutingState};
pub use strategy::cost_based::CostBased;
pub use strategy::latency_based::LatencyBased;
pub use strategy::least_busy::LeastBusy;
pub use strategy::usage_based::UsageBased;
pub use strategy::weighted_shuffle::WeightedShuffle;
pub use strategy::{
    DeploymentMetrics, RecordFailureResult, RoutingContext, RoutingState, RoutingStrategy,
};
