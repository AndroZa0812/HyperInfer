# hyperinfer-router

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-router?style=flat-square)](https://crates.io/crates/hyperinfer-router)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-router?style=flat-square)](https://docs.rs/hyperinfer-router)

Intelligent request routing engine for HyperInfer with pluggable strategies, deployment pool management, fallback chains, and Redis-backed routing state.

## Routing Strategies

| Strategy | Description |
|----------|-------------|
| **WeightedShuffle** (default) | Weighted random selection adjusted by RPM/TPM capacity |
| **LatencyBased** | Selects deployment with lowest EWMA latency |
| **LeastBusy** | Selects deployment with fewest in-flight requests |
| **UsageBased** | Selects deployment with lowest token usage relative to limits |
| **CostBased** | Selects cheapest deployment based on estimated token costs |

## How It Works

```
Incoming Request
      │
      ▼
Model Alias Resolution (e.g., "fast" → "gpt-4o-mini")
      │
      ▼
Routing Strategy Selection (configurable per deployment group)
      │
      ▼
Deployment Selection (from pool, respecting weights and limits)
      │
      ▼
Fallback Chain (if primary fails → try fallback models)
      │
      ▼
Provider Call (hyperinfer-providers handles the actual API call)
```

## Fallback Configuration

Fallbacks can be configured per error kind:

- `content_policy` — fallback when content filtered
- `context_window` — fallback on context length exceeded
- `general` — fallback on any other error

## License

MIT
