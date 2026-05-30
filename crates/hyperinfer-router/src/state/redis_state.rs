use crate::error::RoutingError;
use crate::strategy::{DeploymentMetrics, RecordFailureResult, RoutingState};
use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::Client;
use std::collections::HashMap;
use tracing::warn;

const METRICS_KEY_PREFIX: &str = "hyperinfer:routing:metrics:";

const METRIC_UPDATE_SCRIPT: &str = r#"
local prefix = KEYS[1]
local outcome = ARGV[1]
local latency_ms = tonumber(ARGV[2])
local token_count = tonumber(ARGV[3])
local alpha = tonumber(ARGV[4])
local allowed_fails = tonumber(ARGV[5])
local cooldown_secs = tonumber(ARGV[6])
local tpm_ttl = tonumber(ARGV[7])
local latency_ttl = tonumber(ARGV[8])
local failures_ttl = tonumber(ARGV[9])

local in_flight = tonumber(redis.call('GET', prefix .. ':in_flight') or '0')
if in_flight > 0 then
    redis.call('DECR', prefix .. ':in_flight')
end

if outcome == 'success' then
    local old_ewma = tonumber(redis.call('GET', prefix .. ':latency') or '0')
    local new_ewma
    if old_ewma == 0 or old_ewma == nil then
        new_ewma = latency_ms
    else
        new_ewma = alpha * latency_ms + (1 - alpha) * old_ewma
    end
    redis.call('SET', prefix .. ':latency', tostring(new_ewma), 'EX', latency_ttl)

    if token_count > 0 then
        local tpm_key = prefix .. ':tpm'
        redis.call('INCRBY', tpm_key, token_count)
        if redis.call('TTL', tpm_key) == -1 then
            redis.call('EXPIRE', tpm_key, tpm_ttl)
        end
    end

    redis.call('SET', prefix .. ':failures', '0', 'EX', failures_ttl)
    redis.call('INCR', prefix .. ':total_req')
    return {0, 0}
else
    local fails = redis.call('INCR', prefix .. ':failures')
    if redis.call('TTL', prefix .. ':failures') == -1 then
        redis.call('EXPIRE', prefix .. ':failures', failures_ttl)
    end
    local cooldown_triggered = 0
    if fails >= allowed_fails then
        redis.call('SET', prefix .. ':cooldown', '1', 'EX', cooldown_secs)
        cooldown_triggered = 1
    end
    redis.call('INCR', prefix .. ':total_fail')
    return {fails, cooldown_triggered}
end
"#;

fn parse_u64(v: &Option<String>) -> u64 {
    v.as_deref()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

fn parse_f64(v: &Option<String>) -> f64 {
    v.as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn metrics_from_values(values: &[Option<String>], base: usize) -> DeploymentMetrics {
    DeploymentMetrics {
        latency_ewma_ms: parse_f64(&values[base]),
        in_flight: parse_u64(&values[base + 1]),
        tpm_used: parse_u64(&values[base + 2]),
        rpm_used: parse_u64(&values[base + 3]),
        total_requests: parse_u64(&values[base + 4]),
        total_failures: parse_u64(&values[base + 5]),
        last_failure_ts: None,
    }
}

#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub alpha: f64,
    pub allowed_fails: u64,
    pub cooldown_secs: u64,
    pub tpm_ttl_secs: u64,
    pub latency_ttl_secs: u64,
    pub failures_ttl_secs: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            alpha: 0.3,
            allowed_fails: 3,
            cooldown_secs: 30,
            tpm_ttl_secs: 60,
            latency_ttl_secs: 600,
            failures_ttl_secs: 300,
        }
    }
}

#[derive(Clone)]
pub struct RedisRoutingState {
    manager: ConnectionManager,
    config: RedisConfig,
}

impl RedisRoutingState {
    pub async fn new(redis_url: &str, config: RedisConfig) -> Result<Self, RoutingError> {
        let client = Client::open(redis_url)?;
        let manager = ConnectionManager::new(client).await?;
        Ok(Self { manager, config })
    }

    fn prefix(id: &str) -> String {
        format!("{}{}", METRICS_KEY_PREFIX, id)
    }
}

#[async_trait]
impl RoutingState for RedisRoutingState {
    async fn get_metrics(&self, deployment_id: &str) -> Result<DeploymentMetrics, RoutingError> {
        let prefix = Self::prefix(deployment_id);
        let mut conn = self.manager.clone();

        let keys = [
            format!("{}:latency", prefix),
            format!("{}:in_flight", prefix),
            format!("{}:tpm", prefix),
            format!("{}:rpm", prefix),
            format!("{}:total_req", prefix),
            format!("{}:total_fail", prefix),
        ];

        let values: Vec<Option<String>> = redis::cmd("MGET")
            .arg(&keys[0])
            .arg(&keys[1])
            .arg(&keys[2])
            .arg(&keys[3])
            .arg(&keys[4])
            .arg(&keys[5])
            .query_async(&mut conn)
            .await?;

        Ok(metrics_from_values(&values, 0))
    }

    async fn get_all_metrics(
        &self,
        ids: &[&str],
    ) -> Result<HashMap<String, DeploymentMetrics>, RoutingError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.manager.clone();
        let keys_per_deployment = 6;
        let mut all_keys: Vec<String> = Vec::with_capacity(ids.len() * keys_per_deployment);

        for id in ids {
            let prefix = Self::prefix(id);
            all_keys.push(format!("{}:latency", prefix));
            all_keys.push(format!("{}:in_flight", prefix));
            all_keys.push(format!("{}:tpm", prefix));
            all_keys.push(format!("{}:rpm", prefix));
            all_keys.push(format!("{}:total_req", prefix));
            all_keys.push(format!("{}:total_fail", prefix));
        }

        let mut cmd = redis::cmd("MGET");
        for key in &all_keys {
            cmd.arg(key);
        }

        let values: Vec<Option<String>> = cmd.query_async(&mut conn).await?;

        let mut result = HashMap::with_capacity(ids.len());
        for (i, id) in ids.iter().enumerate() {
            let base = i * keys_per_deployment;
            if base + 5 < values.len() {
                result.insert(id.to_string(), metrics_from_values(&values, base));
            }
        }

        Ok(result)
    }

    async fn is_cooled_down(&self, deployment_id: &str) -> Result<bool, RoutingError> {
        let prefix = Self::prefix(deployment_id);
        let mut conn = self.manager.clone();

        let val: Option<String> = redis::cmd("GET")
            .arg(format!("{}:cooldown", prefix))
            .query_async(&mut conn)
            .await?;

        Ok(val.is_some())
    }

    async fn record_request_start(&self, deployment_id: &str) -> Result<(), RoutingError> {
        let prefix = Self::prefix(deployment_id);
        let manager = self.manager.clone();
        let deployment_id = deployment_id.to_string();

        tokio::spawn(async move {
            let mut conn = manager.clone();
            let rpm_key = format!("{}:rpm", prefix);
            let pipe = redis::pipe()
                .atomic()
                .incr(format!("{}:in_flight", prefix), 1)
                .incr(&rpm_key, 1)
                .cmd("EXPIRE")
                .arg(&rpm_key)
                .arg(60)
                .clone();

            if let Err(e) = pipe.query_async::<()>(&mut conn).await {
                warn!(deployment_id = %deployment_id, error = %e, "failed to record request start");
            }
        });

        Ok(())
    }

    async fn record_request_success(
        &self,
        deployment_id: &str,
        latency_ms: f64,
        tokens: u64,
    ) -> Result<(), RoutingError> {
        let prefix = Self::prefix(deployment_id);
        let manager = self.manager.clone();
        let config = self.config.clone();
        let deployment_id = deployment_id.to_string();

        tokio::spawn(async move {
            let mut conn = manager.clone();
            let result: Result<Vec<u64>, redis::RedisError> = redis::cmd("EVAL")
                .arg(METRIC_UPDATE_SCRIPT)
                .arg(1)
                .arg(&prefix)
                .arg("success")
                .arg(latency_ms)
                .arg(tokens)
                .arg(config.alpha)
                .arg(config.allowed_fails)
                .arg(config.cooldown_secs)
                .arg(config.tpm_ttl_secs)
                .arg(config.latency_ttl_secs)
                .arg(config.failures_ttl_secs)
                .query_async(&mut conn)
                .await;

            if let Err(e) = result {
                warn!(deployment_id = %deployment_id, error = %e, "failed to record request success");
            }
        });

        Ok(())
    }

    async fn record_request_failure(
        &self,
        deployment_id: &str,
    ) -> Result<RecordFailureResult, RoutingError> {
        let prefix = Self::prefix(deployment_id);
        let mut conn = self.manager.clone();

        let result: Vec<u64> = redis::cmd("EVAL")
            .arg(METRIC_UPDATE_SCRIPT)
            .arg(1)
            .arg(&prefix)
            .arg("failure")
            .arg(0.0_f64)
            .arg(0_u64)
            .arg(self.config.alpha)
            .arg(self.config.allowed_fails)
            .arg(self.config.cooldown_secs)
            .arg(self.config.tpm_ttl_secs)
            .arg(self.config.latency_ttl_secs)
            .arg(self.config.failures_ttl_secs)
            .query_async(&mut conn)
            .await?;

        let failure_count = result.first().copied().unwrap_or(0);
        let cooldown_triggered = result.get(1).copied().unwrap_or(0);

        Ok(RecordFailureResult {
            failure_count,
            cooldown_triggered: cooldown_triggered == 1,
        })
    }
}
