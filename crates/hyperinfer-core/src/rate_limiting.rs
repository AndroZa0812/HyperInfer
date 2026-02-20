//! Rate limiting utilities for HyperInfer
//!
//! Provides distributed quota enforcement using Redis and GCRA algorithm.

use redis::aio::ConnectionManager;
use redis::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

const GCRA_SCRIPT: &str = r#"
local key = KEYS[1]
local limit = tonumber(ARGV[1])
local window = tonumber(ARGV[2])
local tokens = tonumber(ARGV[3])

local current = tonumber(redis.call('GET', key) or "0")
local new_total = current + tokens

if new_total > limit then
    return {0, current}
end

if current == 0 then
    redis.call('SETEX', key, window, tokens)
else
    redis.call('INCRBY', key, tokens)
end

return {1, new_total}
"#;

const RPM_SCRIPT: &str = r#"
local key = KEYS[1]
local limit = tonumber(ARGV[1])
local window = tonumber(ARGV[2])

local current = redis.call('INCR', key)
if current == 1 then
    redis.call('EXPIRE', key, window)
end

if current > limit then
    local ttl = redis.call('TTL', key)
    return {0, 0, ttl}
end
return {1, limit - current, 0}
"#;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub capacity: u64,
    pub tokens: u64,
    pub refill_rate: u64,
    pub last_refill: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quota {
    pub max_requests_per_minute: Option<u64>,
    pub max_tokens_per_minute: Option<u64>,
    pub budget_cents: Option<u64>,
}

pub struct RateLimiter {
    redis_manager: Option<ConnectionManager>,
    default_rpm: u64,
    default_tpm: u64,
}

impl RateLimiter {
    pub async fn new(redis_url: Option<&str>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let redis_manager = match redis_url {
            Some(url) => {
                let client = Client::open(url)?;
                Some(ConnectionManager::new(client).await?)
            }
            None => None,
        };
        Ok(Self {
            redis_manager,
            default_rpm: 60,
            default_tpm: 100000,
        })
    }

    pub async fn is_allowed(&self, key: &str, amount: u64) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref manager) = self.redis_manager {
            let mut conn = manager.clone();
            
            let result: Vec<u64> = redis::cmd("EVAL")
                .arg(RPM_SCRIPT)
                .arg(1)
                .arg(format!("hyperinfer:ratelimit:rpm:{}", key))
                .arg(self.default_rpm)
                .arg(60)
                .query_async(&mut conn)
                .await?;
            
            if result[0] == 0 {
                return Ok(false);
            }

            let tpm_key = format!("hyperinfer:ratelimit:tpm:{}", key);
            let tpm_result: Vec<u64> = redis::cmd("EVAL")
                .arg(GCRA_SCRIPT)
                .arg(1)
                .arg(&tpm_key)
                .arg(self.default_tpm)
                .arg(60)
                .arg(amount)
                .query_async(&mut conn)
                .await?;

            Ok(tpm_result[0] == 1)
        } else {
            Ok(true)
        }
    }

    pub async fn check_rpm(&self, key: &str, limit: u64) -> Result<(bool, u64), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref manager) = self.redis_manager {
            let mut conn = manager.clone();
            
            let result: Vec<u64> = redis::cmd("EVAL")
                .arg(RPM_SCRIPT)
                .arg(1)
                .arg(format!("hyperinfer:ratelimit:rpm:{}", key))
                .arg(limit)
                .arg(60)
                .query_async(&mut conn)
                .await?;
            
            Ok((result[0] == 1, result[1]))
        } else {
            Ok((true, limit))
        }
    }

    pub async fn check_tpm(&self, key: &str, limit: u64, tokens: u64) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref manager) = self.redis_manager {
            let mut conn = manager.clone();
            
            let result: Vec<u64> = redis::cmd("EVAL")
                .arg(GCRA_SCRIPT)
                .arg(1)
                .arg(format!("hyperinfer:ratelimit:tpm:{}", key))
                .arg(limit)
                .arg(60)
                .arg(tokens)
                .query_async(&mut conn)
                .await?;
            
            Ok(result[0] == 1)
        } else {
            Ok(true)
        }
    }

    pub async fn record_usage(&self, key: &str, tokens_used: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref manager) = self.redis_manager {
            let mut conn = manager.clone();
            
            redis::pipe()
                .atomic()
                .cmd("INCRBY")
                .arg(format!("hyperinfer:usage:tokens:{}", key))
                .arg(tokens_used)
                .cmd("INCR")
                .arg(format!("hyperinfer:usage:requests:{}", key))
                .query_async::<()>(&mut conn)
                .await?;
        }
        Ok(())
    }
}
