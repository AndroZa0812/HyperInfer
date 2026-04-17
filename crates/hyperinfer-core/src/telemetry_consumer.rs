//! Telemetry consumer for reading usage data from Redis Streams
//!
//! This consumer reads telemetry data pushed by hyperinfer-client from Redis Streams
//! and can forward it to a database for persistence.

use redis::aio::MultiplexedConnection;
use redis::Client;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::types::UsageRecord;

const DEFAULT_TELEMETRY_STREAM: &str = "hyperinfer:telemetry";
const DEFAULT_CONSUMER_GROUP: &str = "telemetry-consumer";
const XAUTOCLAIM_IDLE_MS: &str = "600000";
const XREADGROUP_BLOCK_MS: u32 = 5000;
const XREADGROUP_COUNT: u32 = 10;
const XAUTOCLAIM_COUNT: u32 = 100;
const MAX_BACKOFF_SECS: u64 = 60;

type StreamEntry = (String, Vec<(String, String)>);

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
        let result: Result<(), redis::RedisError> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(stream_key)
            .arg(consumer_group)
            .arg("0")
            .arg("MKSTREAM")
            .query_async(conn)
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.to_string().contains("BUSYGROUP") {
                    Ok(())
                } else {
                    Err(e.into())
                }
            }
        }
    }

    async fn ack_messages(
        conn: &mut MultiplexedConnection,
        stream_key: &str,
        consumer_group: &str,
        msg_ids: &[&str],
    ) -> Result<(), redis::RedisError> {
        Self::ack_messages_with_retry(conn, stream_key, consumer_group, msg_ids, 3, 50).await
    }

    async fn ack_messages_with_retry(
        conn: &mut MultiplexedConnection,
        stream_key: &str,
        consumer_group: &str,
        msg_ids: &[&str],
        max_retries: u32,
        base_delay_ms: u64,
    ) -> Result<(), redis::RedisError> {
        if msg_ids.is_empty() {
            return Ok(());
        }

        let mut last_error = None;
        for attempt in 0..max_retries {
            match Self::do_ack_messages(conn, stream_key, consumer_group, msg_ids).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_error = Some(e.clone());
                    if attempt < max_retries - 1 {
                        let delay_ms = base_delay_ms * (2_u64.pow(attempt));
                        warn!(
                            "XACK failed (attempt {}/{}), retrying in {}ms: {}",
                            attempt + 1,
                            max_retries,
                            delay_ms,
                            e
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }
        Err(last_error.unwrap())
    }

    async fn do_ack_messages(
        conn: &mut MultiplexedConnection,
        stream_key: &str,
        consumer_group: &str,
        msg_ids: &[&str],
    ) -> Result<(), redis::RedisError> {
        let mut cmd = redis::cmd("XACK");
        cmd.arg(stream_key).arg(consumer_group);
        for id in msg_ids {
            cmd.arg(id);
        }
        let count: usize = cmd.query_async(conn).await?;
        if count < msg_ids.len() {
            let remaining = msg_ids.len() - count;
            warn!(
                "XACK only acknowledged {}/{} messages; {} may need recovery on reconnect",
                count,
                msg_ids.len(),
                remaining
            );
        }
        Ok(())
    }

    async fn process_entry<F, Fut>(msg_id: &str, fields: &[(String, String)], handler: &F) -> bool
    where
        F: Fn(UsageRecord) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    {
        if let Some(record) = Self::parse_entry(Some(msg_id), fields) {
            match handler(record).await {
                Ok(_) => true,
                Err(e) => {
                    warn!("Failed to process message {}: {:?}", msg_id, e);
                    false
                }
            }
        } else {
            warn!("Failed to parse message {}", msg_id);
            true
        }
    }

    async fn recover_pending_messages<F, Fut>(
        conn: &mut MultiplexedConnection,
        stream_key: &str,
        consumer_group: &str,
        consumer_name: &str,
        handler: &F,
    ) -> Result<(), redis::RedisError>
    where
        F: Fn(UsageRecord) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    {
        let mut start_id = "0-0".to_string();
        loop {
            let result: Result<(String, Vec<StreamEntry>, Vec<redis::Value>), redis::RedisError> =
                redis::cmd("XAUTOCLAIM")
                    .arg(stream_key)
                    .arg(consumer_group)
                    .arg(consumer_name)
                    .arg(XAUTOCLAIM_IDLE_MS)
                    .arg(&start_id)
                    .arg("COUNT")
                    .arg(XAUTOCLAIM_COUNT)
                    .query_async(conn)
                    .await;

            let (next_start, claimed, _) = match result {
                Ok(res) => res,
                Err(e) => {
                    warn!("XAUTOCLAIM failed: {}", e);
                    return Err(e);
                }
            };

            let mut ack_ids = Vec::with_capacity(claimed.len());
            for (msg_id, fields) in &claimed {
                if Self::process_entry(msg_id, fields, handler).await {
                    ack_ids.push(msg_id.as_str());
                }
            }
            if !ack_ids.is_empty() {
                Self::ack_messages(conn, stream_key, consumer_group, &ack_ids).await?;
            }

            if next_start == "0-0" {
                return Ok(());
            }
            start_id = next_start;
        }
    }

    async fn read_and_process_batch<F, Fut>(
        conn: &mut MultiplexedConnection,
        stream_key: &str,
        consumer_group: &str,
        consumer_name: &str,
        handler: &F,
    ) -> Result<(), redis::RedisError>
    where
        F: Fn(UsageRecord) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    {
        #[allow(clippy::type_complexity)]
        let results: Vec<(String, Vec<(String, Vec<(String, String)>)>)> = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(consumer_group)
            .arg(consumer_name)
            .arg("COUNT")
            .arg(XREADGROUP_COUNT)
            .arg("BLOCK")
            .arg(XREADGROUP_BLOCK_MS)
            .arg("STREAMS")
            .arg(stream_key)
            .arg(">")
            .query_async(conn)
            .await?;

        for (_stream, entries) in results {
            let mut ack_ids = Vec::with_capacity(entries.len());
            for (entry_id, fields) in &entries {
                if Self::process_entry(entry_id, fields, handler).await {
                    ack_ids.push(entry_id.as_str());
                }
            }
            if !ack_ids.is_empty() {
                Self::ack_messages(conn, stream_key, consumer_group, &ack_ids).await?;
            }
        }

        Ok(())
    }

    pub async fn start_consuming<F, Fut>(
        &self,
        handler: F,
        cancellation_token: CancellationToken,
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
                if cancellation_token.is_cancelled() {
                    info!("Telemetry consumer shutting down");
                    return;
                }

                let config = redis::AsyncConnectionConfig::new().set_response_timeout(Some(
                    std::time::Duration::from_millis((XREADGROUP_BLOCK_MS + 5000) as u64),
                ));
                let conn_result = client
                    .get_multiplexed_async_connection_with_config(&config)
                    .await;
                if let Err(e) = &conn_result {
                    error!(
                        "Failed to connect to Redis: {}. Reconnecting in {}s",
                        e, backoff
                    );
                    tokio::select! {
                        _ = cancellation_token.cancelled() => {
                            info!("Telemetry consumer shutting down");
                            return;
                        }
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(backoff)) => {
                            backoff = (backoff * 2).min(MAX_BACKOFF_SECS);
                        }
                    }
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

                let recover_result = Self::recover_pending_messages(
                    &mut conn,
                    &stream_key,
                    &consumer_group,
                    &consumer_name,
                    &handler,
                )
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>);

                let mut do_reconnect = false;
                if let Err(e) = &recover_result {
                    warn!("Failed to recover pending messages: {}", e);
                    do_reconnect = true;
                }

                if do_reconnect {
                    error!("Recovery failed, reconnecting to retry on next cycle");
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF_SECS);
                    continue;
                }

                loop {
                    if cancellation_token.is_cancelled() {
                        info!("Telemetry consumer shutting down");
                        return;
                    }

                    tokio::select! {
                        result = Self::read_and_process_batch(
                            &mut conn,
                            &stream_key,
                            &consumer_group,
                            &consumer_name,
                            &handler,
                        ) => {
                            match result {
                                Ok(_) => {
                                    backoff = 1;
                                }
                                Err(e) => {
                                    error!(
                                        "Telemetry consumer error: {}. Reconnecting in {}s",
                                        e, backoff
                                    );
                                    backoff = (backoff * 2).min(MAX_BACKOFF_SECS);
                                    break;
                                }
                            }
                        }
                        _ = cancellation_token.cancelled() => {
                            info!("Telemetry consumer shutting down");
                            return;
                        }
                    }
                }
            }
        });

        Ok(handle)
    }

    fn parse_entry(msg_id: Option<&str>, fields: &[(String, String)]) -> Option<UsageRecord> {
        let mut map = std::collections::HashMap::new();
        for (k, v) in fields {
            map.insert(k.clone(), v.clone());
        }

        let key = map.get("key")?.clone();
        let model = map.get("model")?.clone();

        if key.trim().is_empty() || model.trim().is_empty() {
            return None;
        }

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
            msg_id: msg_id.map(String::from),
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
        let config = redis::AsyncConnectionConfig::new()
            .set_response_timeout(Some(std::time::Duration::from_millis(10000))); // Generous timeout for single batch
        let mut conn = self
            .client
            .get_multiplexed_async_connection_with_config(&config)
            .await?;

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
                if let Some(record) = Self::parse_entry(None, &fields) {
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

        let record = TelemetryConsumer::parse_entry(None, &fields);
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
    fn test_parse_entry_with_msg_id() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(Some("1234567890-0"), &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.key, "test-key");
        assert_eq!(record.model, "gpt-4");
        assert_eq!(record.msg_id, Some("1234567890-0".to_string()));
    }

    #[test]
    fn test_parse_entry_missing_field() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
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

        let record = TelemetryConsumer::parse_entry(None, &fields);
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

    #[test]
    fn test_parse_entry_extra_fields() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
            ("extra_field".to_string(), "ignored".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.key, "test-key");
    }

    #[test]
    fn test_parse_entry_empty() {
        let fields = vec![];
        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_none());
    }

    #[test]
    fn test_parse_entry_partial_fields() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_none());
    }

    #[test]
    fn test_parse_entry_negative_numbers() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "-100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_none());
    }

    #[test]
    fn test_parse_entry_overflow_u32() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "4294967296".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_none());
    }

    #[test]
    fn test_parse_entry_overflow_u64() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            (
                "response_time_ms".to_string(),
                "18446744073709551616".to_string(),
            ),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_none());
    }

    #[test]
    fn test_parse_entry_max_values() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), u32::MAX.to_string()),
            ("output_tokens".to_string(), u32::MAX.to_string()),
            ("response_time_ms".to_string(), u64::MAX.to_string()),
            ("timestamp".to_string(), u64::MAX.to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.input_tokens, u32::MAX);
        assert_eq!(record.output_tokens, u32::MAX);
        assert_eq!(record.response_time_ms, u64::MAX);
        assert_eq!(record.timestamp, u64::MAX);
    }

    #[test]
    fn test_parse_entry_zero_values() {
        let fields = vec![
            ("key".to_string(), "test-key".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "0".to_string()),
            ("output_tokens".to_string(), "0".to_string()),
            ("response_time_ms".to_string(), "0".to_string()),
            ("timestamp".to_string(), "0".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.input_tokens, 0);
        assert_eq!(record.output_tokens, 0);
        assert_eq!(record.response_time_ms, 0);
        assert_eq!(record.timestamp, 0);
    }

    #[test]
    fn test_parse_entry_empty_strings() {
        let fields = vec![
            ("key".to_string(), "".to_string()),
            ("model".to_string(), "".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_none());
    }

    #[test]
    fn test_parse_entry_whitespace_strings() {
        let fields = vec![
            ("key".to_string(), "   ".to_string()),
            ("model".to_string(), "   ".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_none());
    }

    #[test]
    fn test_parse_entry_special_characters() {
        let fields = vec![
            ("key".to_string(), "test-key-!@#$%".to_string()),
            ("model".to_string(), "gpt-4-turbo-preview".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.key, "test-key-!@#$%");
        assert_eq!(record.model, "gpt-4-turbo-preview");
    }

    #[test]
    fn test_parse_entry_unicode() {
        let fields = vec![
            ("key".to_string(), "test-key-🔑".to_string()),
            ("model".to_string(), "gpt-4".to_string()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.key, "test-key-🔑");
    }

    #[test]
    fn test_parse_entry_very_long_strings() {
        let long_key = "a".repeat(10000);
        let long_model = "b".repeat(10000);
        let fields = vec![
            ("key".to_string(), long_key.clone()),
            ("model".to_string(), long_model.clone()),
            ("input_tokens".to_string(), "100".to_string()),
            ("output_tokens".to_string(), "50".to_string()),
            ("response_time_ms".to_string(), "250".to_string()),
            ("timestamp".to_string(), "1700000000000".to_string()),
        ];

        let record = TelemetryConsumer::parse_entry(None, &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.key, long_key);
        assert_eq!(record.model, long_model);
    }
}
