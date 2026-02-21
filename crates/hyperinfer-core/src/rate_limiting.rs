//! Rate limiting utilities for HyperInfer
//!
//! Provides distributed quota enforcement using Redis and GCRA algorithm.

use redis::Client;
use redis::aio::ConnectionManager;
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
    pub async fn new(
        redis_url: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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

    pub async fn is_allowed(
        &self,
        key: &str,
        amount: u64,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
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

            let allowed = result.first().copied().unwrap_or(0);
            if allowed == 0 {
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

            Ok(tpm_result.first().copied().unwrap_or(0) == 1)
        } else {
            Ok(true)
        }
    }

    pub async fn check_rpm(
        &self,
        key: &str,
        limit: u64,
    ) -> Result<(bool, u64), Box<dyn std::error::Error + Send + Sync>> {
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

            let allowed = result.first().copied().unwrap_or(0) == 1;
            let remaining = result.get(1).copied().unwrap_or(0);
            Ok((allowed, remaining))
        } else {
            Ok((true, limit))
        }
    }

    pub async fn check_tpm(
        &self,
        key: &str,
        limit: u64,
        tokens: u64,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
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

            Ok(result.first().copied().unwrap_or(0) == 1)
        } else {
            Ok(true)
        }
    }

    pub async fn record_usage(
        &self,
        key: &str,
        tokens_used: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_new_without_redis() {
        let result = RateLimiter::new(None).await;
        assert!(result.is_ok());
        let limiter = result.unwrap();
        assert_eq!(limiter.default_rpm, 60);
        assert_eq!(limiter.default_tpm, 100000);
    }

    #[tokio::test]
    async fn test_rate_limiter_is_allowed_without_redis() {
        let limiter = RateLimiter::new(None).await.unwrap();
        let result = limiter.is_allowed("test-key", 1).await;
        assert!(result.is_ok());
        // Without Redis, should always allow
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_rate_limiter_check_rpm_without_redis() {
        let limiter = RateLimiter::new(None).await.unwrap();
        let result = limiter.check_rpm("test-key", 100).await;
        assert!(result.is_ok());
        let (allowed, remaining) = result.unwrap();
        // Without Redis, should always allow
        assert!(allowed);
        assert_eq!(remaining, 100);
    }

    #[tokio::test]
    async fn test_rate_limiter_check_tpm_without_redis() {
        let limiter = RateLimiter::new(None).await.unwrap();
        let result = limiter.check_tpm("test-key", 1000, 100).await;
        assert!(result.is_ok());
        // Without Redis, should always allow
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_rate_limiter_record_usage_without_redis() {
        let limiter = RateLimiter::new(None).await.unwrap();
        let result = limiter.record_usage("test-key", 50).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket {
            capacity: 100,
            tokens: 100,
            refill_rate: 10,
            last_refill: Instant::now(),
        };

        assert_eq!(bucket.capacity, 100);
        assert_eq!(bucket.tokens, 100);
        assert_eq!(bucket.refill_rate, 10);
    }

    #[test]
    fn test_token_bucket_clone() {
        let bucket = TokenBucket {
            capacity: 50,
            tokens: 25,
            refill_rate: 5,
            last_refill: Instant::now(),
        };

        let cloned = bucket.clone();
        assert_eq!(bucket.capacity, cloned.capacity);
        assert_eq!(bucket.tokens, cloned.tokens);
        assert_eq!(bucket.refill_rate, cloned.refill_rate);
    }

    #[test]
    fn test_quota_creation() {
        let quota = Quota {
            max_requests_per_minute: Some(60),
            max_tokens_per_minute: Some(100000),
            budget_cents: Some(1000),
        };

        assert_eq!(quota.max_requests_per_minute, Some(60));
        assert_eq!(quota.max_tokens_per_minute, Some(100000));
        assert_eq!(quota.budget_cents, Some(1000));
    }

    #[test]
    fn test_quota_with_none_values() {
        let quota = Quota {
            max_requests_per_minute: None,
            max_tokens_per_minute: None,
            budget_cents: None,
        };

        assert_eq!(quota.max_requests_per_minute, None);
        assert_eq!(quota.max_tokens_per_minute, None);
        assert_eq!(quota.budget_cents, None);
    }

    #[test]
    fn test_quota_clone() {
        let quota = Quota {
            max_requests_per_minute: Some(100),
            max_tokens_per_minute: Some(50000),
            budget_cents: Some(2000),
        };

        let cloned = quota.clone();
        assert_eq!(
            quota.max_requests_per_minute,
            cloned.max_requests_per_minute
        );
        assert_eq!(quota.max_tokens_per_minute, cloned.max_tokens_per_minute);
        assert_eq!(quota.budget_cents, cloned.budget_cents);
    }

    #[tokio::test]
    async fn test_rate_limiter_is_allowed_with_zero_amount() {
        let limiter = RateLimiter::new(None).await.unwrap();
        let result = limiter.is_allowed("test-key", 0).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_rate_limiter_is_allowed_with_large_amount() {
        let limiter = RateLimiter::new(None).await.unwrap();
        let result = limiter.is_allowed("test-key", 999999).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_rate_limiter_check_rpm_with_different_limits() {
        let limiter = RateLimiter::new(None).await.unwrap();

        let result1 = limiter.check_rpm("key1", 10).await;
        assert!(result1.is_ok());
        assert_eq!(result1.unwrap().1, 10);

        let result2 = limiter.check_rpm("key2", 1000).await;
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().1, 1000);
    }

    #[tokio::test]
    async fn test_rate_limiter_record_usage_multiple_times() {
        let limiter = RateLimiter::new(None).await.unwrap();

        assert!(limiter.record_usage("key", 100).await.is_ok());
        assert!(limiter.record_usage("key", 200).await.is_ok());
        assert!(limiter.record_usage("key", 300).await.is_ok());
    }
}
