use hyperinfer_core::{ChatRequest, Deployment, Provider};
use hyperinfer_router::{
    deployment::Deployment as RouterDeployment,
    engine::{GlobalLimits, RouterEngine as HyperInferRouterEngine, RoutingResult},
    error::RoutingError,
    strategy::{
        cost_based::CostBased, latency_based::LatencyBased, least_busy::LeastBusy,
        usage_based::UsageBased, weighted_shuffle::WeightedShuffle, RoutingContext, RoutingState,
    },
};
use std::collections::HashMap;
use std::sync::Arc;

/// Client-side router engine that wraps the core routing engine
pub struct RouterEngine {
    inner: Arc<HyperInferRouterEngine>,
    state: Arc<dyn RoutingState>,
}

impl RouterEngine {
    /// Create a new router engine with default configuration and all built-in strategies
    pub async fn new() -> Self {
        let engine = HyperInferRouterEngine::new(GlobalLimits::default());
        Self::register_all_strategies(&engine).await;
        let state = Arc::new(NoopState);
        Self {
            inner: Arc::new(engine),
            state,
        }
    }

    /// Create a new router engine with custom global limits
    pub async fn with_limits(limits: GlobalLimits) -> Self {
        let engine = HyperInferRouterEngine::new(limits);
        Self::register_all_strategies(&engine).await;
        let state = Arc::new(NoopState);
        Self {
            inner: Arc::new(engine),
            state,
        }
    }

    async fn register_all_strategies(engine: &HyperInferRouterEngine) {
        engine
            .register_strategy(Box::new(WeightedShuffle::new()))
            .await;
        engine
            .register_strategy(Box::new(LatencyBased::new()))
            .await;
        engine.register_strategy(Box::new(LeastBusy::new())).await;
        engine.register_strategy(Box::new(UsageBased::new())).await;
        engine.register_strategy(Box::new(CostBased::new())).await;
    }

    /// Load deployments into the routing pool
    pub async fn load_deployments(&self, deployments: Vec<Deployment>) {
        for d in deployments {
            let router_deployment = self.core_to_router_deployment(d);
            self.inner.add_deployment(router_deployment).await;
        }
    }

    /// Rebuild the deployment pool with a fresh set
    pub async fn rebuild_pool(&self, deployments: Vec<Deployment>) {
        let router_deployments: Vec<RouterDeployment> = deployments
            .into_iter()
            .map(|d| self.core_to_router_deployment(d))
            .collect();
        self.inner.rebuild_pool(router_deployments).await;
    }

    /// Select a deployment for a request using routing strategies
    pub async fn select_deployment(
        &self,
        request: &ChatRequest,
    ) -> Result<RoutingResult, RoutingError> {
        let ctx = RoutingContext::default();
        self.inner
            .select_deployment(&request.model, self.state.as_ref(), &ctx)
            .await
    }

    /// Record a successful request for metrics
    pub async fn record_success(&self, deployment_id: &str, latency_ms: f64, tokens: u64) {
        self.inner
            .record_success(deployment_id, latency_ms, tokens, self.state.as_ref())
            .await;
    }

    /// Record a failed request for metrics
    pub async fn record_failure(&self, deployment_id: &str) {
        self.inner
            .record_failure(deployment_id, self.state.as_ref())
            .await;
    }

    /// Get the inner router engine (for advanced use)
    pub fn inner(&self) -> &Arc<HyperInferRouterEngine> {
        &self.inner
    }

    fn core_to_router_deployment(&self, d: Deployment) -> RouterDeployment {
        let provider = match d.provider.as_str() {
            "openai" => Provider::OpenAI,
            "anthropic" => Provider::Anthropic,
            _ => Provider::Other,
        };
        let mut router_deployment = RouterDeployment::new(
            d.name.clone(),
            provider,
            d.model.clone(),
            d.api_key_ref.clone(),
        );
        router_deployment.id = d.id.clone();
        if !d.base_url.is_empty() {
            router_deployment = router_deployment.with_base_url(d.base_url.clone());
        }
        router_deployment = router_deployment.with_weight(d.weight);
        if let Some(max_tpm) = d.max_tpm {
            router_deployment = router_deployment.with_tpm_limit(max_tpm as u64);
        }
        if let Some(max_rpm) = d.max_rpm {
            router_deployment = router_deployment.with_rpm_limit(max_rpm as u64);
        }
        if let Some(cost) = d.cost_per_1k_input_tokens {
            router_deployment = router_deployment.with_input_cost(cost);
        }
        if let Some(cost) = d.cost_per_1k_output_tokens {
            router_deployment = router_deployment.with_output_cost(cost);
        }
        router_deployment
    }
}

/// Noop routing state for client-side routing (metrics tracked via RedisRoutingState)
struct NoopState;

#[async_trait::async_trait]
impl RoutingState for NoopState {
    async fn get_metrics(
        &self,
        _deployment_id: &str,
    ) -> Result<hyperinfer_router::strategy::DeploymentMetrics, RoutingError> {
        Ok(hyperinfer_router::strategy::DeploymentMetrics::default())
    }

    async fn get_all_metrics(
        &self,
        _ids: &[&str],
    ) -> Result<HashMap<String, hyperinfer_router::strategy::DeploymentMetrics>, RoutingError> {
        Ok(HashMap::new())
    }

    async fn is_cooled_down(&self, _deployment_id: &str) -> Result<bool, RoutingError> {
        Ok(false)
    }

    async fn record_request_start(&self, _deployment_id: &str) -> Result<(), RoutingError> {
        Ok(())
    }

    async fn record_request_success(
        &self,
        _deployment_id: &str,
        _latency_ms: f64,
        _tokens: u64,
    ) -> Result<(), RoutingError> {
        Ok(())
    }

    async fn record_request_failure(
        &self,
        _deployment_id: &str,
    ) -> Result<hyperinfer_router::strategy::RecordFailureResult, RoutingError> {
        Ok(hyperinfer_router::strategy::RecordFailureResult {
            failure_count: 0,
            cooldown_triggered: false,
        })
    }
}
