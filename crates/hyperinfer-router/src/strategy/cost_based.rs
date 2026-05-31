use super::{RoutingContext, RoutingState, RoutingStrategy};
use crate::deployment::Deployment;
use crate::error::RoutingError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBased;

impl Default for CostBased {
    fn default() -> Self {
        Self
    }
}

impl CostBased {
    pub fn new() -> Self {
        Self
    }
}

fn estimate_cost(deployment: &Deployment, ctx: &RoutingContext) -> f64 {
    let input_tokens = ctx.estimated_input_tokens.unwrap_or(0) as f64;
    let output_tokens = ctx.estimated_output_tokens.unwrap_or(0) as f64;

    let input_cost = deployment.input_cost_per_1k.unwrap_or(0.0);
    let output_cost = deployment.output_cost_per_1k.unwrap_or(0.0);

    (input_tokens / 1000.0) * input_cost + (output_tokens / 1000.0) * output_cost
}

#[async_trait]
impl RoutingStrategy for CostBased {
    fn name(&self) -> &str {
        "cost-based"
    }

    async fn select<'a>(
        &self,
        _model: &str,
        candidates: &'a [Arc<Deployment>],
        state: &dyn RoutingState,
        request: &RoutingContext,
    ) -> Result<&'a Arc<Deployment>, RoutingError> {
        if candidates.is_empty() {
            return Err(RoutingError::NoDeployments("empty candidates".into()));
        }

        let ids: Vec<&str> = candidates.iter().map(|d| d.id.as_str()).collect();
        let all_metrics = state.get_all_metrics(&ids).await?;

        let mut eligible: Vec<usize> = Vec::new();

        for (i, deployment) in candidates.iter().enumerate() {
            if state.is_cooled_down(&deployment.id).await? {
                continue;
            }

            let metrics = all_metrics.get(&deployment.id).cloned().unwrap_or_default();

            if let Some(rpm_limit) = deployment.rpm_limit {
                if metrics.rpm_used >= rpm_limit {
                    continue;
                }
            }

            if let Some(tpm_limit) = deployment.tpm_limit {
                if metrics.tpm_used >= tpm_limit {
                    continue;
                }
            }

            eligible.push(i);
        }

        if eligible.is_empty() {
            return Err(RoutingError::NoDeployments(
                "no eligible deployments after filtering".into(),
            ));
        }

        let mut best_idx = eligible[0];
        let mut best_cost = estimate_cost(&candidates[best_idx], request);
        let mut best_weight = candidates[best_idx].weight;

        for &i in eligible.iter().skip(1) {
            let cost = estimate_cost(&candidates[i], request);
            let weight = candidates[i].weight;

            if cost < best_cost || ((cost - best_cost).abs() < f64::EPSILON && weight > best_weight)
            {
                best_idx = i;
                best_cost = cost;
                best_weight = weight;
            }
        }

        Ok(&candidates[best_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deployment::Deployment;
    use crate::strategy::weighted_shuffle::tests_helpers::MockState;
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

    fn make_deployment_with_costs(
        id: &str,
        weight: u32,
        input_cost: f64,
        output_cost: f64,
    ) -> Arc<Deployment> {
        let mut d = Deployment::new(
            "test-model".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            format!("key-{}", id),
        );
        d.weight = weight;
        d.id = id.to_string();
        d.input_cost_per_1k = Some(input_cost);
        d.output_cost_per_1k = Some(output_cost);
        Arc::new(d)
    }

    #[tokio::test]
    async fn test_selects_cheapest() {
        let d1 = make_deployment_with_costs("d1", 1, 0.03, 0.06);
        let d2 = make_deployment_with_costs("d2", 1, 0.01, 0.02);
        let candidates = vec![d1, d2.clone()];

        let state = MockState::new();
        let strategy = CostBased::new();
        let ctx = RoutingContext {
            estimated_input_tokens: Some(1000),
            estimated_output_tokens: Some(1000),
            ..Default::default()
        };

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }

    #[tokio::test]
    async fn test_equal_cost_prefers_higher_weight() {
        let d1 = make_deployment_with_costs("d1", 5, 0.01, 0.02);
        let d2 = make_deployment_with_costs("d2", 1, 0.01, 0.02);
        let candidates = vec![d1.clone(), d2];

        let state = MockState::new();
        let strategy = CostBased::new();
        let ctx = RoutingContext {
            estimated_input_tokens: Some(1000),
            estimated_output_tokens: Some(1000),
            ..Default::default()
        };

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d1");
    }

    #[tokio::test]
    async fn test_no_cost_data_returns_first() {
        let d1 = make_deployment("d1", 1);
        let d2 = make_deployment("d2", 1);
        let candidates = vec![d1.clone(), d2];

        let state = MockState::new();
        let strategy = CostBased::new();
        let ctx = RoutingContext {
            estimated_input_tokens: Some(1000),
            estimated_output_tokens: Some(1000),
            ..Default::default()
        };

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d1");
    }

    #[test]
    fn test_estimate_cost_calculation() {
        let d = make_deployment_with_costs("d1", 1, 0.01, 0.03);
        let ctx = RoutingContext {
            estimated_input_tokens: Some(2000),
            estimated_output_tokens: Some(1000),
            ..Default::default()
        };

        let cost = estimate_cost(&d, &ctx);
        assert!((cost - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_estimate_cost_no_prices() {
        let d = make_deployment("d1", 1);
        let ctx = RoutingContext {
            estimated_input_tokens: Some(2000),
            estimated_output_tokens: Some(1000),
            ..Default::default()
        };

        let cost = estimate_cost(&d, &ctx);
        assert!((cost - 0.0).abs() < f64::EPSILON);
    }
}
