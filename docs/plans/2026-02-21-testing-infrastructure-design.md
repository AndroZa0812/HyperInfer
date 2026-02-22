# Testing Infrastructure Design for hyperinfer-server

**Date:** 2026-02-21
**Status:** Approved
**Author:** Design session

## Overview

Implement a tiered testing infrastructure for hyperinfer-server with:

- Trait-based abstractions for `Database` and `ConfigStore`
- Mock implementations for fast unit tests
- Testcontainers-based integration tests against real PostgreSQL and Redis
- GitHub Actions workflow with conditional test execution

## Goals

1. **Regression testing** - Catch real bugs with integration tests
2. **Fast feedback** - Unit tests run on every commit (< 30s)
3. **CI efficiency** - Skip expensive integration tests when unnecessary
4. **Maintainability** - Traits enable clean dependency injection

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        hyperinfer-server                         │
├─────────────────────────────────────────────────────────────────┤
│  Handlers (main.rs)                                              │
│       │                                                          │
│       ▼                                                          │
│  ┌─────────────┐    ┌──────────────┐                            │
│  │  Database   │    │ ConfigStore  │  ← Traits                  │
│  │   (trait)   │    │   (trait)    │                            │
│  └─────────────┘    └──────────────┘                            │
│       │                   │                                      │
│       ▼                   ▼                                      │
│  ┌──────────┐       ┌───────────┐                                │
│  │  SqlxDb  │       │ RedisCfg  │  ← Real implementations       │
│  │ (sqlx)   │       │ (redis)   │                                │
│  └──────────┘       └───────────┘                                │
│       │                   │                                      │
│       ▼                   ▼                                      │
│  ┌──────────┐       ┌───────────┐                                │
│  │PostgreSQL│       │  Redis    │                                │
│  └──────────┘       └───────────┘                                │
└─────────────────────────────────────────────────────────────────┘

Test Environment:
┌─────────────────────────────────────────────────────────────────┐
│  Unit Tests              │  Integration Tests                    │
│  ──────────              │  ──────────────────                   │
│  MockDatabase            │  Testcontainers (PostgreSQL + Redis)  │
│  MockConfigStore         │  Real SqlxDb + RedisCfg               │
└─────────────────────────────────────────────────────────────────┘
```

## Trait Definitions

### Database Trait

Location: `crates/hyperinfer-core/src/traits/database.rs`

```rust
use async_trait::async_trait;
use crate::types::{Team, User, ApiKey, ModelAlias, Quota};
use crate::error::DbError;

#[async_trait::async_trait]
pub trait Database: Clone + Send + Sync + 'static {
    async fn get_team(&self, id: &str) -> Result<Option<Team>, DbError>;
    async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, DbError>;
    async fn get_user(&self, id: &str) -> Result<Option<User>, DbError>;
    async fn create_user(&self, team_id: &str, email: &str, role: &str) -> Result<User, DbError>;
    async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError>;
    async fn create_api_key(&self, key_hash: &str, user_id: &str, team_id: &str, name: Option<&str>) -> Result<ApiKey, DbError>;
    async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError>;
    async fn create_model_alias(&self, team_id: &str, alias: &str, target_model: &str, provider: &str) -> Result<ModelAlias, DbError>;
    async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError>;
    async fn create_quota(&self, team_id: &str, rpm_limit: i32, tpm_limit: i32) -> Result<Quota, DbError>;
}
```

### ConfigStore Trait

Location: `crates/hyperinfer-core/src/traits/config_store.rs`

```rust
use async_trait::async_trait;
use crate::types::Config;
use crate::redis::{PolicyUpdate, ConfigError};

#[async_trait::async_trait]
pub trait ConfigStore: Clone + Send + Sync + 'static {
    async fn fetch_config(&self) -> Result<Config, ConfigError>;
    async fn publish_config_update(&self, config: &Config) -> Result<(), ConfigError>;
    async fn publish_policy_update(&self, update: &PolicyUpdate) -> Result<(), ConfigError>;
}
```

## Error Types

Location: `crates/hyperinfer-core/src/error.rs`

```rust
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Invalid UUID: {0}")]
    InvalidUuid(String),
    #[error("Not found")]
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

## Implementation Changes

### SqlxDb Implementation

Location: `crates/hyperinfer-server/src/db.rs` (renamed implementation)

- Rename `Db` to `SqlxDb`
- Implement `Database` trait
- Return `DbError` instead of `sqlx::Error`

### RedisConfigStore Implementation

Location: `crates/hyperinfer-core/src/redis.rs`

- Create `RedisConfigStore` wrapper
- Implement `ConfigStore` trait
- Return `ConfigError` instead of `Box<dyn Error>`

### AppState Update

Location: `crates/hyperinfer-server/src/main.rs`

```rust
#[derive(Clone)]
struct AppState<D: Database, C: ConfigStore> {
    config: Arc<RwLock<Config>>,
    db: D,
    config_manager: C,
}

// Type alias for production
type ProdState = AppState<SqlxDb, RedisConfigStore>;
```

## Test Structure

### Unit Tests

Location: `crates/hyperinfer-server/src/handlers.rs` (tests module)

- Use `mockall::mock!` to generate mock implementations
- Test each handler in isolation
- Verify correct HTTP status codes and responses

### Integration Tests

Location: `crates/hyperinfer-server/tests/integration/main.rs`

- Use `testcontainers-modules` for PostgreSQL and Redis
- Run migrations on container startup
- Test full request/response cycle via `axum::test_helpers::TestClient`

## Dependencies

Add to `crates/hyperinfer-core/Cargo.toml`:

```toml
[dependencies]
async-trait = "0.1"
thiserror = "1.0"

[dev-dependencies]
mockall = "0.13"
```

Add to `crates/hyperinfer-server/Cargo.toml`:

```toml
[dev-dependencies]
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres", "redis"] }
tokio = { version = "1.0", features = ["macros", "rt-multi-thread"] }
tower = "0.5"
http-body-util = "0.1"
```

## GitHub Actions Workflow

Location: `.github/workflows/test.yml`

### Test Tiers

| Tier | Tests              | Trigger                    | Speed  |
| ---- | ------------------ | -------------------------- | ------ |
| Fast | Unit tests (mocks) | Every commit/PR            | <30s   |
| Full | Integration tests  | PRs to main, changed files | 2-5min |

### Integration Test Triggers

Run integration tests when:

1. Push to `main` branch
2. PR has `needs-integration-tests` label
3. Changed files include `db.rs`, `redis.rs`, `main.rs`, or `migrations/`
4. Manual dispatch with `run-integration=true`

## Migration Path

1. Add trait definitions and error types to `hyperinfer-core`
2. Refactor `Db` to `SqlxDb` implementing `Database` trait
3. Create `RedisConfigStore` implementing `ConfigStore` trait
4. Update `AppState` to use generics
5. Add mockall for unit tests
6. Add testcontainers for integration tests
7. Create GitHub Actions workflow
8. Add PR label automation (optional)

## Risks and Mitigations

| Risk                                | Mitigation                                         |
| ----------------------------------- | -------------------------------------------------- |
| Traits add complexity               | Keep traits minimal, only methods used by handlers |
| Generic AppState complicates code   | Use type alias `ProdState` for production          |
| Testcontainers slow in CI           | Only run when necessary, cache Docker images       |
| Mock drift from real implementation | Run integration tests regularly, code review mocks |

## Success Criteria

1. Unit tests run in < 30 seconds
2. Integration tests run in < 5 minutes
3. CI skips integration tests for type-only changes
4. All existing functionality preserved
5. Clear documentation for adding new tests
