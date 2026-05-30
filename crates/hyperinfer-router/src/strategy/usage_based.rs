use super::{RoutingContext, RoutingState, RoutingStrategy};
use crate::deployment::Deployment;
use crate::error::RoutingError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageBased;

impl Default for UsageBased {
    fn default() -> Self {
        Self
    }
}

impl UsageBased {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RoutingStrategy for UsageBased {
    fn name(&self) -> &str {
        "usage-based"
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

            let utilization = match deployment.tpm_limit {
                Some(limit) if limit > 0 => metrics.tpm_used as f64 / limit as f64,
                _ => metrics.tpm_used as f64,
            };

            eligible.push((i, utilization));
        }

        if eligible.is_empty() {
            return Err(RoutingError::NoDeployments(
                "no eligible deployments after filtering".into(),
            ));
        }

        let best_idx = eligible
            .iter()
            .enumerate()
            .min_by(|(_, (_, a)), (_, (_, b))| {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)
            .unwrap();

        let (i, _) = eligible[best_idx];
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

    fn make_deployment(id: &str, weight: u32, tpm_limit: Option<u64>) -> Arc<Deployment> {
        let mut d = Deployment::new(
            "test-model".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            format!("key-{}", id),
        );
        d.weight = weight;
        d.id = id.to_string();
        d.tpm_limit = tpm_limit;
        Arc::new(d)
    }

    #[tokio::test]
    async fn test_selects_lowest_utilization() {
        let d1 = make_deployment("d1", 1, Some(1000));
        let d2 = make_deployment("d2", 1, Some(1000));
        let candidates = vec![d1, d2.clone()];

        let state = MockState::new()
            .with_metrics(
                "d1",
                DeploymentMetrics {
                    tpm_used: 800,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    tpm_used: 200,
                    ..Default::default()
                },
            );

        let strategy = UsageBased::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }

    #[tokio::test]
    async fn test_ratio_not_absolute() {
        let d_big = make_deployment("d_big", 1, Some(100000));
        let d_small = make_deployment("d_small", 1, Some(1000));
        let candidates = vec![d_big.clone(), d_small];

        let state = MockState::new()
            .with_metrics(
                "d_big",
                DeploymentMetrics {
                    tpm_used: 50000,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d_small",
                DeploymentMetrics {
                    tpm_used: 900,
                    ..Default::default()
                },
            );

        let strategy = UsageBased::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d_big");
    }

    #[tokio::test]
    async fn test_cooled_down_excluded() {
        let d1 = make_deployment("d1", 1, Some(1000));
        let d2 = make_deployment("d2", 1, Some(1000));
        let candidates = vec![d1, d2.clone()];

        let state = MockState::new()
            .with_metrics(
                "d1",
                DeploymentMetrics {
                    tpm_used: 100,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    tpm_used: 800,
                    ..Default::default()
                },
            )
            .with_cooldown("d1");

        let strategy = UsageBased::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }
}
