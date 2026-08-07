# Routing Guide

HyperInfer's routing engine is a stateful, policy-driven system that decides which deployment handles a given request. This guide covers the built-in strategies, their tuning parameters, and how they integrate with fallbacks.

## 1. Built-in Routing Strategies

HyperInfer ships with several production-ready strategies. Each one optimizes for a different constraint (latency, cost, utilization).

### 1.1. Weighted Shuffle (Default)

Distributes requests across deployments based on static weights, adjusted dynamically by remaining capacity.

*   **Best for**: General load balancing across heterogeneous providers (e.g., 70% to GPT-4, 30% to Claude).
*   **Key Parameters**:
    *   `weight` (int): Static traffic weight assigned per deployment.

### 1.2. Latency-Based (EWMA)

Selects the deployment with the lowest **EWMA** (Exponentially Weighted Moving Average) latency.

*   **Best for**: User-facing applications where Time-To-First-Token (TTFT) matters.
*   **Key Parameters**:

    | Parameter | Type | Default | Behavioral Impact |
    | :--- | :--- | :--- | :--- |
    | `latency_buffer` | `float` | `0.1` | Extra "padding" added to EWMA. Higher values = more conservative switching, prevents flapping. |
    | `latency_ttl_secs` | `int` | `300` | Time-to-live for the latency cache. After this, deployments are considered fresh. |

*   **Configuration Example**:

    Routing strategies are configured server-side via the Control Plane or deployment config. Python clients consume the configured strategy transparently.

    === "JSON"

        ```json
        {
          "strategy": "latency_based",
          "latency_buffer": 0.1,
          "latency_ttl_secs": 300
        }
        ```

### 1.3. Least-Busy

Selects the deployment with the fewest in-flight requests.

*   **Best for**: Bursty workloads, ensuring no single deployment gets overwhelmed.
*   **Key Parameters**: None (uses atomic Redis counters internally).

### 1.4. Usage-Based (Quota-Aware)

Selects the deployment with the lowest token usage relative to its configured limit.

*   **Best for**: Budget enforcement and strict rate limits (e.g., monthly TPM caps).
*   **Key Parameters**:
    *   `quota_period` (str): `"minute"`, `"hour"`, or `"day"`. The window for usage tracking.

### 1.5. Cost-Based

Selects the cheapest deployment based on estimated input/output token costs.

*   **Best for**: Cost optimization when running non-critical batch jobs.
*   **Key Parameters**:
    *   `input_cost_per_1k` (float): Cost per 1,000 input tokens.
    *   `output_cost_per_1k` (float): Cost per 1,000 output tokens.

---

## 2. Custom Routing Strategies

!!! info "Deep Dive Available"
    This section covers the built-in strategies and their configuration. For a complete guide on **implementing your own custom routing strategy** (including the `RoutingStrategy` trait, the `select` method, stateful routing, and registration), see the dedicated guide:

    **→ [Custom Router Policies](custom-router-policies.md)**

    There you will find:

    *   A full Rust implementation example (`RoundRobinStrategy`).
    *   A detailed breakdown of the `select` method parameters.
    *   How to read latency/error data from `RoutingState`.
    *   How to register your custom strategy with the `RouterEngine`.

---

## 3. Automatic Fallbacks

Fallbacks ensure high availability by routing to a secondary model when a primary deployment fails (e.g., 5xx errors, timeouts, or content policy violations).

=== "Python"

    ```python
    # Fallback chains are configured on the Deployment object
    # via the Control Plane API.
    # Python clients consume them automatically.
    response = await client.chat(
        key="my-key", model="gpt-4o", messages=[{"role": "user", "content": "Hello!"}]
    )
    # If gpt-4o fails (rate limit, content policy, etc.),
    # the router transparently retries on the configured fallback.
    ```

=== "JSON"

    ```json
    {
      "fallbacks": {
        "gpt-4o": {
          "content_policy": "gpt-4o-mini",
          "context_window": "claude-3-5-sonnet-20241022",
          "general": "claude-3-haiku-20240307"
        }
      }
    }
    ```

!!! tip "Fallback Categories"

    You can specify fallbacks for specific failure types:

    *   `content_policy`: Triggered when the model refuses the prompt.
    *   `context_window`: Triggered when the prompt exceeds the model's context length.
    *   `general`: Triggered by network errors, 5xx responses, or timeouts.
    
---

## 4. Redis State & Performance Tracking

The routing state (latency, in-flight count, failures) is stored in **Redis** using atomic Lua scripts for consistency. This allows the router to make data-driven decisions across multiple instances of the data plane.

*   **Atomic Counters**: In-flight requests are incremented/decremented via Lua scripts, preventing race conditions.
*   **Latency Buffers**: Latency is aggregated using an EWMA algorithm to react quickly to changes without being jittery.
*   **Distributed Coordination**: All HyperInfer data plane nodes share the same view of the fleet's health.

---

## 5. Comparison Table: Routing Concepts

| Concept | Python (Client View) | Rust (Control Plane View) |
| :--- | :--- | :--- |
| **Model Selection** | `model` parameter in `client.chat` | `Deployment` configuration |
| **Failover** | Automatic, transparent | Configured via `fallbacks` JSON |
| **Strategy Choice** | Inherited from deployment | `strategy` field in config |
| **State Monitoring** | Telemetry events | Redis Lua scripts |
| **Custom Logic** | Not supported (Core only) | See [Custom Router Policies](custom-router-policies.md) |
