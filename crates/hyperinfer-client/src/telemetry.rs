use hex;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_STREAM_KEY: &str = "hyperinfer:telemetry";

pub struct Telemetry {
    manager: Option<redis::aio::ConnectionManager>,
    stream_key: String,
}

impl Telemetry {
    /// Returns a truncated hash suffix of the key for safe logging
    fn key_id(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash = hasher.finalize();
        let hex_hash = hex::encode(hash);
        // Return last 8 characters of hash
        if hex_hash.len() >= 8 {
            format!("...{}", &hex_hash[hex_hash.len() - 8..])
        } else {
            hex_hash
        }
    }
    pub async fn new(redis_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let manager = match redis::Client::open(redis_url) {
            Ok(client) => match redis::aio::ConnectionManager::new(client).await {
                Ok(m) => Some(m),
                Err(e) => {
                    tracing::warn!("Failed to create Redis connection manager: {}", e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Invalid Redis URL for telemetry: {}", e);
                None
            }
        };

        Ok(Self {
            manager,
            stream_key: DEFAULT_STREAM_KEY.to_string(),
        })
    }

    pub fn with_stream_key(mut self, stream_key: &str) -> Self {
        if !stream_key.trim().is_empty() {
            self.stream_key = stream_key.to_string();
        }
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

        if let Some(ref manager) = self.manager {
            let stream_key = self.stream_key.clone();
            let key_clone = key.to_string();
            let model_clone = model.to_string();
            let mut manager = manager.clone();

            tokio::spawn(async move {
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
                    .query_async(&mut manager)
                    .await;

                if let Err(e) = result {
                    tracing::error!("Failed to push telemetry to Redis stream: {:?}", e);
                }
            });
        } else {
            tracing::debug!(
                "Telemetry skipped (Redis unavailable): key_id={}, model={}, input_tokens={}, output_tokens={}, response_time_ms={}",
                Self::key_id(key), model, input_tokens, output_tokens, response_time_ms
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
            .with_stream_key("custom:stream");
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

    #[tokio::test]
    async fn test_telemetry_with_empty_stream_key() {
        let telemetry = Telemetry::new("redis://localhost:6379")
            .await
            .unwrap()
            .with_stream_key("");
        assert_eq!(telemetry.stream_key, DEFAULT_STREAM_KEY);
    }

    #[tokio::test]
    async fn test_telemetry_with_whitespace_stream_key() {
        let telemetry = Telemetry::new("redis://localhost:6379")
            .await
            .unwrap()
            .with_stream_key("   ");
        assert_eq!(telemetry.stream_key, DEFAULT_STREAM_KEY);
    }

    #[tokio::test]
    async fn test_telemetry_record_empty_key() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry.record("", "gpt-4", 250).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_empty_model() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry.record("test-key", "", 250).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_max_values() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry
            .record_with_tokens("test-key", "gpt-4", u32::MAX, u32::MAX, u64::MAX)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_special_characters_in_key() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry.record("test-key-!@#$%^&*()", "gpt-4", 250).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_special_characters_in_model() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry
            .record("test-key", "gpt-4-turbo-preview-2024", 250)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_unicode() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let result = telemetry.record("test-key-🔑", "gpt-4", 250).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_record_very_long_strings() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let long_key = "a".repeat(10000);
        let long_model = "b".repeat(10000);
        let result = telemetry.record(&long_key, &long_model, 250).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_concurrent_records() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();
        let telemetry = std::sync::Arc::new(telemetry);

        let mut handles = vec![];
        for i in 0..10 {
            let telemetry_clone = telemetry.clone();
            let handle = tokio::spawn(async move {
                telemetry_clone
                    .record(&format!("key-{}", i), "gpt-4", 100 + i * 10)
                    .await
            });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_telemetry_record_sequential() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();

        for i in 0..5 {
            let result = telemetry
                .record(&format!("key-{}", i), "gpt-4", 100 + i * 10)
                .await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_telemetry_record_different_token_counts() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();

        let test_cases = vec![
            (0, 0),
            (1, 1),
            (100, 50),
            (u32::MAX, u32::MAX),
            (500, 0),
            (0, 500),
        ];

        for (input, output) in test_cases {
            let result = telemetry
                .record_with_tokens("test-key", "gpt-4", input, output, 250)
                .await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_telemetry_with_very_long_stream_key() {
        let long_key = "a".repeat(1000);
        let telemetry = Telemetry::new("redis://localhost:6379")
            .await
            .unwrap()
            .with_stream_key(&long_key);
        assert_eq!(telemetry.stream_key, long_key);
    }

    #[tokio::test]
    async fn test_telemetry_record_rapid_succession() {
        let telemetry = Telemetry::new("redis://localhost:6379").await.unwrap();

        for _ in 0..100 {
            let result = telemetry.record("test-key", "gpt-4", 250).await;
            assert!(result.is_ok());
        }
    }
}
