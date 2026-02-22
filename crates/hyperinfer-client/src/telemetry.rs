use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_STREAM_KEY: &str = "hyperinfer:telemetry";

pub struct Telemetry {
    client: Option<Arc<redis::Client>>,
    stream_key: String,
}

impl Telemetry {
    pub async fn new(redis_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = match redis::Client::open(redis_url) {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                tracing::warn!("Invalid Redis URL for telemetry: {}", e);
                None
            }
        };

        Ok(Self {
            client,
            stream_key: DEFAULT_STREAM_KEY.to_string(),
        })
    }

    pub async fn with_stream_key(mut self, stream_key: &str) -> Self {
        self.stream_key = stream_key.to_string();
        self
    }

    pub async fn record(
        &self,
        key: &str,
        model: &str,
        response_time_ms: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let input_tokens = 0u32;
        let output_tokens = 0u32;

        self.record_with_tokens(key, model, input_tokens, output_tokens, response_time_ms)
            .await
    }

    pub async fn record_with_tokens(
        &self,
        key: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        response_time_ms: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Some(ref client) = self.client {
            let stream_key = self.stream_key.clone();
            let key_clone = key.to_string();
            let model_clone = model.to_string();
            let client = Arc::clone(client);

            tokio::spawn(async move {
                let conn_result = client.get_multiplexed_async_connection().await;
                if conn_result.is_err() {
                    tracing::error!("Failed to get Redis connection for telemetry");
                    return;
                }

                let mut conn = conn_result.unwrap();
                let result: Result<(), redis::RedisError> = redis::cmd("XADD")
                    .arg(&stream_key)
                    .arg("*")
                    .arg("key")
                    .arg(&key_clone)
                    .arg("model")
                    .arg(&model_clone)
                    .arg("input_tokens")
                    .arg(input_tokens.to_string())
                    .arg("output_tokens")
                    .arg(output_tokens.to_string())
                    .arg("response_time_ms")
                    .arg(response_time_ms.to_string())
                    .arg("timestamp")
                    .arg(timestamp.to_string())
                    .query_async(&mut conn)
                    .await;

                if result.is_err() {
                    tracing::error!(
                        "Failed to push telemetry to Redis stream: {:?}",
                        result.err()
                    );
                }
            });
        } else {
            tracing::debug!(
                "Telemetry skipped (Redis unavailable): key={}, model={}, input_tokens={}, output_tokens={}, response_time_ms={}",
                key, model, input_tokens, output_tokens, response_time_ms
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_telemetry_new() {
        let result = Telemetry::new("redis://localhost:6379").await;
        assert!(result.is_ok());
        let telemetry = result.unwrap();
        assert_eq!(telemetry.stream_key, "hyperinfer:telemetry");
    }

    #[tokio::test]
    async fn test_telemetry_new_different_url() {
        let result = Telemetry::new("redis://custom-host:1234/0").await;
        assert!(result.is_ok());
        let telemetry = result.unwrap();
        assert_eq!(telemetry.stream_key, "hyperinfer:telemetry");
    }

    #[tokio::test]
    async fn test_telemetry_with_stream_key() {
        let telemetry = Telemetry::new("redis://localhost:6379")
            .await
            .unwrap()
            .with_stream_key("custom:stream")
            .await;
        assert_eq!(telemetry.stream_key, "custom:stream");
    }

    #[tokio::test]
    async fn test_telemetry_record() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry.record("test-key", "gpt-4", 250).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_with_tokens() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry
            .record_with_tokens("test-key", "gpt-4", 100, 50, 250)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_multiple_calls() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();

        assert!(telemetry.record("key1", "gpt-4", 100).await.is_ok());
        assert!(telemetry.record("key2", "claude-3", 200).await.is_ok());
        assert!(telemetry.record("key1", "gpt-4", 150).await.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_zero_response_time() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry.record("test-key", "gpt-4", 0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_large_response_time() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry.record("test-key", "gpt-4", 999999).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_invalid_redis() {
        let telemetry = Telemetry::new("invalid-url").await.unwrap();
        let result = telemetry.record("test-key", "gpt-4", 250).await;
        assert!(result.is_ok());
    }
}
