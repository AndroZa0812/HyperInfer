# Routing Guide

HyperInfer's routing engine supports multiple strategies for intelligent request distribution.

## Strategies

### Weighted Shuffle (default)

Distributes requests across deployments based on weights, adjusted by remaining capacity.

### Latency-Based

Selects the deployment with the lowest EWMA (Exponentially Weighted Moving Average) latency.

```json
{
  "strategy": "latency_based",
  "latency_buffer": 0.1,
  "latency_ttl_secs": 300
}
```

### Least-Busy

Selects the deployment with the fewest in-flight requests.

### Usage-Based

Selects the deployment with the lowest token usage relative to its limit.

### Cost-Based

Selects the cheapest deployment based on estimated input/output token costs.

## Fallbacks

Configure fallback models for when a primary deployment fails:

```json
{
  "fallbacks": {
    "gpt-4o": {
      "content_policy": "gpt-4o-mini",
      "context_window": "claude-sonnet-4-20250514",
      "general": "claude-haiku-3-5"
    }
  }
}
```

## Redis State

The routing state (latency, in-flight count, failures) is stored in Redis using atomic Lua scripts for consistency.
