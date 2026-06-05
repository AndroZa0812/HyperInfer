use super::{DeploymentMetrics, RoutingContext, RoutingState, RoutingStrategy};
use crate::deployment::Deployment;
use crate::error::RoutingError;
use async_trait::async_trait;
use rand::Rng;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct WeightedShuffle;

impl WeightedShuffle {
    pub fn new() -> Self {
        Self
    }

    fn effective_weight(deployment: &Deployment, metrics: &DeploymentMetrics) -> f64 {
        let base_weight = deployment.weight as f64;

        let rpm_ratio = match deployment.rpm_limit {
            Some(limit) if limit > 0 => 1.0 - (metrics.rpm_used as f64 / limit as f64),
            _ => 1.0,
        };

        let tpm_ratio = match deployment.tpm_limit {
            Some(limit) if limit > 0 => 1.0 - (metrics.tpm_used as f64 / limit as f64),
            _ => 1.0,
        };

        let capacity = rpm_ratio.min(tpm_ratio).max(0.0);
        base_weight * capacity
    }
}

impl Default for WeightedShuffle {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RoutingStrategy for WeightedShuffle {
    fn name(&self) -> &str {
        "weighted-shuffle"
    }

    async fn select<'a>(
        &self,
        _model: &str,
        candidates: &'a [Arc<Deployment>],
        state: &dyn RoutingState,
        _request: &RoutingContext,
    ) -> Result<&'a Arc<Deployment>, RoutingError> {
        if candidates.is_empty() {
            return Err(RoutingError::NoDeployments("empty candidates".into()));
        }

        let ids: Vec<&str> = candidates.iter().map(|d| d.id.as_str()).collect();
        let all_metrics = state.get_all_metrics(&ids).await?;

        let mut eligible: Vec<(usize, f64)> = Vec::new();

        for (i, deployment) in candidates.iter().enumerate() {
            if state.is_cooled_down(&deployment.id).await? {
                continue;
            }

            let metrics = all_metrics.get(&deployment.id).cloned().unwrap_or_default();

            let ew = Self::effective_weight(deployment, &metrics);
            if ew > 0.0 {
                eligible.push((i, ew));
            }
        }

        if eligible.is_empty() {
            return Err(RoutingError::NoDeployments(
                "no eligible deployments after filtering".into(),
            ));
        }

        let filtered_candidates: Vec<&Arc<Deployment>> =
            eligible.iter().map(|(i, _)| &candidates[*i]).collect();
        let weights: Vec<f64> = eligible.iter().map(|(_, w)| *w).collect();

        let selected = Self::weighted_select_owned(&filtered_candidates, &weights);
        let selected_id = &selected.id;

        candidates
            .iter()
            .find(|d| d.id == *selected_id)
            .ok_or_else(|| RoutingError::NoDeployments("selected deployment not found".into()))
    }
}

impl WeightedShuffle {
    fn weighted_select_owned(candidates: &[&Arc<Deployment>], weights: &[f64]) -> Arc<Deployment> {
        let total_weight: f64 = weights.iter().sum();
        let mut rng = rand::thread_rng();
        let mut threshold = rng.gen_range(0.0..total_weight);

        for (i, weight) in weights.iter().enumerate() {
            threshold -= weight;
            if threshold <= 0.0 {
                return Arc::clone(candidates[i]);
            }
        }

        Arc::clone(candidates.last().unwrap())
    }
}

#[cfg(test)]
pub mod tests_helpers {
    use super::super::{DeploymentMetrics, RecordFailureResult, RoutingError, RoutingState};
    use async_trait::async_trait;
    use std::collections::HashMap;

    #[derive(Debug, Clone, Default)]
    pub struct MockState {
        pub metrics: HashMap<String, DeploymentMetrics>,
        pub cooled_down: HashMap<String, bool>,
    }

    impl MockState {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_metrics(mut self, id: &str, metrics: DeploymentMetrics) -> Self {
            self.metrics.insert(id.to_string(), metrics);
            self
        }

        pub fn with_cooldown(mut self, id: &str) -> Self {
            self.cooled_down.insert(id.to_string(), true);
            self
        }
    }

    #[async_trait]
    impl RoutingState for MockState {
        async fn get_metrics(
            &self,
            deployment_id: &str,
        ) -> Result<DeploymentMetrics, RoutingError> {
            Ok(self.metrics.get(deployment_id).cloned().unwrap_or_default())
        }

        async fn get_all_metrics(
            &self,
            ids: &[&str],
        ) -> Result<HashMap<String, DeploymentMetrics>, RoutingError> {
            let mut result = HashMap::new();
            for id in ids {
                if let Some(m) = self.metrics.get(*id) {
                    result.insert(id.to_string(), m.clone());
                }
            }
            Ok(result)
        }

        async fn is_cooled_down(&self, deployment_id: &str) -> Result<bool, RoutingError> {
            Ok(self
                .cooled_down
                .get(deployment_id)
                .copied()
                .unwrap_or(false))
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
        ) -> Result<RecordFailureResult, RoutingError> {
            Ok(RecordFailureResult {
                failure_count: 0,
                cooldown_triggered: false,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::DeploymentMetrics;
    use super::tests_helpers::MockState;
    use super::*;
    use crate::deployment::Deployment;
    use hyperinfer_core::Provider;

    fn make_deployment(id: &str, weight: u32) -> Arc<Deployment> {
        let mut d = Deployment::new(
            "test-model".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            format!("key-{}", id),
        );
        d.weight = weight;
        d.id = id.to_string();
        Arc::new(d)
    }

    fn make_deployment_with_limits(
        id: &str,
        weight: u32,
        rpm_limit: Option<u64>,
        tpm_limit: Option<u64>,
    ) -> Arc<Deployment> {
        let mut d = Deployment::new(
            "test-model".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            format!("key-{}", id),
        );
        d.weight = weight;
        d.id = id.to_string();
        d.rpm_limit = rpm_limit;
        d.tpm_limit = tpm_limit;
        Arc::new(d)
    }

    #[tokio::test]
    async fn test_single_candidate() {
        let d = make_deployment("d1", 1);
        let candidates = vec![d.clone()];
        let state = MockState::new();
        let strategy = WeightedShuffle::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d1");
    }

    #[tokio::test]
    async fn test_empty_candidates() {
        let candidates: Vec<Arc<Deployment>> = vec![];
        let state = MockState::new();
        let strategy = WeightedShuffle::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RoutingError::NoDeployments(_)
        ));
    }

    #[tokio::test]
    async fn test_cooled_down_excluded() {
        let d1 = make_deployment("d1", 1);
        let d2 = make_deployment("d2", 1);
        let candidates = vec![d1, d2.clone()];
        let state = MockState::new().with_cooldown("d1");
        let strategy = WeightedShuffle::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }

    #[tokio::test]
    async fn test_all_cooled_down_returns_error() {
        let d1 = make_deployment("d1", 1);
        let d2 = make_deployment("d2", 1);
        let candidates = vec![d1, d2];
        let state = MockState::new().with_cooldown("d1").with_cooldown("d2");
        let strategy = WeightedShuffle::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RoutingError::NoDeployments(_)
        ));
    }

    #[tokio::test]
    async fn test_at_capacity_excluded() {
        let d1 = make_deployment_with_limits("d1", 5, Some(100), None);
        let d2 = make_deployment_with_limits("d2", 1, None, None);
        let candidates = vec![d1, d2.clone()];

        let metrics_d1 = DeploymentMetrics {
            rpm_used: 100,
            ..Default::default()
        };

        let state = MockState::new().with_metrics("d1", metrics_d1);
        let strategy = WeightedShuffle::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }

    #[tokio::test]
    async fn test_weight_distribution() {
        let d1 = make_deployment("d1", 9);
        let d2 = make_deployment("d2", 1);
        let candidates = vec![d1, d2];
        let state = MockState::new();
        let strategy = WeightedShuffle::new();
        let ctx = RoutingContext::default();

        let mut d1_count = 0u32;
        let iterations = 10000;

        for _ in 0..iterations {
            let result = strategy
                .select("test-model", &candidates, &state, &ctx)
                .await
                .unwrap();
            if result.id == "d1" {
                d1_count += 1;
            }
        }

        let ratio = d1_count as f64 / iterations as f64;
        assert!(
            ratio > 0.80 && ratio < 0.98,
            "expected d1 ratio between 80-98%, got {:.2}%",
            ratio * 100.0
        );
    }

    #[test]
    fn test_effective_weight_no_limits() {
        let d = make_deployment("d1", 5);
        let metrics = DeploymentMetrics::default();
        let ew = WeightedShuffle::effective_weight(&d, &metrics);
        assert!((ew - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_effective_weight_at_rpm_limit() {
        let d = make_deployment_with_limits("d1", 5, Some(100), None);
        let metrics = DeploymentMetrics {
            rpm_used: 100,
            ..Default::default()
        };
        let ew = WeightedShuffle::effective_weight(&d, &metrics);
        assert!((ew - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_effective_weight_half_capacity() {
        let d = make_deployment_with_limits("d1", 10, Some(100), None);
        let metrics = DeploymentMetrics {
            rpm_used: 50,
            ..Default::default()
        };
        let ew = WeightedShuffle::effective_weight(&d, &metrics);
        assert!((ew - 5.0).abs() < f64::EPSILON);
    }
}
