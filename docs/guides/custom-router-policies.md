# Custom Router Policies

This guide explains how to implement custom routing logic in HyperInfer. We cover the `RoutingStrategy` trait for core engine extensions and how to control routing behavior using the Python `Config`.

## Conceptual Overview

HyperInfer's router doesn't just pick a random server; it uses **Routing Strategies** to make intelligent decisions based on real-time performance data.

### The Orchestration Loop

The `RouterEngine` manages a pool of deployments and applies a `RoutingStrategy` to select the best one.

1.  **Selection**: The `RouterEngine` calls `RoutingStrategy::select()`.
2.  **Execution**: The selected deployment is used to execute the request.
3.  **Feedback**: After the request, the engine calls `record_success()` or `record_failure()` on the `RoutingState`.
4.  **Adaptation**: The next time `select()` is called, the strategy uses that updated state to make a better decision.

---

## Implementing a Custom Strategy

The `RoutingStrategy` trait defines how a strategy selects a deployment from a list of candidates. This is the core interface for customizing the router's decision-making.

=== "Python"

    In Python, you configure routing behavior at **client-construction time** using the `Config` builder. Aliases and routing rules are passed to the Rust core during `init()`.

    ```python
    from hyperinfer import Client, Config


    async def main():
        # Build a config with aliases and routing rules
        config = (
            Config()
            .with_alias("smart-model", "gpt-4o")
            .with_routing_rule(
                name="default", priority=1, fallbacks=["gpt-4o-mini", "claude-3-haiku-20240307"]
            )
        )

        client = Client(redis_url="redis://localhost:6379", config=config)
        await client.init()

        # "smart-model" resolves to "gpt-4o" at the router level
        response = await client.chat(
            key="my-key", model="smart-model", messages=[{"role": "user", "content": "Hello!"}]
        )
        print(response)
    ```

=== "Rust"

    ```rust
    use async_trait::async_trait;
    use hyperinfer_router::strategy::{RoutingStrategy, RoutingState, RoutingContext, RoutingError};
    use hyperinfer_router::deployment::Deployment;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub struct RoundRobinStrategy {
        index: AtomicUsize,
    }

    impl RoundRobinStrategy {
        pub fn new() -> Self {
            Self { index: AtomicUsize::new(0) }
        }
    }

    #[async_trait]
    impl RoutingStrategy for RoundRobinStrategy {
        fn name(&self) -> &str {
            "round-robin"
        }

        async fn select<'a>(
            &self,
            model: &str,
            candidates: &'a [Arc<Deployment>],
            _state: &dyn RoutingState,
            _request: &RoutingContext,
        ) -> Result<&'a Arc<Deployment>, RoutingError> {
            if candidates.is_empty() {
                return Err(RoutingError::NoDeployments(model.to_string()));
            }

            // Increment index and wrap around using modulo
            let idx = self.index.fetch_add(1, Ordering::SeqCst) % candidates.len();
            Ok(&candidates[idx])
        }
    }
    ```

---

## The `select` Method in Detail

The `select` method is the heart of every strategy. It receives four critical inputs:

| Parameter | Type | Purpose |
| :--- | :--- | :--- |
| `model` | `&str` | The model name requested by the client (e.g., `"gpt-4o"`). Useful for strategy-specific logic. |
| `candidates` | `&'a [Arc<Deployment>]` | A slice of `Arc<Deployment>` containing all healthy deployments for that model. |
| `state` | `&dyn RoutingState` | The routing state trait object. Query this for latency, error rates, and quota usage. |
| `request` | `&RoutingContext` | The request context (e.g., user ID, team, virtual key). Allows for per-tenant routing. |

**Return Value**: You must return a `&'a Arc<Deployment>` — a reference to one of the entries in the `candidates` slice. The engine will then use that deployment to execute the request.

## Stateful Routing

A powerful feature of HyperInfer is **Stateful Routing**. The `RoutingState` object passed into `select` allows your strategy to be "aware" of the recent history of the deployments.

*   **Latency-Aware**: Choose the deployment with the lowest recent latency.
*   **Error-Aware**: Avoid deployments that have recently returned 5xx errors.
*   **Quota-Aware**: Balance load based on remaining token quotas.

The `RoutingState` trait provides methods to query and update deployment metrics:

| Method | Purpose |
| :--- | :--- |
| `get_metrics(deployment_id)` | Get current metrics (latency, errors, etc.) for one deployment. |
| `get_all_metrics(ids)` | Batch-fetch metrics for multiple deployments. |
| `is_cooled_down(deployment_id)` | Check if a deployment is in a cooldown period after failures. |
| `record_request_start(deployment_id)` | Mark that a request has started (for in-flight tracking). |
| `record_request_success(deployment_id, latency_ms, tokens)` | Record a successful completion. |
| `record_request_failure(deployment_id)` | Record a failure (may trigger cooldown). |

=== "Python"

    Python users benefit from stateful routing automatically — the engine feeds metrics back into the strategy after every request. The `Config` only declares the *rules*; the engine handles the *feedback loop*.

    ```python
    config = (
        Config()
        .with_alias("api-v1", "gpt-4o")
        .with_routing_rule(name="production-cluster", priority=1, fallbacks=["gpt-4o-mini"])
        .with_quota(key="my-team", rpm=1000, tpm=500_000)
    )
    ```

=== "Rust"

    ```rust
    // Reading state metrics in Rust (conceptual example)
    async fn select<'a>(
        &self,
        model: &str,
        candidates: &'a [Arc<Deployment>],
        state: &dyn RoutingState,
        _request: &RoutingContext,
    ) -> Result<&'a Arc<Deployment>, RoutingError> {
        if candidates.is_empty() {
            return Err(RoutingError::NoDeployments(model.to_string()));
        }

        // Query the state for metrics on all candidates
        let ids: Vec<&str> = candidates.iter().map(|d| d.id.as_str()).collect();
        let metrics = state.get_all_metrics(&ids).await?;

        // Pick the deployment with the lowest latency
        let best = candidates
            .iter()
            .min_by_key(|d| {
                metrics
                    .get(d.id.as_str())
                    .map(|m| m.ewma_latency_ms as u64)
                    .unwrap_or(u64::MAX)
            })
            .unwrap(); // Safe: we checked is_empty() above

        Ok(best)
    }
    ```

---

## Comparison Table

| Concept | Rust (Core Implementation) | Python (Client Configuration) |
| :--- | :--- | :--- |
| **Implementing Logic** | Implement `RoutingStrategy` trait | Not supported (Core only) |
| **Targeting Models** | `Deployment` in `DeploymentPool` | `model` or alias in `client.chat` |
| **Alias Mapping** | `set_alias` (internal) | `Config.with_alias(...)` |
| **Routing Rules** | `routing_groups` (internal) | `Config.with_routing_rule(...)` |
| **Performance Feedback** | `RoutingState::record_success` | Handled automatically by the engine |

!!! note "Future API Improvements"
    Adding ergonomic runtime methods like `client.set_alias(...)` or `client.set_routing_group(...)` is a planned follow-up. Currently, all routing configuration is set at client construction time via the `Config` builder.
