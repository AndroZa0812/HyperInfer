use crate::deployment::{Deployment, DeploymentPool};
use crate::error::RoutingError;
use crate::fallback::{ErrorKind, FallbackConfig};
use crate::strategy::{RoutingContext, RoutingState, RoutingStrategy};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct GlobalLimits {
    pub max_total_attempts: u32,
    pub global_timeout_ms: u64,
}

impl Default for GlobalLimits {
    fn default() -> Self {
        Self {
            max_total_attempts: 10,
            global_timeout_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoutingResult {
    pub deployment: Arc<Deployment>,
    pub attempt: u32,
    pub fallback_chain: Vec<String>,
}

pub struct RouterEngine {
    pool: Arc<RwLock<DeploymentPool>>,
    strategies: Arc<RwLock<HashMap<String, Box<dyn RoutingStrategy>>>>,
    default_strategy: Arc<RwLock<String>>,
    fallback_config: Arc<RwLock<FallbackConfig>>,
    global_limits: GlobalLimits,
    model_aliases: Arc<RwLock<HashMap<String, String>>>,
    routing_groups: Arc<RwLock<HashMap<String, String>>>,
}

impl RouterEngine {
    pub fn new(global_limits: GlobalLimits) -> Self {
        Self {
            pool: Arc::new(RwLock::new(DeploymentPool::new())),
            strategies: Arc::new(RwLock::new(HashMap::new())),
            default_strategy: Arc::new(RwLock::new("weighted-shuffle".to_string())),
            fallback_config: Arc::new(RwLock::new(FallbackConfig::new())),
            global_limits,
            model_aliases: Arc::new(RwLock::new(HashMap::new())),
            routing_groups: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_deployment(&self, deployment: Deployment) {
        let mut pool = self.pool.write().await;
        pool.add(deployment);
    }

    pub async fn remove_deployment(&self, deployment_id: &str) {
        let mut pool = self.pool.write().await;
        pool.remove(deployment_id);
    }

    pub async fn rebuild_pool(&self, deployments: Vec<Deployment>) {
        let mut pool = self.pool.write().await;
        *pool = DeploymentPool::new();
        for d in deployments {
            pool.add(d);
        }
    }

    pub async fn register_strategy(&self, strategy: Box<dyn RoutingStrategy>) {
        let name = strategy.name().to_string();
        let mut strategies = self.strategies.write().await;
        strategies.insert(name, strategy);
    }

    pub async fn set_default_strategy(&self, name: &str) {
        let mut ds = self.default_strategy.write().await;
        *ds = name.to_string();
    }

    pub async fn set_fallback_config(&self, config: FallbackConfig) {
        let mut fc = self.fallback_config.write().await;
        *fc = config;
    }

    pub async fn set_alias(&self, alias: &str, target: &str) {
        let mut aliases = self.model_aliases.write().await;
        aliases.insert(alias.to_string(), target.to_string());
    }

    pub async fn set_routing_group(&self, model: &str, strategy_name: &str) {
        let mut groups = self.routing_groups.write().await;
        groups.insert(model.to_string(), strategy_name.to_string());
    }

    pub async fn record_success(
        &self,
        deployment_id: &str,
        latency_ms: f64,
        tokens: u64,
        state: &dyn RoutingState,
    ) {
        let _ = state
            .record_request_success(deployment_id, latency_ms, tokens)
            .await;
    }

    pub async fn record_failure(&self, deployment_id: &str, state: &dyn RoutingState) {
        let _ = state.record_request_failure(deployment_id).await;
    }

    pub fn resolve_alias(model: &str, aliases: &HashMap<String, String>) -> String {
        aliases
            .get(model)
            .cloned()
            .unwrap_or_else(|| model.to_string())
    }

    pub async fn select_deployment(
        &self,
        model: &str,
        state: &dyn RoutingState,
        ctx: &RoutingContext,
    ) -> Result<RoutingResult, RoutingError> {
        let aliases = self.model_aliases.read().await;
        let resolved_model = Self::resolve_alias(model, &aliases);
        drop(aliases);

        let pool = self.pool.read().await;
        let candidates = pool
            .get(&resolved_model)
            .ok_or_else(|| RoutingError::NoDeployments(resolved_model.clone()))?;

        let candidates_vec: Vec<Arc<Deployment>> = candidates.to_vec();

        if candidates_vec.is_empty() {
            return Err(RoutingError::NoDeployments(resolved_model.clone()));
        }

        let strategy_name = {
            let groups = self.routing_groups.read().await;
            if let Some(name) = groups.get(&resolved_model) {
                name.clone()
            } else {
                let ds = self.default_strategy.read().await;
                ds.clone()
            }
        };

        let strategies = self.strategies.read().await;
        let strategy = strategies.get(&strategy_name).ok_or_else(|| {
            RoutingError::StrategyError(format!("strategy '{}' not found", strategy_name))
        })?;

        let selected = strategy
            .select(&resolved_model, &candidates_vec, state, ctx)
            .await?;

        Ok(RoutingResult {
            deployment: Arc::clone(selected),
            attempt: 1,
            fallback_chain: vec![resolved_model],
        })
    }

    pub async fn route_with_fallback<T>(
        &self,
        model: &str,
        state: &dyn RoutingState,
        ctx: &RoutingContext,
        executor: impl Fn(
                Arc<Deployment>,
            )
                -> Pin<Box<dyn Future<Output = Result<T, hyperinfer_core::HyperInferError>> + Send>>
            + Send,
    ) -> Result<(RoutingResult, T), RoutingError> {
        let start_time = Instant::now();
        let mut total_attempts: u32 = 0;

        let aliases = self.model_aliases.read().await;
        let initial_model = Self::resolve_alias(model, &aliases);
        drop(aliases);

        let mut current_model = initial_model.clone();
        let mut fallback_chain = vec![current_model.clone()];
        let mut excluded_ids: HashSet<String> = HashSet::new();
        let mut fallback_models_tried: HashSet<String> = HashSet::new();

        let fallback_config = self.fallback_config.read().await.clone();

        loop {
            if total_attempts >= self.global_limits.max_total_attempts {
                return Err(RoutingError::MaxAttemptsExceeded {
                    attempts: total_attempts,
                });
            }

            if start_time.elapsed().as_millis() as u64 >= self.global_limits.global_timeout_ms {
                return Err(RoutingError::GlobalTimeout {
                    timeout_ms: self.global_limits.global_timeout_ms,
                });
            }

            let candidates = {
                let pool = self.pool.read().await;
                pool.get(&current_model).map(|c| c.to_vec())
            };

            let candidates = match candidates {
                Some(c) if !c.is_empty() => c,
                _ => {
                    let fallbacks =
                        fallback_config.get_fallbacks(&current_model, &ErrorKind::Other);
                    let next = fallbacks
                        .into_iter()
                        .find(|m| !fallback_models_tried.contains(m));
                    match next {
                        Some(m) => {
                            fallback_models_tried.insert(m.clone());
                            current_model = m.clone();
                            fallback_chain.push(m);
                            continue;
                        }
                        None => {
                            return Err(RoutingError::AllDeploymentsFailed(initial_model));
                        }
                    }
                }
            };

            let eligible: Vec<Arc<Deployment>> = candidates
                .into_iter()
                .filter(|d| !excluded_ids.contains(&d.id))
                .collect();

            if eligible.is_empty() {
                let fallbacks = fallback_config.get_fallbacks(&current_model, &ErrorKind::Other);
                let next = fallbacks
                    .into_iter()
                    .find(|m| !fallback_models_tried.contains(m));
                match next {
                    Some(m) => {
                        fallback_models_tried.insert(m.clone());
                        current_model = m.clone();
                        fallback_chain.push(m);
                        continue;
                    }
                    None => {
                        return Err(RoutingError::AllDeploymentsFailed(initial_model));
                    }
                }
            }

            let strategy_name = {
                let groups = self.routing_groups.read().await;
                if let Some(name) = groups.get(&current_model) {
                    name.clone()
                } else {
                    let ds = self.default_strategy.read().await;
                    ds.clone()
                }
            };

            let selected = {
                let strategies = self.strategies.read().await;
                let strategy = strategies.get(&strategy_name).ok_or_else(|| {
                    RoutingError::StrategyError(format!("strategy '{}' not found", strategy_name))
                })?;
                strategy
                    .select(&current_model, &eligible, state, ctx)
                    .await?
                    .clone()
            };

            total_attempts += 1;
            let _ = state.record_request_start(&selected.id).await;

            match executor(Arc::clone(&selected)).await {
                Ok(value) => {
                    return Ok((
                        RoutingResult {
                            deployment: selected,
                            attempt: total_attempts,
                            fallback_chain,
                        },
                        value,
                    ));
                }
                Err(err) => {
                    let error_kind = ErrorKind::classify(&err);
                    let _ = state.record_request_failure(&selected.id).await;
                    excluded_ids.insert(selected.id.clone());

                    let same_model_candidates = {
                        let pool = self.pool.read().await;
                        pool.get(&current_model).map(|c| c.to_vec())
                    };

                    let same_model_remaining = same_model_candidates
                        .map(|c| {
                            c.into_iter()
                                .filter(|d| !excluded_ids.contains(&d.id))
                                .count()
                        })
                        .unwrap_or(0);

                    if same_model_remaining > 0
                        && total_attempts < self.global_limits.max_total_attempts
                    {
                        continue;
                    }

                    let fallbacks = fallback_config.get_fallbacks(&current_model, &error_kind);
                    let next = fallbacks
                        .into_iter()
                        .find(|m| !fallback_models_tried.contains(m));
                    match next {
                        Some(m) => {
                            fallback_models_tried.insert(m.clone());
                            current_model = m.clone();
                            fallback_chain.push(m);
                            excluded_ids.clear();
                        }
                        None => {
                            return Err(RoutingError::AllDeploymentsFailed(initial_model));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deployment::Deployment;
    use crate::strategy::weighted_shuffle::tests_helpers::MockState;
    use crate::strategy::weighted_shuffle::WeightedShuffle;
    use crate::strategy::RoutingStrategyExt;
    use hyperinfer_core::Provider;
    use std::sync::Arc;

    fn make_deployment(model_name: &str, id: &str) -> Deployment {
        let mut d = Deployment::new(
            model_name.to_string(),
            Provider::OpenAI,
            model_name.to_string(),
            format!("key-{}", id),
        );
        d.id = id.to_string();
        d
    }

    #[tokio::test]
    async fn test_select_deployment_basic() {
        let engine = RouterEngine::new(GlobalLimits::default());
        engine
            .register_strategy(WeightedShuffle::new().boxed())
            .await;
        engine.add_deployment(make_deployment("gpt-4", "d1")).await;

        let state = MockState::new();
        let ctx = RoutingContext::default();
        let result = engine
            .select_deployment("gpt-4", &state, &ctx)
            .await
            .unwrap();

        assert_eq!(result.deployment.id, "d1");
        assert_eq!(result.attempt, 1);
        assert_eq!(result.fallback_chain, vec!["gpt-4"]);
    }

    #[tokio::test]
    async fn test_alias_resolution() {
        let engine = RouterEngine::new(GlobalLimits::default());
        engine
            .register_strategy(WeightedShuffle::new().boxed())
            .await;
        engine
            .add_deployment(make_deployment("gpt-4-turbo", "d1"))
            .await;
        engine.set_alias("smart", "gpt-4-turbo").await;

        let state = MockState::new();
        let ctx = RoutingContext::default();
        let result = engine
            .select_deployment("smart", &state, &ctx)
            .await
            .unwrap();

        assert_eq!(result.deployment.id, "d1");
        assert_eq!(result.deployment.model_name, "gpt-4-turbo");
    }

    #[tokio::test]
    async fn test_no_deployments_error() {
        let engine = RouterEngine::new(GlobalLimits::default());
        engine
            .register_strategy(WeightedShuffle::new().boxed())
            .await;

        let state = MockState::new();
        let ctx = RoutingContext::default();
        let result = engine.select_deployment("nonexistent", &state, &ctx).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RoutingError::NoDeployments(_)
        ));
    }

    #[tokio::test]
    async fn test_routing_group_strategy_selection() {
        let engine = RouterEngine::new(GlobalLimits::default());
        engine
            .register_strategy(WeightedShuffle::new().boxed())
            .await;
        engine
            .register_strategy(Box::new(WeightedShuffle::new()))
            .await;
        engine.add_deployment(make_deployment("gpt-4", "d1")).await;

        engine.set_routing_group("gpt-4", "weighted-shuffle").await;

        let state = MockState::new();
        let ctx = RoutingContext::default();
        let result = engine
            .select_deployment("gpt-4", &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.deployment.id, "d1");
    }

    #[tokio::test]
    async fn test_global_limits_max_attempts() {
        let limits = GlobalLimits {
            max_total_attempts: 2,
            global_timeout_ms: 60_000,
        };
        let engine = RouterEngine::new(limits);
        engine
            .register_strategy(WeightedShuffle::new().boxed())
            .await;
        engine.add_deployment(make_deployment("gpt-4", "d1")).await;
        engine.add_deployment(make_deployment("gpt-4", "d2")).await;

        let state = MockState::new();
        let ctx = RoutingContext::default();

        let executor = |_d: Arc<Deployment>| -> Pin<
            Box<dyn Future<Output = Result<(), hyperinfer_core::HyperInferError>> + Send>,
        > {
            Box::pin(async {
                Err::<(), _>(hyperinfer_core::HyperInferError::ApiError {
                    status: 500,
                    message: "fail".into(),
                })
            })
        };

        let result = engine
            .route_with_fallback::<()>("gpt-4", &state, &ctx, executor)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                RoutingError::AllDeploymentsFailed(_) | RoutingError::MaxAttemptsExceeded { .. }
            ),
            "expected AllDeploymentsFailed or MaxAttemptsExceeded, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_record_success_passthrough() {
        let engine = RouterEngine::new(GlobalLimits::default());
        let state = MockState::new();
        engine.record_success("d1", 100.0, 500, &state).await;
    }

    #[tokio::test]
    async fn test_record_failure_passthrough() {
        let engine = RouterEngine::new(GlobalLimits::default());
        let state = MockState::new();
        engine.record_failure("d1", &state).await;
    }
}
