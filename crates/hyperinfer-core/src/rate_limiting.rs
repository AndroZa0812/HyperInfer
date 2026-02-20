//! Rate limiting utilities for HyperInfer
//!
//! Provides distributed quota enforcement using Redis and GCRA algorithm.

use redis::aio::ConnectionManager;
use redis::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

const GCRA_SCRIPT: &str = r#"
local key = KEYS[1]
local rate = tonumber(ARGV[1])
local capacity = tonumber(ARGV[2])
local now = tonumber(ARGV[3])
local cost = tonumber(ARGV[4])

local emission_interval = capacity / rate
local tat = redis.call('GET', key)

if not tat then
    tat = now
else
    tat = tonumber(tat)
end

local new_tat = math.max(tat, now) + cost * emission_interval
local allow_at = new_tat - capacity

if allow_at <= now then
    redis.call('SET', key, new_tat, 'EX', math.ceil(capacity * 2))
    return {1, 0}
else
    return {0, math.ceil(allow_at - now)}
end
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
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .as_millis() as u64;
            let rate = self.default_tpm / 60;
            let tpm_result: Vec<u64> = redis::cmd("EVAL")
                .arg(GCRA_SCRIPT)
                .arg(1)
                .arg(&tpm_key)
                .arg(rate)
                .arg(self.default_tpm)
                .arg(now)
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
            
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .as_millis() as u64;
            let rate = limit / 60;
            
            let result: Vec<u64> = redis::cmd("EVAL")
                .arg(GCRA_SCRIPT)
                .arg(1)
                .arg(format!("hyperinfer:ratelimit:tpm:{}", key))
                .arg(rate)
                .arg(limit)
                .arg(now)
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
