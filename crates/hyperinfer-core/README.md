# hyperinfer-core

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-core?style=flat-square)](https://crates.io/crates/hyperinfer-core)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-core?style=flat-square)](https://docs.rs/hyperinfer-core)

Shared types, traits, and error handling for the HyperInfer LLM Gateway. This is the foundational crate used by all other HyperInfer components.

## Key Types

| Type | Description |
|------|-------------|
| `ChatRequest` | Unified LLM chat request with model, messages, parameters |
| `ChatResponse` | LLM chat response with choices and usage |
| `ChatMessage` | Single message with role and content |
| `MessageRole` | User, assistant, system, or tool role enum |
| `Usage` | Token usage tracking (prompt, completion, total) |
| `Deployment` | Model deployment configuration with provider, model, and routing metadata |
| `Config` | Client configuration for API endpoints and team settings |

## Key Traits

| Trait | Description |
|-------|-------------|
| `Database` | Async persistence trait for teams, users, API keys, deployments (27 methods) |
| `ConfigStore` | Async config storage and Pub/Sub trait via Redis |
| `RateLimiter` | Distributed rate limiting using GCRA algorithm |

## Error Types

- `HyperInferError` — main error enum covering network, auth, rate-limit, serialization, etc.
- `DbError` — database-specific errors (not found, unique violation, connection)
- `ConfigError` — configuration parsing and validation errors

## Cargo Features

- `test-mocks` — enables `mockall` for mock implementations of traits in tests

## Usage

```toml
[dependencies]
hyperinfer-core = "0.1"
```

## License

MIT
