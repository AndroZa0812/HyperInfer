# Phase 1 Implementation Details

## Overview
This document details the implementation plan to complete Phase 1 of HyperInfer: The Core Mesh & Governance.

---

## 1. Database Schema (PostgreSQL)

### Location
Create at: `crates/hyperinfer-server/migrations/`

### Tables to Create

```sql
-- teams.sql
CREATE TABLE teams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    budget_cents BIGINT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- users.sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    email VARCHAR(255) NOT NULL UNIQUE,
    role VARCHAR(50) DEFAULT 'member',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- api_keys.sql
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash VARCHAR(255) NOT NULL UNIQUE,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    name VARCHAR(255),
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE
);

-- model_aliases.sql
CREATE TABLE model_aliases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    alias VARCHAR(255) NOT NULL,
    target_model VARCHAR(255) NOT NULL,
    provider VARCHAR(50) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(team_id, alias)
);

-- quotas.sql
CREATE TABLE quotas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE UNIQUE,
    rpm_limit INTEGER DEFAULT 60,
    tpm_limit INTEGER DEFAULT 100000,
    budget_cents BIGINT DEFAULT 0,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
```

---

## 2. HTTP Client Implementation (Data Plane)

### File: `crates/hyperinfer-client/src/lib.rs`

Replace with actual implementation:

```rust
//! HyperInfer Client Library - Data Plane

pub mod http_client;
pub mod router;
pub mod telemetry;

pub use http_client::HttpCaller;
pub use router::Router;
pub use telemetry::Telemetry;

// Main client struct
pub struct HyperInferClient {
    config: Config,
    http_caller: HttpCaller,
    router: Router,
    rate_limiter: RateLimiter,
    telemetry: Telemetry,
}

impl HyperInferClient {
    pub async fn new(redis_url: &str, config: Config) -> Result<Self, HyperInferError> {
        let http_caller = HttpCaller::new()?;
        let router = Router::new(config.routing_rules.clone());
        let rate_limiter = RateLimiter::new(Some(redis_url))?;
        let telemetry = Telemetry::new(redis_url).await?;
        
        Ok(Self {
            config,
            http_caller,
            router,
            rate_limiter,
            telemetry,
        })
    }

    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, HyperInferError> {
        // 1. Check rate limit
        // 2. Resolve model alias
        // 3. Execute HTTP call
        // 4. Record telemetry
        // 5. Return response
    }
}
```

### File: `crates/hyperinfer-client/src/http_client.rs`

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct HttpCaller {
    client: Client,
}

impl HttpCaller {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self { client })
    }

    pub async fn call_openai(
        &self,
        endpoint: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, HyperInferError> { /* ... */ }

    pub async fn call_anthropic(
        &self,
        endpoint: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, HyperInferError> { /* ... */ }
}
```

### File: `crates/hyperinfer-client/src/router.rs`

```rust
pub struct Router {
    rules: Vec<RoutingRule>,
}

impl Router {
    pub fn resolve(&self, model: &str) ->ResolvedModel {/* ... */}
    
    pub fn get_fallback(&self, model: &str) -> Option<String> {/* ... */}
}
```

---

## 3. Redis Pub/Sub Config Sync

### File: `crates/hyperinfer-core/src/redis.rs`

Implement actual subscription:

```rust
impl ConfigManager {
    pub async fn subscribe_to_config_updates(
        &self,
        config: Arc<RwLock<Config>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = self.client.get_async_pubsub().await?;
        
        conn.subscribe("hyperinfer:config_updates").await?;
        
        let mut stream = conn.on_message();
        
        tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                let payload: ConfigUpdate = serde_json::from_str(&msg.get_payload()?)?;
                let mut cfg = config.write().await;
                *cfg = payload.config;
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        });
        
        Ok(())
    }

    pub async fn fetch_config(&self) -> Result<Config, Box<dyn std::error::Error>> {
        let mut conn = self.client.get_async_connection().await?;
        let data: Vec<u8> = redis::cmd("GET")
            .arg("hyperinfer:config")
            .query_async(&mut conn)
            .await?;
        
        Ok(serde_json::from_slice(&data)?)
    }
}
```

---

## 4. Rate Limiting (GCRA via Redis Lua)

### File: `crates/hyperinfer-core/src/rate_limiting.rs`

Implement Redis Lua script:

```rust
const GCRA_SCRIPT: &str = r#"
local key = KEYS[1]
local limit = tonumber(ARGV[1])
local window = tonumber(ARGV[2])
local now = tonumber(ARGV[3])

local current = redis.call('GET', key)
if current then
    local remaining = limit - tonumber(current)
    if remaining < 0 then
        return {0, tonumber(current)}
    end
end

redis.call('SETEX', key, window, 1)
return {1, 1}
"#;

impl RateLimiter {
    pub async fn is_allowed(&self, key: &str, amount: u64) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(ref client) = self.redis_client {
            let mut conn = client.get_async_connection().await?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64;
            
            let result: Vec<u64> = redis::cmd("EVAL")
                .arg(GCRA_SCRIPT)
                .arg(1)
                .arg(format!("hyperinfer:ratelimit:{}", key))
                .arg(amount)
                .arg(60) // window in seconds
                .arg(now)
                .query_async(&mut conn)
                .await?;
            
            Ok(result[0] == 1)
        } else {
            Ok(true)
        }
    }
}
```

---

## 5. Telemetry (Redis Streams)

### File: `crates/hyperinfer-client/src/telemetry.rs`

```rust
use redis::AsyncCommands;

pub struct Telemetry {
    producer: redis::Client,
}

impl Telemetry {
    pub async fn new(redis_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let producer = redis::Client::open(redis_url)?;
        Ok(Self { producer })
    }

    pub async fn record(&self, usage: &Usage) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = self.producer.get_async_connection().await?;
        
        let fields = [
            ("key", usage.key.as_str()),
            ("model", usage.model.as_str()),
            ("input_tokens", &usage.input_tokens.to_string()),
            ("output_tokens", &usage.output_tokens.to_string()),
            ("latency_ms", &usage.latency_ms.to_string()),
            ("timestamp", &usage.timestamp.to_string()),
        ];
        
        conn.xadd(
            "hyperinfer:usage",
            "*",
            &fields,
        ).await?;
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Usage {
    pub key: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub latency_ms: u64,
    pub timestamp: u64,
}
```

---

## 6. Server API Endpoints

### File: `crates/hyperinfer-server/src/main.rs`

Implement Axum routes:

```rust
use axum::{
    Router,
    routing::{get, post},
    extract::State,
    Json,
};
use hyperinfer_core::types::*;

pub async fn config_sync() -> Json<Config> {/* ... */}

pub async fn chat_completions(
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, HyperInferError> {/* ... */}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/v1/config/sync", get(config_sync))
        .route("/v1/chat/completions", post(chat_completions));
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

---

## 7. Implementation Order

| Step | Task | Priority |
|------|------|----------|
| 1 | Create SQL migrations for database schema | High |
| 2 | Implement HTTP client with OpenAI/Anthropic calls | High |
| 3 | Implement model routing and alias resolution | High |
| 4 | Implement Redis Lua GCRA rate limiter | High |
| 5 | Implement Redis Pub/Sub config sync | High |
| 6 | Implement Redis Stream telemetry | High |
| 7 | Create Axum server routes | High |
| 8 | Add sqlx database integration | Medium |

---

## 8. Dependencies to Add

In `Cargo.toml` files:

**hyperinfer-client/Cargo.toml:**
```toml
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["full"] }
```

**hyperinfer-server/Cargo.toml:**
```toml
axum = "0.8"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid"] }
tower = "0.5"
```

**hyperinfer-core/Cargo.toml:**
```toml
redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }
```
