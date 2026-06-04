use super::{DeploymentMetrics, RoutingContext, RoutingState, RoutingStrategy};
use crate::deployment::Deployment;
use crate::error::RoutingError;
use async_trait::async_trait;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyBased {
    pub ttl_secs: u64,
    pub buffer: f64,
    pub default_latency_ms: f64,
}

impl Default for LatencyBased {
    fn default() -> Self {
        Self {
            ttl_secs: 3600,
            buffer: 0.2,
            default_latency_ms: 1000.0,
        }
    }
}

impl LatencyBased {
    pub fn new() -> Self {
        Self::default()
    }
}

fn compute_global_median(metrics: &HashMap<String, DeploymentMetrics>) -> f64 {
    let mut latencies: Vec<f64> = metrics
        .values()
        .map(|m| m.latency_ewma_ms)
        .filter(|&l| l > 0.0)
        .collect();

    if latencies.is_empty() {
        return 0.0;
    }

    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let len = latencies.len();
    if len.is_multiple_of(2) {
        (latencies[len / 2 - 1] + latencies[len / 2]) / 2.0
    } else {
        latencies[len / 2]
    }
}

#[async_trait]
impl RoutingStrategy for LatencyBased {
    fn name(&self) -> &str {
        "latency-based"
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

        let global_median = compute_global_median(&all_metrics);
        let fallback_latency = if global_median > 0.0 {
            global_median
        } else {
            self.default_latency_ms
        };

        let mut eligible: Vec<(usize, f64)> = Vec::new();

        for (i, deployment) in candidates.iter().enumerate() {
            if state.is_cooled_down(&deployment.id).await? {
                continue;
            }

            let metrics = all_metrics.get(&deployment.id);
            let latency = match metrics {
                Some(m) if m.latency_ewma_ms > 0.0 => m.latency_ewma_ms,
                _ => fallback_latency,
            };

            eligible.push((i, latency));
        }

        if eligible.is_empty() {
            return Err(RoutingError::NoDeployments(
                "no eligible deployments after filtering".into(),
            ));
        }

        let best_latency = eligible
            .iter()
            .map(|(_, l)| *l)
            .fold(f64::INFINITY, f64::min);

        let threshold = best_latency * (1.0 + self.buffer);

        let within_threshold: Vec<(usize, f64)> = eligible
            .into_iter()
            .filter(|(_, l)| *l <= threshold)
            .collect();

        let weights: Vec<f64> = within_threshold
            .iter()
            .map(|(i, _)| candidates[*i].weight as f64)
            .collect();

        let total_weight: f64 = weights.iter().sum();
        if total_weight <= 0.0 {
            let (i, _) = within_threshold[0];
            return Ok(&candidates[i]);
        }
        let mut rng = rand::thread_rng();
        let mut pick = rng.gen_range(0.0..total_weight);

        for (idx, weight) in weights.iter().enumerate() {
            pick -= weight;
            if pick <= 0.0 {
                let (i, _) = within_threshold[idx];
                return Ok(&candidates[i]);
            }
        }

        let (i, _) = *within_threshold.last().unwrap();
        Ok(&candidates[i])
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

    #[tokio::test]
    async fn test_selects_lowest_latency() {
        let d1 = make_deployment("d1", 1);
        let d2 = make_deployment("d2", 1);
        let candidates = vec![d1, d2.clone()];

        let state = MockState::new()
            .with_metrics(
                "d1",
                DeploymentMetrics {
                    latency_ewma_ms: 200.0,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    latency_ewma_ms: 50.0,
                    ..Default::default()
                },
            );

        let strategy = LatencyBased::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }

    #[tokio::test]
    async fn test_buffer_includes_near_candidates() {
        let d1 = make_deployment("d1", 1);
        let d2 = make_deployment("d2", 1);
        let d3 = make_deployment("d3", 1);
        let candidates = vec![d1, d2, d3];

        let state = MockState::new()
            .with_metrics(
                "d1",
                DeploymentMetrics {
                    latency_ewma_ms: 100.0,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    latency_ewma_ms: 115.0,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d3",
                DeploymentMetrics {
                    latency_ewma_ms: 500.0,
                    ..Default::default()
                },
            );

        let strategy = LatencyBased {
            buffer: 0.2,
            ..Default::default()
        };
        let ctx = RoutingContext::default();

        let mut d3_count = 0u32;
        for _ in 0..1000 {
            let result = strategy
                .select("test-model", &candidates, &state, &ctx)
                .await
                .unwrap();
            if result.id == "d3" {
                d3_count += 1;
            }
        }

        assert_eq!(d3_count, 0, "d3 should never be selected with buffer=0.2");
    }

    #[tokio::test]
    async fn test_cold_start_uses_global_median() {
        let d1 = make_deployment("d1", 1);
        let d2 = make_deployment("d2", 1);
        let d_new = make_deployment("d_new", 1);
        let candidates = vec![d1, d2, d_new];

        let state = MockState::new()
            .with_metrics(
                "d1",
                DeploymentMetrics {
                    latency_ewma_ms: 100.0,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    latency_ewma_ms: 110.0,
                    ..Default::default()
                },
            );

        let strategy = LatencyBased::new();
        let ctx = RoutingContext::default();

        let mut new_count = 0u32;
        for _ in 0..1000 {
            let result = strategy
                .select("test-model", &candidates, &state, &ctx)
                .await
                .unwrap();
            if result.id == "d_new" {
                new_count += 1;
            }
        }

        assert!(
            (200..=800).contains(&new_count),
            "new deployment should get significant traffic, got {}",
            new_count
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
                    latency_ewma_ms: 50.0,
                    ..Default::default()
                },
            )
            .with_metrics(
                "d2",
                DeploymentMetrics {
                    latency_ewma_ms: 200.0,
                    ..Default::default()
                },
            )
            .with_cooldown("d1");

        let strategy = LatencyBased::new();
        let ctx = RoutingContext::default();

        let result = strategy
            .select("test-model", &candidates, &state, &ctx)
            .await
            .unwrap();
        assert_eq!(result.id, "d2");
    }

    #[test]
    fn test_global_median_odd() {
        let mut metrics = HashMap::new();
        metrics.insert(
            "a".to_string(),
            DeploymentMetrics {
                latency_ewma_ms: 100.0,
                ..Default::default()
            },
        );
        metrics.insert(
            "b".to_string(),
            DeploymentMetrics {
                latency_ewma_ms: 200.0,
                ..Default::default()
            },
        );
        metrics.insert(
            "c".to_string(),
            DeploymentMetrics {
                latency_ewma_ms: 300.0,
                ..Default::default()
            },
        );

        let median = compute_global_median(&metrics);
        assert!((median - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_global_median_even() {
        let mut metrics = HashMap::new();
        metrics.insert(
            "a".to_string(),
            DeploymentMetrics {
                latency_ewma_ms: 100.0,
                ..Default::default()
            },
        );
        metrics.insert(
            "b".to_string(),
            DeploymentMetrics {
                latency_ewma_ms: 200.0,
                ..Default::default()
            },
        );
        metrics.insert(
            "c".to_string(),
            DeploymentMetrics {
                latency_ewma_ms: 300.0,
                ..Default::default()
            },
        );
        metrics.insert(
            "d".to_string(),
            DeploymentMetrics {
                latency_ewma_ms: 400.0,
                ..Default::default()
            },
        );

        let median = compute_global_median(&metrics);
        assert!((median - 250.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_global_median_empty() {
        let metrics = HashMap::new();
        let median = compute_global_median(&metrics);
        assert!((median - 0.0).abs() < f64::EPSILON);
    }
}
