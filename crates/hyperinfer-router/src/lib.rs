pub mod deployment;
pub mod error;
pub mod state;
pub mod strategy;

pub use deployment::{Deployment, DeploymentPool};
pub use error::RoutingError;
pub use state::{RedisConfig, RedisRoutingState};
pub use strategy::weighted_shuffle::WeightedShuffle;
pub use strategy::{
    DeploymentMetrics, RecordFailureResult, RoutingContext, RoutingState, RoutingStrategy,
};
