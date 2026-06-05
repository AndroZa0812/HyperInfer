use super::{RoutingContext, RoutingState, RoutingStrategy};
use crate::deployment::Deployment;
use crate::error::RoutingError;
use async_trait::async_trait;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeastBusy;

impl Default for LeastBusy {
    fn default() -> Self {
        Self
    }
}

impl LeastBusy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RoutingStrategy for LeastBusy {
    fn name(&self) -> &str {
        "least-busy"
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

        let mut eligible: Vec<(usize, u64)> = Vec::new();

        for (i, deployment) in candidates.iter().enumerate() {
            if state.is_cooled_down(&deployment.id).await? {
                continue;
            }

            let metrics = all_metrics.get(&deployment.id).cloned().unwrap_or_default();
            eligible.push((i, metrics.in_flight));
        }

        if eligible.is_empty() {
            return Err(RoutingError::NoDeployments(
                "no eligible deployments after filtering".into(),
            ));
        }

        let min_in_flight = eligible.iter().map(|(_, f)| *f).min().unwrap();

        let tied: Vec<(usize, u64)> = eligible
            .into_iter()
            .filter(|(_, f)| *f == min_in_flight)
            .collect();

        let weights: Vec<f64> = tied
            .iter()
            .map(|(i, _)| candidates[*i].weight as f64)
            .collect();

        let total_weight: f64 = weights.iter().sum();
        let mut rng = rand::thread_rng();
        let mut pick = rng.gen_range(0.0..total_weight);

        for (idx, weight) in weights.iter().enumerate() {
            pick -= weight;
            if pick <= 0.0 {
                let (i, _) = tied[idx];
                return Ok(&candidates[i]);
            }
        }

        let (i, _) = *tied.last().unwrap();
        Ok(&candidates[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deployment::Deployment;
    use crate::strategy::weighted_shuffle::tests_helpers::MockState;
    use crate::strategy::DeploymentMetrics;
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

    #[tokio::test]
    async fn test_selects_least_busy() {
        let d1 = make_deployment("d1", 1);
        let d2 = make_deployment("d2", 1);
        let candidates = vec![d1, d2.clone()];

        let state = MockState::new()
            .with_metrics(
                "d1",
                DeploymentMetrics {
                    in_flight: 10,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    in_flight: 2,
                    ..Default::default()
                },
            );

        let strategy = LeastBusy::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }

    #[tokio::test]
    async fn test_tie_broken_by_weight() {
        let d1 = make_deployment("d1", 9);
        let d2 = make_deployment("d2", 1);
        let candidates = vec![d1, d2];

        let state = MockState::new()
            .with_metrics(
                "d1",
                DeploymentMetrics {
                    in_flight: 5,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    in_flight: 5,
                    ..Default::default()
                },
            );

        let strategy = LeastBusy::new();
        let ctx = RoutingContext::default();

        let mut d1_count = 0u32;
        for _ in 0..5000 {
            let result = strategy
                .select("test-model", &candidates, &state, &ctx)
                .await
                .unwrap();
            if result.id == "d1" {
                d1_count += 1;
            }
        }

        let ratio = d1_count as f64 / 5000.0;
        assert!(
            ratio > 0.80,
            "d1 should win >80% with 9:1 weight, got {:.2}%",
            ratio * 100.0
        );
    }

    #[tokio::test]
    async fn test_cooled_down_excluded() {
        let d1 = make_deployment("d1", 1);
        let d2 = make_deployment("d2", 1);
        let candidates = vec![d1, d2.clone()];

        let state = MockState::new()
            .with_metrics(
                "d1",
                DeploymentMetrics {
                    in_flight: 1,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    in_flight: 10,
                    ..Default::default()
                },
            )
            .with_cooldown("d1");

        let strategy = LeastBusy::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }
}
