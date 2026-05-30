pub mod latency_based;
pub mod least_busy;
pub mod weighted_shuffle;

use crate::deployment::Deployment;
use crate::error::RoutingError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeploymentMetrics {
    pub latency_ewma_ms: f64,
    pub in_flight: u64,
    pub tpm_used: u64,
    pub rpm_used: u64,
    pub total_requests: u64,
    pub total_failures: u64,
    pub last_failure_ts: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct RoutingContext {
    pub estimated_input_tokens: Option<u64>,
    pub estimated_output_tokens: Option<u64>,
    pub team_id: Option<String>,
}

#[async_trait]
pub trait RoutingStrategy: Send + Sync + dyn_clone::DynClone {
    fn name(&self) -> &str;

    async fn select<'a>(
        &self,
        model: &str,
        candidates: &'a [Arc<Deployment>],
        state: &dyn RoutingState,
        request: &RoutingContext,
    ) -> Result<&'a Arc<Deployment>, RoutingError>;
}

dyn_clone::clone_trait_object!(RoutingStrategy);

#[async_trait]
pub trait RoutingState: Send + Sync {
    async fn get_metrics(&self, deployment_id: &str) -> Result<DeploymentMetrics, RoutingError>;

    async fn get_all_metrics(
        &self,
        ids: &[&str],
    ) -> Result<HashMap<String, DeploymentMetrics>, RoutingError>;

    async fn is_cooled_down(&self, deployment_id: &str) -> Result<bool, RoutingError>;

    async fn record_request_start(&self, deployment_id: &str) -> Result<(), RoutingError>;

    async fn record_request_success(
        &self,
        deployment_id: &str,
        latency_ms: f64,
        tokens: u64,
    ) -> Result<(), RoutingError>;

    async fn record_request_failure(
        &self,
        deployment_id: &str,
    ) -> Result<RecordFailureResult, RoutingError>;
}

#[derive(Debug, Clone)]
pub struct RecordFailureResult {
    pub failure_count: u64,
    pub cooldown_triggered: bool,
}

#[async_trait]
pub trait RoutingStrategyExt: RoutingStrategy {
    fn boxed(self) -> Box<dyn RoutingStrategy>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

impl<T: RoutingStrategy + 'static> RoutingStrategyExt for T {}
