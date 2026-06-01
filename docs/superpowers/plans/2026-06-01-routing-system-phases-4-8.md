# Routing System Phases 4-8 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the `hyperinfer-router` crate (Phases 1-3) into the server, client, Python bindings, and dashboard to complete the routing system.

**Architecture:** The server persists deployments in PostgreSQL and exposes CRUD + OpenAI-compatible proxy endpoints. The client embeds `RouterEngine` with `RedisRoutingState` for local routing decisions. Python bindings expose custom strategy authoring. The dashboard shows deployment health.

**Tech Stack:** Rust (axum, sqlx, PyO3), PostgreSQL, Redis, SvelteKit

**Working directory:** All commands run from the worktree root at `.worktrees/routing-system/`

---

## File Structure

### Phase 4: Server Deployment CRUD
| Action | File | Responsibility |
|--------|------|----------------|
| Create | `crates/hyperinfer-server/migrations/005_deployments.sql` | Deployments table schema |
| Modify | `crates/hyperinfer-core/src/traits/database.rs` | Add deployment CRUD methods to `Database` trait + `Deployment` model struct |
| Modify | `crates/hyperinfer-core/src/lib.rs` | Re-export new `DeploymentRecord` type |
| Modify | `crates/hyperinfer-server/src/db.rs` | Implement deployment CRUD on `SqlxDb` + `DeploymentRow` |
| Modify | `crates/hyperinfer-server/src/main.rs` | Add deployment REST endpoints + request types |

### Phase 5: Server OpenAI Proxy
| Action | File | Responsibility |
|--------|------|----------------|
| Create | `crates/hyperinfer-server/src/proxy.rs` | OpenAI-compatible `/v1/chat/completions` proxy handler |
| Modify | `crates/hyperinfer-server/Cargo.toml` | Add `hyperinfer-router` + `hyperinfer-providers` deps |
| Modify | `crates/hyperinfer-server/src/main.rs` | Wire proxy routes + RouterEngine init |
| Modify | `crates/hyperinfer-server/src/lib.rs` | Export proxy module |

### Phase 6: Client Integration
| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `crates/hyperinfer-client/Cargo.toml` | Add `hyperinfer-router` dep |
| Modify | `crates/hyperinfer-client/src/router.rs` | Replace simple Router with RouterEngine wrapper |
| Modify | `crates/hyperinfer-client/src/lib.rs` | Wire RouterEngine into `chat()` and `chat_stream()` |

### Phase 7: Python Bindings
| Action | File | Responsibility |
|--------|------|----------------|
| Create | `crates/hyperinfer-python/src/routing.rs` | PyO3 wrappers for RouterEngine, Deployment, strategies |
| Modify | `crates/hyperinfer-python/Cargo.toml` | Add `hyperinfer-router` dep |
| Modify | `crates/hyperinfer-python/src/lib.rs` | Register routing classes in module |

### Phase 8: Dashboard Routing View
| Action | File | Responsibility |
|--------|------|----------------|
| Create | `apps/dashboard/src/lib/types.ts` (modify) | Add Deployment, DeploymentMetrics types |
| Create | `apps/dashboard/src/lib/api.ts` (modify) | Add deployment API methods |
| Create | `apps/dashboard/src/routes/dashboard/routing/+page.svelte` | Routing health page |
| Modify | `apps/dashboard/src/lib/components/Sidebar.svelte` | Add routing nav item |

---

## Phase 4: Server Deployment CRUD

### Task 1: Database Migration for Deployments Table

**Files:**
- Create: `crates/hyperinfer-server/migrations/005_deployments.sql`

- [ ] **Step 1: Create the migration file**

```sql
-- Deployments table for routing engine

CREATE TABLE deployments (
    id VARCHAR(16) PRIMARY KEY,
    model_name VARCHAR(255) NOT NULL,
    provider VARCHAR(50) NOT NULL,
    model VARCHAR(255) NOT NULL,
    api_key_ref TEXT NOT NULL,
    base_url TEXT,
    weight INTEGER NOT NULL DEFAULT 1,
    rpm_limit BIGINT,
    tpm_limit BIGINT,
    input_cost_per_1k DOUBLE PRECISION,
    output_cost_per_1k DOUBLE PRECISION,
    sort_order INTEGER NOT NULL DEFAULT 0,
    tags JSONB NOT NULL DEFAULT '{}',
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_deployments_model_name ON deployments(model_name);
CREATE INDEX idx_deployments_active ON deployments(is_active) WHERE is_active = true;

CREATE TRIGGER update_deployments_updated_at
    BEFORE UPDATE ON deployments
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
```

- [ ] **Step 2: Verify migration runs**

Run: `cd crates/hyperinfer-server && sqlx migrate run` (or start the server which auto-runs migrations)
Expected: Migration applies without error.

- [ ] **Step 3: Commit**

```bash
git add crates/hyperinfer-server/migrations/005_deployments.sql
git commit -m "feat(server): add deployments table migration"
```

---

### Task 2: Add DeploymentRecord to Core Trait

**Files:**
- Modify: `crates/hyperinfer-core/src/traits/database.rs`
- Modify: `crates/hyperinfer-core/src/lib.rs`

- [ ] **Step 1: Add DeploymentRecord struct and Database trait methods**

Add to `crates/hyperinfer-core/src/traits/database.rs` after the `UsageLog` struct (end of file):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub id: String,
    pub model_name: String,
    pub provider: String,
    pub model: String,
    pub api_key_ref: String,
    pub base_url: Option<String>,
    pub weight: i32,
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
    pub sort_order: i32,
    pub tags: serde_json::Value,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

Add these methods to the `Database` trait (before the closing `}`):

```rust
    async fn list_deployments(&self) -> Result<Vec<DeploymentRecord>, DbError>;
    async fn get_deployment(&self, id: &str) -> Result<Option<DeploymentRecord>, DbError>;
    async fn create_deployment(&self, record: &DeploymentRecord) -> Result<DeploymentRecord, DbError>;
    async fn update_deployment(&self, record: &DeploymentRecord) -> Result<DeploymentRecord, DbError>;
    async fn delete_deployment(&self, id: &str) -> Result<(), DbError>;
```

- [ ] **Step 2: Re-export DeploymentRecord from core**

In `crates/hyperinfer-core/src/lib.rs`, update the traits re-export line:

```rust
pub use traits::{ApiKey, ConfigStore, Database, DeploymentRecord, ModelAlias, Quota, Team, UsageLog, User};
```

- [ ] **Step 3: Verify core compiles**

Run: `cargo check -p hyperinfer-core`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/hyperinfer-core/
git commit -m "feat(core): add DeploymentRecord and deployment CRUD to Database trait"
```

---

### Task 3: Implement Deployment CRUD in SqlxDb

**Files:**
- Modify: `crates/hyperinfer-server/src/db.rs`

- [ ] **Step 1: Add DeploymentRow struct**

Add to `crates/hyperinfer-server/src/db.rs` after the `UsageLogRow` struct and its `From` impl (around line 438):

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct DeploymentRow {
    id: String,
    model_name: String,
    provider: String,
    model: String,
    api_key_ref: String,
    base_url: Option<String>,
    weight: i32,
    rpm_limit: Option<i64>,
    tpm_limit: Option<i64>,
    input_cost_per_1k: Option<f64>,
    output_cost_per_1k: Option<f64>,
    sort_order: i32,
    tags: serde_json::Value,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DeploymentRow> for DeploymentRecord {
    fn from(row: DeploymentRow) -> Self {
        DeploymentRecord {
            id: row.id,
            model_name: row.model_name,
            provider: row.provider,
            model: row.model,
            api_key_ref: row.api_key_ref,
            base_url: row.base_url,
            weight: row.weight,
            rpm_limit: row.rpm_limit,
            tpm_limit: row.tpm_limit,
            input_cost_per_1k: row.input_cost_per_1k,
            output_cost_per_1k: row.output_cost_per_1k,
            sort_order: row.sort_order,
            tags: row.tags,
            is_active: row.is_active,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
```

- [ ] **Step 2: Add `use` import for DeploymentRecord**

At the top of `db.rs`, update the `hyperinfer_core` import:

```rust
use hyperinfer_core::{
    ApiKey, ConfigStore, Database, DbError, DeploymentRecord, ModelAlias, PolicyUpdate, Quota, Team, UsageLog, User,
};
```

- [ ] **Step 3: Implement deployment CRUD methods on SqlxDb**

Add inside the `impl Database for SqlxDb` block (before the closing `}`):

```rust
    async fn list_deployments(&self) -> Result<Vec<DeploymentRecord>, DbError> {
        let rows: Vec<DeploymentRow> = sqlx::query_as(
            "SELECT id, model_name, provider, model, api_key_ref, base_url, weight, rpm_limit, tpm_limit, input_cost_per_1k, output_cost_per_1k, sort_order, tags, is_active, created_at, updated_at FROM deployments WHERE is_active = true ORDER BY sort_order, id"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(DeploymentRecord::from).collect())
    }

    async fn get_deployment(&self, id: &str) -> Result<Option<DeploymentRecord>, DbError> {
        let result: Option<DeploymentRow> = sqlx::query_as(
            "SELECT id, model_name, provider, model, api_key_ref, base_url, weight, rpm_limit, tpm_limit, input_cost_per_1k, output_cost_per_1k, sort_order, tags, is_active, created_at, updated_at FROM deployments WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(result.map(DeploymentRecord::from))
    }

    async fn create_deployment(&self, record: &DeploymentRecord) -> Result<DeploymentRecord, DbError> {
        let row: DeploymentRow = sqlx::query_as(
            "INSERT INTO deployments (id, model_name, provider, model, api_key_ref, base_url, weight, rpm_limit, tpm_limit, input_cost_per_1k, output_cost_per_1k, sort_order, tags, is_active) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING id, model_name, provider, model, api_key_ref, base_url, weight, rpm_limit, tpm_limit, input_cost_per_1k, output_cost_per_1k, sort_order, tags, is_active, created_at, updated_at"
        )
        .bind(&record.id)
        .bind(&record.model_name)
        .bind(&record.provider)
        .bind(&record.model)
        .bind(&record.api_key_ref)
        .bind(&record.base_url)
        .bind(record.weight)
        .bind(record.rpm_limit)
        .bind(record.tpm_limit)
        .bind(record.input_cost_per_1k)
        .bind(record.output_cost_per_1k)
        .bind(record.sort_order)
        .bind(&record.tags)
        .bind(record.is_active)
        .fetch_one(&self.pool)
        .await?;
        Ok(DeploymentRecord::from(row))
    }

    async fn update_deployment(&self, record: &DeploymentRecord) -> Result<DeploymentRecord, DbError> {
        let row: DeploymentRow = sqlx::query_as(
            "UPDATE deployments SET model_name = $2, provider = $3, model = $4, api_key_ref = $5, base_url = $6, weight = $7, rpm_limit = $8, tpm_limit = $9, input_cost_per_1k = $10, output_cost_per_1k = $11, sort_order = $12, tags = $13, is_active = $14 WHERE id = $1 RETURNING id, model_name, provider, model, api_key_ref, base_url, weight, rpm_limit, tpm_limit, input_cost_per_1k, output_cost_per_1k, sort_order, tags, is_active, created_at, updated_at"
        )
        .bind(&record.id)
        .bind(&record.model_name)
        .bind(&record.provider)
        .bind(&record.model)
        .bind(&record.api_key_ref)
        .bind(&record.base_url)
        .bind(record.weight)
        .bind(record.rpm_limit)
        .bind(record.tpm_limit)
        .bind(record.input_cost_per_1k)
        .bind(record.output_cost_per_1k)
        .bind(record.sort_order)
        .bind(&record.tags)
        .bind(record.is_active)
        .fetch_one(&self.pool)
        .await?;
        Ok(DeploymentRecord::from(row))
    }

    async fn delete_deployment(&self, id: &str) -> Result<(), DbError> {
        let result = sqlx::query("UPDATE deployments SET is_active = false WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }
```

- [ ] **Step 4: Verify server compiles**

Run: `cargo check -p hyperinfer-server`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/hyperinfer-server/src/db.rs
git commit -m "feat(server): implement deployment CRUD in SqlxDb"
```

---

### Task 4: Add Deployment REST Endpoints

**Files:**
- Modify: `crates/hyperinfer-server/src/main.rs`

- [ ] **Step 1: Add request/response types**

Add after the existing `CreateQuotaRequest` struct (around line 306):

```rust
#[derive(Deserialize)]
struct CreateDeploymentRequest {
    model_name: String,
    provider: String,
    model: String,
    api_key_ref: String,
    base_url: Option<String>,
    weight: Option<i32>,
    rpm_limit: Option<i64>,
    tpm_limit: Option<i64>,
    input_cost_per_1k: Option<f64>,
    output_cost_per_1k: Option<f64>,
    sort_order: Option<i32>,
    tags: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct UpdateDeploymentRequest {
    model_name: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    api_key_ref: Option<String>,
    base_url: Option<String>,
    weight: Option<i32>,
    rpm_limit: Option<i64>,
    tpm_limit: Option<i64>,
    input_cost_per_1k: Option<f64>,
    output_cost_per_1k: Option<f64>,
    sort_order: Option<i32>,
    tags: Option<serde_json::Value>,
}
```

- [ ] **Step 2: Add handler functions**

Add after the `create_quota` handler function:

```rust
async fn list_deployments<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    match state.db.list_deployments().await {
        Ok(deployments) => Json(deployments).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn get_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_deployment(&id).await {
        Ok(Some(d)) => Json(d).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Deployment not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn create_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateDeploymentRequest>,
) -> impl IntoResponse {
    use hyperinfer_core::DeploymentRecord;
    use chrono::Utc;

    let weight = req.weight.unwrap_or(1);
    if weight < 0 {
        return (StatusCode::BAD_REQUEST, "weight must be non-negative").into_response();
    }
    let sort_order = req.sort_order.unwrap_or(0);
    if sort_order < 0 {
        return (StatusCode::BAD_REQUEST, "sort_order must be non-negative").into_response();
    }

    let id = hyperinfer_router::Deployment::generate_id(
        &req.provider.parse().unwrap_or(hyperinfer_core::Provider::Other),
        &req.model,
        &req.base_url,
        &req.api_key_ref,
    );

    let record = DeploymentRecord {
        id,
        model_name: req.model_name,
        provider: req.provider,
        model: req.model,
        api_key_ref: req.api_key_ref,
        base_url: req.base_url,
        weight,
        rpm_limit: req.rpm_limit,
        tpm_limit: req.tpm_limit,
        input_cost_per_1k: req.input_cost_per_1k,
        output_cost_per_1k: req.output_cost_per_1k,
        sort_order,
        tags: req.tags.unwrap_or(serde_json::json!({})),
        is_active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    match state.db.create_deployment(&record).await {
        Ok(d) => (StatusCode::CREATED, Json(d)).into_response(),
        Err(e) => match e {
            DbError::UniqueViolation(msg) => (StatusCode::CONFLICT, msg).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create deployment").into_response(),
        },
    }
}

async fn update_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDeploymentRequest>,
) -> impl IntoResponse {
    let existing = match state.db.get_deployment(&id).await {
        Ok(Some(d)) => d,
        Ok(None) => return (StatusCode::NOT_FOUND, "Deployment not found").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };

    let updated = hyperinfer_core::DeploymentRecord {
        model_name: req.model_name.unwrap_or(existing.model_name),
        provider: req.provider.unwrap_or(existing.provider),
        model: req.model.unwrap_or(existing.model),
        api_key_ref: req.api_key_ref.unwrap_or(existing.api_key_ref),
        base_url: req.base_url.or(existing.base_url),
        weight: req.weight.unwrap_or(existing.weight),
        rpm_limit: req.rpm_limit.or(existing.rpm_limit),
        tpm_limit: req.tpm_limit.or(existing.tpm_limit),
        input_cost_per_1k: req.input_cost_per_1k.or(existing.input_cost_per_1k),
        output_cost_per_1k: req.output_cost_per_1k.or(existing.output_cost_per_1k),
        sort_order: req.sort_order.unwrap_or(existing.sort_order),
        tags: req.tags.unwrap_or(existing.tags),
        ..existing
    };

    match state.db.update_deployment(&updated).await {
        Ok(d) => Json(d).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to update deployment").into_response(),
    }
}

async fn delete_deployment<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_deployment(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(DbError::NotFound) => (StatusCode::NOT_FOUND, "Deployment not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete deployment").into_response(),
    }
}
```

- [ ] **Step 3: Register routes in the v1_router**

In `main()`, add deployment routes to the `v1_router` builder (after the quotas routes, before `.layer`):

```rust
        .route("/v1/deployments", get(list_deployments))
        .route("/v1/deployments", post(create_deployment))
        .route("/v1/deployments/{id}", get(get_deployment))
        .route("/v1/deployments/{id}", axum::routing::put(update_deployment))
        .route("/v1/deployments/{id}", axum::routing::delete(delete_deployment))
```

- [ ] **Step 4: Add hyperinfer-router dependency to server**

In `crates/hyperinfer-server/Cargo.toml`, add to `[dependencies]`:

```toml
hyperinfer-router = { path = "../hyperinfer-router" }
```

- [ ] **Step 5: Add `use` for `put` and `delete` routing**

At the top of `main.rs`, update the axum routing import:

```rust
use axum::{
    body::Body,
    extract::{Extension, Json, Path, State},
    http::{header::SET_COOKIE, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
```

And update the route registrations to use the imported `put` and `delete`:

```rust
        .route("/v1/deployments/{id}", put(update_deployment))
        .route("/v1/deployments/{id}", delete(delete_deployment))
```

- [ ] **Step 6: Verify server compiles**

Run: `cargo check -p hyperinfer-server`
Expected: Compiles without errors.

- [ ] **Step 7: Commit**

```bash
git add crates/hyperinfer-server/
git commit -m "feat(server): add deployment CRUD REST endpoints"
```

---

### Task 5: Add Deployment CRUD Tests

**Files:**
- Modify: `crates/hyperinfer-server/src/main.rs` (test module)

- [ ] **Step 1: Add mock expectations for deployment methods**

In the `mock!` block for `MockDatabase`, add these method signatures:

```rust
            async fn list_deployments(&self) -> Result<Vec<DeploymentRecord>, DbError>;
            async fn get_deployment(&self, id: &str) -> Result<Option<DeploymentRecord>, DbError>;
            async fn create_deployment(&self, record: &DeploymentRecord) -> Result<DeploymentRecord, DbError>;
            async fn update_deployment(&self, record: &DeploymentRecord) -> Result<DeploymentRecord, DbError>;
            async fn delete_deployment(&self, id: &str) -> Result<(), DbError>;
```

Add `DeploymentRecord` to the test imports:

```rust
    use hyperinfer_core::{
        ApiKey, ConfigError, DbError, DeploymentRecord, ModelAlias, PolicyUpdate, Quota, Team, UsageLog, User,
    };
```

- [ ] **Step 2: Add test for list_deployments**

```rust
    #[tokio::test]
    async fn test_list_deployments_empty() {
        let mut db = MockDatabase::new();
        db.expect_list_deployments()
            .times(1)
            .returning(|| Ok(vec![]));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let response = list_deployments(State(state)).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }
```

- [ ] **Step 3: Add test for get_deployment not found**

```rust
    #[tokio::test]
    async fn test_get_deployment_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_deployment()
            .with(eq("nonexistent"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let response = get_deployment(State(state), Path("nonexistent".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
```

- [ ] **Step 4: Add test for delete_deployment**

```rust
    #[tokio::test]
    async fn test_delete_deployment_success() {
        let mut db = MockDatabase::new();
        db.expect_delete_deployment()
            .with(eq("abc123"))
            .times(1)
            .returning(|_| Ok(()));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
            admin_token: Arc::new("test-token".to_string()),
            jwt_secret: Arc::new("test-jwt-secret".to_string()),
        };

        let response = delete_deployment(State(state), Path("abc123".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p hyperinfer-server`
Expected: All tests pass including new deployment tests.

- [ ] **Step 6: Commit**

```bash
git add crates/hyperinfer-server/src/main.rs
git commit -m "test(server): add deployment CRUD handler tests"
```

---

## Phase 5: Server OpenAI Proxy

### Task 6: Create Proxy Module

**Files:**
- Create: `crates/hyperinfer-server/src/proxy.rs`
- Modify: `crates/hyperinfer-server/src/lib.rs`

- [ ] **Step 1: Add hyperinfer-providers dependency**

In `crates/hyperinfer-server/Cargo.toml`, add to `[dependencies]`:

```toml
hyperinfer-providers = { path = "../hyperinfer-providers", features = ["openai", "anthropic"] }
```

- [ ] **Step 2: Create proxy.rs with handler**

Create `crates/hyperinfer-server/src/proxy.rs`:

```rust
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use hyperinfer_core::{ChatRequest, ChatResponse, HyperInferError, Provider};
use hyperinfer_router::{
    Deployment, GlobalLimits, RedisConfig, RedisRoutingState, RouterEngine, RoutingContext,
    RoutingStrategy, CostBased, LatencyBased, LeastBusy, UsageBased, WeightedShuffle,
};
use hyperinfer_providers::ProviderRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn};

pub struct ProxyState {
    pub engine: RouterEngine,
    pub routing_state: RedisRoutingState,
    pub provider_registry: Arc<ProviderRegistry>,
    pub api_keys: Arc<RwLock<std::collections::HashMap<String, String>>>,
}

impl ProxyState {
    pub async fn new(
        redis_url: &str,
        api_keys: Arc<RwLock<std::collections::HashMap<String, String>>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let engine = RouterEngine::new(GlobalLimits::default());

        engine.register_strategy(Box::new(WeightedShuffle::new())).await;
        engine.register_strategy(Box::new(LatencyBased::new())).await;
        engine.register_strategy(Box::new(LeastBusy::new())).await;
        engine.register_strategy(Box::new(UsageBased::new())).await;
        engine.register_strategy(Box::new(CostBased::new())).await;

        let routing_state = RedisRoutingState::new(redis_url, RedisConfig::default()).await?;

        let provider_registry = Arc::new(ProviderRegistry::new());
        hyperinfer_providers::init_default_registry(&provider_registry);

        Ok(Self {
            engine,
            routing_state,
            provider_registry,
            api_keys,
        })
    }

    pub async fn load_deployments(
        &self,
        deployments: Vec<hyperinfer_core::DeploymentRecord>,
    ) {
        let router_deployments: Vec<Deployment> = deployments
            .into_iter()
            .filter(|d| d.is_active)
            .map(|d| {
                let provider = match d.provider.as_str() {
                    "openai" => Provider::OpenAI,
                    "anthropic" => Provider::Anthropic,
                    _ => Provider::Other,
                };
                let mut dep = Deployment::new(
                    d.model_name,
                    provider,
                    d.model,
                    d.api_key_ref,
                );
                dep.id = d.id;
                dep.weight = d.weight as u32;
                dep.rpm_limit = d.rpm_limit.map(|v| v as u64);
                dep.tpm_limit = d.tpm_limit.map(|v| v as u64);
                dep.input_cost_per_1k = d.input_cost_per_1k;
                dep.output_cost_per_1k = d.output_cost_per_1k;
                dep.order = d.sort_order as u32;
                if let Some(url) = d.base_url {
                    dep.base_url = Some(url);
                }
                dep
            })
            .collect();

        self.engine.rebuild_pool(router_deployments).await;
    }
}

fn error_to_response(err: HyperInferError) -> impl IntoResponse {
    match err {
        HyperInferError::ApiError { status, message } => {
            let status_code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            (status_code, message).into_response()
        }
        HyperInferError::RateLimit(msg) => {
            (StatusCode::TOO_MANY_REQUESTS, msg).into_response()
        }
        HyperInferError::Config(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
        HyperInferError::Http(e) => {
            (StatusCode::BAD_GATEWAY, e.to_string()).into_response()
        }
    }
}

pub async fn chat_completions_handler(
    State(state): State<Arc<ProxyState>>,
    Json(request): Json<ChatRequest>,
) -> impl IntoResponse {
    if let Err(e) = request.validate() {
        return error_to_response(e).into_response();
    }

    let ctx = RoutingContext::default();
    let state_clone = Arc::clone(&state);
    let request_clone = request.clone();

    let result = state
        .engine
        .route_with_fallback(
            &request.model,
            &state.routing_state,
            &ctx,
            move |deployment| {
                let state = Arc::clone(&state_clone);
                let req = request_clone.clone();
                Box::pin(async move {
                    let provider_name = deployment.provider.to_string();
                    let llm_provider = state
                        .provider_registry
                        .get(&provider_name)
                        .ok_or_else(|| HyperInferError::Config(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Provider '{}' not found", provider_name),
                        )))?;

                    let api_key = {
                        let keys = state.api_keys.read().await;
                        keys.get(&deployment.api_key_ref)
                            .cloned()
                            .unwrap_or_else(|| deployment.api_key_ref.clone())
                    };

                    let mut resolved_request = req;
                    resolved_request.model = deployment.model.clone();
                    llm_provider.chat(&resolved_request, &api_key).await
                })
            },
        )
        .await;

    match result {
        Ok((routing_result, response)) => {
            let tokens = (response.usage.input_tokens + response.usage.output_tokens) as u64;
            state.engine.record_success(
                &routing_result.deployment.id,
                0.0,
                tokens,
                &state.routing_state,
            ).await;
            Json(response).into_response()
        }
        Err(e) => {
            warn!(error = %e, "routing failed");
            (StatusCode::BAD_GATEWAY, format!("Routing error: {}", e)).into_response()
        }
    }
}

    let ctx = RoutingContext::default();
    let state_clone = Arc::clone(&state);
    let request_clone = request.clone();

    let result = state
        .engine
        .route_with_fallback(
            &request.model,
            &state.routing_state,
            &ctx,
            move |deployment| {
                let state = Arc::clone(&state_clone);
                let req = request_clone.clone();
                Box::pin(async move {
                    let provider_name = deployment.provider.to_string();
                    let llm_provider = state
                        .provider_registry
                        .get(&provider_name)
                        .ok_or_else(|| HyperInferError::Config(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Provider '{}' not found", provider_name),
                        )))?;

                    let api_key = {
                        let keys = state.api_keys.read().await;
                        keys.get(&deployment.api_key_ref)
                            .cloned()
                            .unwrap_or_else(|| deployment.api_key_ref.clone())
                    };

                    let mut resolved_request = req;
                    resolved_request.model = deployment.model.clone();
                    llm_provider.chat(&resolved_request, &api_key).await
                })
            },
        )
        .await;

    match result {
        Ok((routing_result, response)) => {
            let tokens = (response.usage.input_tokens + response.usage.output_tokens) as u64;
            state.engine.record_success(
                &routing_result.deployment.id,
                0.0,
                tokens,
                &state.routing_state,
            ).await;
            Json(response).into_response()
        }
        Err(e) => {
            warn!(error = %e, "routing failed");
            (StatusCode::BAD_GATEWAY, format!("Routing error: {}", e)).into_response()
        }
    }
}
```

- [ ] **Step 3: Export proxy module from lib.rs**

Update `crates/hyperinfer-server/src/lib.rs`:

```rust
pub mod auth;
pub mod db;
pub mod frontend;
pub mod mcp;
pub mod proxy;
pub mod seeding;

pub use db::{RedisConfigStore, SqlxDb};
pub use proxy::ProxyState;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p hyperinfer-server`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/hyperinfer-server/
git commit -m "feat(server): add OpenAI-compatible proxy handler with routing engine"
```

---

### Task 7: Wire Proxy Routes into Server

**Files:**
- Modify: `crates/hyperinfer-server/src/main.rs`

- [ ] **Step 1: Initialize ProxyState in main()**

After the `let mcp_state = McpState::new(jwt_secret);` line, add:

```rust
    let proxy_state = match hyperinfer_server::ProxyState::new(&redis_url, Arc::new(RwLock::new(config.read().await.api_keys.clone()))).await {
        Ok(ps) => Arc::new(ps),
        Err(e) => {
            tracing::warn!("Failed to initialize proxy state: {:?}. Proxy endpoints disabled.", e);
            // Continue without proxy - we'll make it optional
            // For now, return error
            return Err(e);
        }
    };

    // Load deployments from DB into the routing engine
    match state.db.list_deployments().await {
        Ok(deployments) => {
            proxy_state.load_deployments(deployments).await;
            info!("Loaded deployments into routing engine");
        }
        Err(e) => {
            warn!("Failed to load deployments: {:?}", e);
        }
    }
```

- [ ] **Step 2: Add proxy routes**

After the `auth_protected_routes` block and before the final `Router::new()` merge, add:

```rust
    let proxy_router = Router::new()
        .route("/v1/chat/completions", post(hyperinfer_server::proxy::chat_completions_handler))
        .with_state(proxy_state);
```

- [ ] **Step 3: Merge proxy routes into app**

Update the app router merge chain to include `.merge(proxy_router)`:

```rust
    let app = Router::new()
        .merge(v1_router)
        .merge(mcp_router)
        .merge(auth_public_routes)
        .merge(auth_protected_routes)
        .merge(proxy_router)
        .fallback(hyperinfer_server::frontend::spa_handler)
        .layer(cors)
        .with_state(state);
```

- [ ] **Step 4: Verify full build**

Run: `cargo build -p hyperinfer-server`
Expected: Builds successfully.

- [ ] **Step 5: Commit**

```bash
git add crates/hyperinfer-server/
git commit -m "feat(server): wire proxy routes with routing engine into server"
```

---

## Phase 6: Client Integration

### Task 8: Add RouterEngine to Client

**Files:**
- Modify: `crates/hyperinfer-client/Cargo.toml`
- Modify: `crates/hyperinfer-client/src/router.rs`

- [ ] **Step 1: Add hyperinfer-router dependency**

In `crates/hyperinfer-client/Cargo.toml`, add to `[dependencies]`:

```toml
hyperinfer-router = { path = "../hyperinfer-router" }
```

- [ ] **Step 2: Rewrite router.rs to wrap RouterEngine**

Replace the entire contents of `crates/hyperinfer-client/src/router.rs`:

```rust
use hyperinfer_core::{Config, Provider};
use hyperinfer_router::{
    CostBased, Deployment, GlobalLimits, LatencyBased, LeastBusy, RedisConfig,
    RedisRoutingState, RouterEngine, RoutingContext, UsageBased, WeightedShuffle,
};
use std::sync::Arc;
use tracing::warn;

pub struct Router {
    engine: Arc<RouterEngine>,
    routing_state: Option<Arc<RedisRoutingState>>,
}

impl Router {
    pub async fn new(redis_url: &str, config: &Config) -> Self {
        let engine = RouterEngine::new(GlobalLimits::default());

        engine.register_strategy(Box::new(WeightedShuffle::new())).await;
        engine.register_strategy(Box::new(LatencyBased::new())).await;
        engine.register_strategy(Box::new(LeastBusy::new())).await;
        engine.register_strategy(Box::new(UsageBased::new())).await;
        engine.register_strategy(Box::new(CostBased::new())).await;

        for (alias, target) in &config.model_aliases {
            engine.set_alias(alias, target).await;
        }

        let routing_state = match RedisRoutingState::new(redis_url, RedisConfig::default()).await {
            Ok(state) => Some(Arc::new(state)),
            Err(e) => {
                warn!("Failed to connect to Redis for routing state: {}. Routing will use defaults.", e);
                None
            }
        };

        Self {
            engine: Arc::new(engine),
            routing_state,
        }
    }

    pub fn engine(&self) -> &Arc<RouterEngine> {
        &self.engine
    }

    pub fn routing_state(&self) -> Option<&Arc<RedisRoutingState>> {
        self.routing_state.as_ref()
    }

    pub async fn load_deployments(&self, deployments: Vec<Deployment>) {
        self.engine.rebuild_pool(deployments).await;
    }

    pub fn resolve(&self, model: &str, _config: &Config) -> Option<(String, Provider)> {
        if model.starts_with("gpt-") || model.starts_with("o1-") || model.starts_with("o3-") {
            Some((model.to_string(), Provider::OpenAI))
        } else if model.starts_with("claude-") {
            Some((model.to_string(), Provider::Anthropic))
        } else {
            None
        }
    }
}
```

- [ ] **Step 3: Verify client compiles**

Run: `cargo check -p hyperinfer-client`
Expected: Compiles (may have warnings about unused fields in lib.rs - that's OK for now).

- [ ] **Step 4: Commit**

```bash
git add crates/hyperinfer-client/
git commit -m "feat(client): replace simple Router with RouterEngine wrapper"
```

---

### Task 9: Wire RouterEngine into chat() and chat_stream()

**Files:**
- Modify: `crates/hyperinfer-client/src/lib.rs`

- [ ] **Step 1: Update HyperInferClient::new() to use async Router**

Replace the `Router::new(...)` call in `HyperInferClient::new()`:

```rust
        let router = Arc::new(Router::new(redis_url, &config).await);
```

- [ ] **Step 2: Update the `chat()` method to use route_with_fallback**

Replace the entire model resolution and execution block (lines 229-345) inside `chat()` with:

```rust
            let ctx = hyperinfer_router::RoutingContext::default();
            let router = Arc::clone(&self.router);
            let config = self.config.read().await.clone();
            let provider_registry = Arc::clone(&self.provider_registry);
            let telemetry = Arc::clone(&self.telemetry);
            let cache = Arc::clone(&self.cache);
            let request_clone = request.clone();
            let key_owned = key.to_string();

            let routing_state = router.routing_state().cloned();

            let result = if let Some(state) = routing_state {
                router.engine().route_with_fallback(
                    &request.model,
                    state.as_ref(),
                    &ctx,
                    move |deployment| {
                        let config = config.clone();
                        let provider_registry = Arc::clone(&provider_registry);
                        let request = request_clone.clone();
                        Box::pin(async move {
                            let provider_name = deployment.provider.to_string();
                            let llm_provider = provider_registry
                                .get(&provider_name)
                                .ok_or_else(|| HyperInferError::Config(std::io::Error::new(
                                    std::io::ErrorKind::NotFound,
                                    format!("Provider '{}' not found", provider_name),
                                )))?;

                            let api_key = config
                                .api_keys
                                .get(&provider_name)
                                .cloned()
                                .ok_or_else(|| {
                                    HyperInferError::Config(std::io::Error::new(
                                        std::io::ErrorKind::NotFound,
                                        format!("API key not found for provider: {:?}", deployment.provider),
                                    ))
                                })?;

                            let mut resolved_request = request;
                            resolved_request.model = deployment.model.clone();
                            llm_provider.chat(&resolved_request, &api_key).await
                        })
                    },
                ).await
            } else {
                let (model, provider) = router.resolve(&request.model, &config).ok_or_else(|| {
                    HyperInferError::Config(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Unknown model: '{}'. No routing rule or alias found.", request.model),
                    ))
                })?;

                let provider_name = provider.to_string();
                let api_key = config
                    .api_keys
                    .get(&provider_name)
                    .cloned()
                    .ok_or_else(|| {
                        HyperInferError::Config(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("API key not found for provider: {:?}", provider),
                        ))
                    })?;

                let llm_provider = {
                    let registry = provider_registry.read().await;
                    registry.get(&provider_name).ok_or_else(|| {
                        HyperInferError::Config(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Provider '{}' not found in registry", provider_name),
                        ))
                    })?
                };

                let mut resolved_request = request.clone();
                resolved_request.model = model.clone();
                let response = llm_provider.chat(&resolved_request, &api_key).await?;

                let routing_result = hyperinfer_router::RoutingResult {
                    deployment: Arc::new(hyperinfer_router::Deployment::new(
                        request.model.clone(),
                        provider,
                        model,
                        provider_name.clone(),
                    )),
                    attempt: 1,
                    fallback_chain: vec![request.model.clone()],
                };

                Ok((routing_result, response))
            };

            match result {
                Ok((routing_result, response)) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    let input_tokens = response.usage.input_tokens;
                    let output_tokens = response.usage.output_tokens;

                    crate::telemetry_otlp::set_gen_ai_usage(
                        &tracing::Span::current(),
                        input_tokens,
                        output_tokens,
                    );

                    let finish_reason = response
                        .choices
                        .first()
                        .and_then(|c| c.finish_reason.as_deref())
                        .unwrap_or("unknown");
                    crate::telemetry_otlp::set_gen_ai_response(
                        &tracing::Span::current(),
                        &response.id,
                        finish_reason,
                    );

                    cache.set(&request, &response).await;

                    let telemetry = telemetry.clone();
                    let model_owned = routing_result.deployment.model.clone();
                    tokio::spawn(async move {
                        if let Err(e) = telemetry
                            .record_with_tokens(
                                &key_owned,
                                &model_owned,
                                input_tokens,
                                output_tokens,
                                elapsed,
                            )
                            .await
                        {
                            tracing::warn!(error = %e, "telemetry record failed");
                        }
                    });

                    let total_tokens = response.usage.input_tokens + response.usage.output_tokens;
                    let _ = self
                        .rate_limiter
                        .record_usage(key, total_tokens as u64)
                        .await;

                    mirroring::maybe_mirror(
                        self.mirror.clone(),
                        self.http_caller.clone(),
                        Arc::clone(&self.router),
                        Arc::new(config),
                        key.to_string(),
                        request,
                    );

                    Ok(response)
                }
                Err(e) => {
                    tracing::error!(error = %e, "routing failed");
                    Err(HyperInferError::Config(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Routing failed: {}", e),
                    )))
                }
            }
```

- [ ] **Step 3: Update `chat_stream()` to use route_with_fallback**

Replace the model resolution block in `chat_stream()` (lines 379-426) with similar logic that uses `route_with_fallback` for the initial routing decision, then streams from the selected deployment. Note: streaming doesn't benefit from fallback chains in the same way, so we use `select_deployment` for the initial selection and handle failures at the stream level.

```rust
        let (deployment, provider_name, api_key) = {
            let config = self.config.read().await;
            let ctx = hyperinfer_router::RoutingContext::default();

            let deployment = if let Some(routing_state) = self.router.routing_state() {
                match self.router.engine().select_deployment(&request.model, routing_state.as_ref(), &ctx).await {
                    Ok(result) => result.deployment,
                    Err(_) => {
                        let (model, provider) = self.router.resolve(&request.model, &config).ok_or_else(|| {
                            HyperInferError::Config(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("Unknown model: '{}'. No routing rule or alias found.", request.model),
                            ))
                        })?;
                        Arc::new(hyperinfer_router::Deployment::new(
                            request.model.clone(),
                            provider.clone(),
                            model,
                            provider.to_string(),
                        ))
                    }
                }
            } else {
                let (model, provider) = self.router.resolve(&request.model, &config).ok_or_else(|| {
                    HyperInferError::Config(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Unknown model: '{}'. No routing rule or alias found.", request.model),
                    ))
                })?;
                Arc::new(hyperinfer_router::Deployment::new(
                    request.model.clone(),
                    provider.clone(),
                    model,
                    provider.to_string(),
                ))
            };

            let provider_name = deployment.provider.to_string();
            let api_key = config
                .api_keys
                .get(&provider_name)
                .cloned()
                .ok_or_else(|| {
                    HyperInferError::Config(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("API key not found for provider: {:?}", deployment.provider),
                    ))
                })?;

            (deployment, provider_name, api_key)
        };
```

Then update the streaming provider lookup and execution:

```rust
        let streaming_provider = {
            let registry = self.provider_registry.read().await;
            registry.get_streaming(&provider_name).ok_or_else(|| {
                HyperInferError::Config(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "Provider '{}' not found in registry or does not support streaming",
                        provider_name
                    ),
                ))
            })?
        };

        let mut resolved_request = request.clone();
        resolved_request.model = deployment.model.clone();
        let provider_stream: Pin<
            Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send>,
        > = streaming_provider.into_stream(&resolved_request, &api_key);
```

And update the AccountedStream initialization:

```rust
        let stream = AccountedStream {
            inner: provider_stream,
            telemetry: self.telemetry.clone(),
            rate_limiter: self.rate_limiter.clone(),
            key: key.to_string(),
            model: deployment.model.clone(),
            start: std::time::Instant::now(),
            input_tokens: 0,
            output_tokens: 0,
            accounted: false,
            span,
        };
```

- [ ] **Step 4: Verify client compiles**

Run: `cargo check -p hyperinfer-client`
Expected: Compiles without errors.

- [ ] **Step 5: Run existing client tests**

Run: `cargo test -p hyperinfer-client`
Expected: All tests pass (existing router tests may need updating to match new async API).

- [ ] **Step 6: Commit**

```bash
git add crates/hyperinfer-client/
git commit -m "feat(client): wire RouterEngine into chat() and chat_stream()"
```

---

## Phase 7: Python Bindings

### Task 10: Add Routing PyO3 Bindings

**Files:**
- Create: `crates/hyperinfer-python/src/routing.rs`
- Modify: `crates/hyperinfer-python/Cargo.toml`
- Modify: `crates/hyperinfer-python/src/lib.rs`

- [ ] **Step 1: Add hyperinfer-router dependency**

In `crates/hyperinfer-python/Cargo.toml`, add to `[dependencies]`:

```toml
hyperinfer-router = { path = "../hyperinfer-router" }
```

- [ ] **Step 2: Create routing.rs with PyO3 wrappers**

Create `crates/hyperinfer-python/src/routing.rs`:

```rust
use hyperinfer_router::{
    Deployment as RustDeployment, GlobalLimits, RouterEngine as RustRouterEngine,
    WeightedShuffle, LatencyBased, LeastBusy, UsageBased, CostBased,
};
use hyperinfer_core::Provider;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

#[pyclass]
#[pyo3(name = "Deployment")]
pub struct PyDeployment {
    inner: RustDeployment,
}

#[pymethods]
impl PyDeployment {
    #[new]
    #[pyo3(signature = (model_name, provider, model, api_key_ref, base_url=None, weight=1))]
    fn new(
        model_name: String,
        provider: String,
        model: String,
        api_key_ref: String,
        base_url: Option<String>,
        weight: u32,
    ) -> Self {
        let p = match provider.as_str() {
            "openai" => Provider::OpenAI,
            "anthropic" => Provider::Anthropic,
            _ => Provider::Other,
        };
        let mut d = RustDeployment::new(model_name, p, model, api_key_ref);
        d.weight = weight;
        if let Some(url) = base_url {
            d = d.with_base_url(url);
        }
        Self { inner: d }
    }

    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn model_name(&self) -> &str {
        &self.inner.model_name
    }

    #[getter]
    fn provider(&self) -> String {
        self.inner.provider.to_string()
    }

    #[getter]
    fn model(&self) -> &str {
        &self.inner.model
    }

    #[getter]
    fn weight(&self) -> u32 {
        self.inner.weight
    }
}

#[pyclass]
#[pyo3(name = "RouterEngine")]
pub struct PyRouterEngine {
    inner: Arc<RustRouterEngine>,
}

#[pymethods]
impl PyRouterEngine {
    #[new]
    fn new() -> Self {
        let engine = RustRouterEngine::new(GlobalLimits::default());
        Self {
            inner: Arc::new(engine),
        }
    }

    fn add_deployment(&self, py: Python<'_>, deployment: PyDeployment) -> PyResult<()> {
        let engine = Arc::clone(&self.inner);
        let dep = deployment.inner.clone();
        py.allow_threads(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                engine.add_deployment(dep).await;
            });
        });
        Ok(())
    }

    fn register_default_strategies(&self, py: Python<'_>) -> PyResult<()> {
        let engine = Arc::clone(&self.inner);
        py.allow_threads(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                engine.register_strategy(Box::new(WeightedShuffle::new())).await;
                engine.register_strategy(Box::new(LatencyBased::new())).await;
                engine.register_strategy(Box::new(LeastBusy::new())).await;
                engine.register_strategy(Box::new(UsageBased::new())).await;
                engine.register_strategy(Box::new(CostBased::new())).await;
            });
        });
        Ok(())
    }

    fn set_alias(&self, py: Python<'_>, alias: String, target: String) -> PyResult<()> {
        let engine = Arc::clone(&self.inner);
        py.allow_threads(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                engine.set_alias(&alias, &target).await;
            });
        });
        Ok(())
    }

    fn set_default_strategy(&self, py: Python<'_>, name: String) -> PyResult<()> {
        let engine = Arc::clone(&self.inner);
        py.allow_threads(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                engine.set_default_strategy(&name).await;
            });
        });
        Ok(())
    }
}
```

- [ ] **Step 3: Register routing classes in the Python module**

Update `crates/hyperinfer-python/src/lib.rs`:

```rust
mod client;
mod providers;
mod registry_wrapper;
mod routing;
mod types;

pub use client::{ChunkStream, HyperInferClient};
pub use registry_wrapper::{create_provider_registry, ProviderRegistryWrapper};
pub use routing::{PyDeployment, PyRouterEngine};
```

And in the `#[pymodule]` function:

```rust
    m.add_class::<PyDeployment>()?;
    m.add_class::<PyRouterEngine>()?;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p hyperinfer-python`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/hyperinfer-python/
git commit -m "feat(python): add RouterEngine and Deployment PyO3 bindings"
```

---

## Phase 8: Dashboard Routing View

### Task 11: Add Deployment Types and API Methods

**Files:**
- Modify: `apps/dashboard/src/lib/types.ts`
- Modify: `apps/dashboard/src/lib/api.ts`

- [ ] **Step 1: Add Deployment types**

Append to `apps/dashboard/src/lib/types.ts`:

```typescript
export interface Deployment {
    id: string;
    model_name: string;
    provider: string;
    model: string;
    api_key_ref: string;
    base_url?: string;
    weight: number;
    rpm_limit?: number;
    tpm_limit?: number;
    input_cost_per_1k?: number;
    output_cost_per_1k?: number;
    sort_order: number;
    tags: Record<string, string>;
    is_active: boolean;
    created_at: string;
    updated_at: string;
}

export interface DeploymentMetrics {
    latency_ewma_ms: number;
    in_flight: number;
    tpm_used: number;
    rpm_used: number;
    total_requests: number;
    total_failures: number;
}
```

- [ ] **Step 2: Add deployment API methods**

Append to the `api` object in `apps/dashboard/src/lib/api.ts`:

```typescript
  getDeployments: () => fetchApi<Deployment[]>("/deployments"),
  getDeployment: (id: string) => fetchApi<Deployment>(`/deployments/${id}`),
  createDeployment: (data: Partial<Deployment>) =>
    fetchApi<Deployment>("/deployments", {
      method: "POST",
      body: JSON.stringify(data),
    }),
  updateDeployment: (id: string, data: Partial<Deployment>) =>
    fetchApi<Deployment>(`/deployments/${id}`, {
      method: "PUT",
      body: JSON.stringify(data),
    }),
  deleteDeployment: (id: string) =>
    fetchApi<void>(`/deployments/${id}`, { method: "DELETE" }),
```

Update the import at the top of `api.ts`:

```typescript
import type { User, Team, ApiKey, UsageData, Deployment } from "./types";
```

- [ ] **Step 3: Commit**

```bash
git add apps/dashboard/src/lib/
git commit -m "feat(dashboard): add deployment types and API methods"
```

---

### Task 12: Create Routing Health Page

**Files:**
- Create: `apps/dashboard/src/routes/dashboard/routing/+page.svelte`
- Modify: `apps/dashboard/src/lib/components/Sidebar.svelte`

- [ ] **Step 1: Create the routing page**

Create `apps/dashboard/src/routes/dashboard/routing/+page.svelte`:

```svelte
<script lang="ts">
    import { api } from '$lib/api';
    import type { Deployment } from '$lib/types';

    let deployments = $state<Deployment[]>([]);
    let loading = $state(true);
    let error = $state<string | null>(null);

    async function loadDeployments() {
        try {
            loading = true;
            deployments = await api.getDeployments();
            error = null;
        } catch (e) {
            error = 'Failed to load deployments';
        } finally {
            loading = false;
        }
    }

    $effect(() => {
        loadDeployments();
    });

    function getStatusColor(d: Deployment): string {
        if (!d.is_active) return 'bg-gray-500';
        return 'bg-green-500';
    }
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold">Routing</h1>
        <button
            class="px-4 py-2 bg-[var(--accent)] text-white rounded-lg hover:opacity-90"
            onclick={() => loadDeployments()}
        >
            Refresh
        </button>
    </div>

    {#if loading}
        <div class="text-center py-12 text-[var(--text-secondary)]">Loading deployments...</div>
    {:else if error}
        <div class="text-center py-12 text-red-500">{error}</div>
    {:else if deployments.length === 0}
        <div class="text-center py-12 text-[var(--text-secondary)]">
            No deployments configured yet.
        </div>
    {:else}
        <div class="grid gap-4">
            {#each deployments as d}
                <div class="bg-[var(--bg-primary)] border border-[var(--bg-secondary)] rounded-lg p-4">
                    <div class="flex items-center justify-between">
                        <div class="flex items-center gap-3">
                            <div class="w-3 h-3 rounded-full {getStatusColor(d)}"></div>
                            <div>
                                <h3 class="font-semibold">{d.model_name}</h3>
                                <p class="text-sm text-[var(--text-secondary)]">
                                    {d.provider} / {d.model}
                                </p>
                            </div>
                        </div>
                        <div class="text-right text-sm text-[var(--text-secondary)]">
                            <div>Weight: {d.weight}</div>
                            {#if d.rpm_limit}
                                <div>RPM: {d.rpm_limit}</div>
                            {/if}
                        </div>
                    </div>
                    {#if d.base_url}
                        <div class="mt-2 text-xs text-[var(--text-secondary)] truncate">
                            {d.base_url}
                        </div>
                    {/if}
                </div>
            {/each}
        </div>
    {/if}
</div>
```

- [ ] **Step 2: Add routing nav item to Sidebar**

In `apps/dashboard/src/lib/components/Sidebar.svelte`, add a routing entry to `navItems`:

```typescript
    const navItems = [
        { path: '/dashboard/teams', label: 'Teams', icon: 'users', admin: true },
        { path: '/dashboard/keys', label: 'Keys', icon: 'key' },
        { path: '/dashboard/routing', label: 'Routing', icon: 'routing' },
        { path: '/dashboard/conversations', label: 'Conversations', icon: 'chat' },
        { path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
    ];
```

Add the routing icon SVG in the icon rendering block:

```svelte
                    {:else if item.icon === 'routing'}
                        <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>
```

- [ ] **Step 3: Verify dashboard builds**

Run: `cd apps/dashboard && npm run build` (or `pnpm build`)
Expected: Builds without errors.

- [ ] **Step 4: Commit**

```bash
git add apps/dashboard/
git commit -m "feat(dashboard): add routing health page with deployment list"
```

---

## Final Verification

### Task 13: Full Build and Test

- [ ] **Step 1: Run full workspace build**

Run: `cargo build --workspace`
Expected: All crates build successfully.

- [ ] **Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Run formatter**

Run: `cargo fmt --all -- --check`
Expected: No formatting issues.

- [ ] **Step 5: Final commit (if any fixups needed)**

```bash
git add -A
git commit -m "fix: address final review findings for routing system phases 4-8"
```
