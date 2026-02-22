//! Telemetry consumer for reading usage data from Redis Streams
//!
//! This consumer reads telemetry data pushed by hyperinfer-client from Redis Streams
//! and can forward it to a database for persistence.

use redis::aio::MultiplexedConnection;
use redis::Client;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::types::UsageRecord;

const DEFAULT_TELEMETRY_STREAM: &str = "hyperinfer:telemetry";
const DEFAULT_CONSUMER_GROUP: &str = "telemetry-consumer";

pub struct TelemetryConsumer {
    client: Arc<Client>,
    stream_key: String,
    consumer_group: String,
    consumer_name: String,
}

impl TelemetryConsumer {
    pub async fn new(redis_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Client::open(redis_url)?;
        let consumer_name = format!("consumer-{}", uuid::Uuid::new_v4());

        Ok(Self {
            client: Arc::new(client),
            stream_key: DEFAULT_TELEMETRY_STREAM.to_string(),
            consumer_group: DEFAULT_CONSUMER_GROUP.to_string(),
            consumer_name,
        })
    }

    pub fn with_stream_key(mut self, stream_key: &str) -> Self {
        self.stream_key = stream_key.to_string();
        self
    }

    pub fn with_consumer_group(mut self, group: &str) -> Self {
        self.consumer_group = group.to_string();
        self
    }

    async fn ensure_consumer_group(
        conn: &mut MultiplexedConnection,
        stream_key: &str,
        consumer_group: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _: Result<(), redis::RedisError> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(stream_key)
            .arg(consumer_group)
            .arg("0")
            .arg("MKSTREAM")
            .query_async(conn)
            .await;
        Ok(())
    }

    pub async fn start_consuming<F, Fut>(
        &self,
        handler: F,
    ) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn(UsageRecord) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    {
        let client = Arc::clone(&self.client);
        let stream_key = self.stream_key.clone();
        let consumer_group = self.consumer_group.clone();
        let consumer_name = self.consumer_name.clone();

        let handle = tokio::spawn(async move {
            let mut backoff = 1u64;

            loop {
                let conn_result = client.get_multiplexed_async_connection().await;
                if let Err(e) = &conn_result {
                    error!(
                        "Failed to connect to Redis: {}. Reconnecting in {}s",
                        e, backoff
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(60);
                    continue;
                }

                let mut conn = conn_result.unwrap();
                if let Err(e) =
                    Self::ensure_consumer_group(&mut conn, &stream_key, &consumer_group).await
                {
                    warn!("Failed to ensure consumer group: {}", e);
                }

                info!(
                    "Starting telemetry consumption from stream: {} (group: {})",
                    stream_key, consumer_group
                );

                loop {
                    #[allow(clippy::type_complexity)]
                    let results: Result<
                        Vec<(String, Vec<(String, Vec<(String, String)>)>)>,
                        redis::RedisError,
                    > = redis::cmd("XREADGROUP")
                        .arg("GROUP")
                        .arg(&consumer_group)
                        .arg(&consumer_name)
                        .arg("COUNT")
                        .arg(10)
                        .arg("BLOCK")
                        .arg(5000)
                        .arg("STREAMS")
                        .arg(&stream_key)
                        .arg(">")
                        .query_async(&mut conn)
                        .await;

                    match results {
                        Ok(results) => {
                            backoff = 1; // Reset backoff on successful read
                            for (_stream, entries) in results {
                                for (entry_id, fields) in entries {
                                    if let Some(record) = Self::parse_entry(&fields) {
                                        let handler_result = handler(record).await;
                                        if handler_result.is_ok() {
                                            let ack_result: Result<(), redis::RedisError> =
                                                redis::cmd("XACK")
                                                    .arg(&stream_key)
                                                    .arg(&consumer_group)
                                                    .arg(&entry_id)
                                                    .query_async(&mut conn)
                                                    .await;
                                            if let Err(e) = ack_result {
                                                warn!("Failed to XACK entry {}: {}", entry_id, e);
                                            }
                                        } else {
                                            warn!(
                                                "Failed to process telemetry record: {:?}",
                                                handler_result.err()
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                "Telemetry consumer error: {}. Reconnecting in {}s",
                                e, backoff
                            );
                            tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
                            backoff = (backoff * 2).min(60);
                            break;
                        }
                    }
                }
            }
        });

        Ok(handle)
    }

    fn parse_entry(fields: &[(String, String)]) -> Option<UsageRecord> {
        let mut map = std::collections::HashMap::new();
        for (k, v) in fields {
            map.insert(k.clone(), v.clone());
        }

        let key = map.get("key")?.clone();
        let model = map.get("model")?.clone();
        let input_tokens: u32 = map.get("input_tokens")?.parse().ok()?;
        let output_tokens: u32 = map.get("output_tokens")?.parse().ok()?;
        let response_time_ms: u64 = map.get("response_time_ms")?.parse().ok()?;
        let timestamp: u64 = map.get("timestamp")?.parse().ok()?;

        Some(UsageRecord {
            key,
            model,
            input_tokens,
            output_tokens,
            response_time_ms,
            timestamp,
        })
    }

    /// Read a single batch of messages from the stream.
    ///
    /// **Note**: This method always reads from the beginning of the stream (ID "0")
    /// and is intended for one-time reads or testing purposes only. For production
    /// use with repeated batch reads, use `start_consuming` which leverages
    /// consumer groups for proper message tracking and acknowledgment.
    pub async fn read_single_batch(
        &self,
    ) -> Result<Vec<UsageRecord>, Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        #[allow(clippy::type_complexity)]
        let results: Vec<(String, Vec<(String, Vec<(String, String)>)>)> = redis::cmd("XREAD")
            .arg("COUNT")
            .arg(100)
            .arg("STREAMS")
            .arg(&self.stream_key)
            .arg("0")
            .query_async(&mut conn)
            .await?;

        let mut records = Vec::new();
        for (_stream, entries) in results {
            for (_entry_id, fields) in entries {
                if let Some(record) = Self::parse_entry(&fields) {
                    records.push(record);
                }
            }
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_entry_valid() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(&fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.key, "test-key");
        assert_eq!(record.model, "gpt-4");
        assert_eq!(record.input_tokens, 100);
        assert_eq!(record.output_tokens, 50);
        assert_eq!(record.response_time_ms, 250);
        assert_eq!(record.timestamp, 1700000000000);
    }

    #[test]
    fn test_parse_entry_missing_field() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(&fields);
        assert!(record.is_none());
    }

    #[test]
    fn test_parse_entry_invalid_number() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "not-a-number".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(&fields);
        assert!(record.is_none());
    }

    #[tokio::test]
    async fn test_telemetry_consumer_new() {
        let result = TelemetryConsumer::new("redis://localhost:6379").await;
        assert!(result.is_ok());
        let consumer = result.unwrap();
        assert_eq!(consumer.stream_key, "hyperinfer:telemetry");
    }

    #[tokio::test]
    async fn test_telemetry_consumer_with_options() {
        let consumer = TelemetryConsumer::new("redis://localhost:6379")
            .await
            .unwrap()
            .with_stream_key("custom:stream")
            .with_consumer_group("custom-group");

        assert_eq!(consumer.stream_key, "custom:stream");
        assert_eq!(consumer.consumer_group, "custom-group");
    }
}
