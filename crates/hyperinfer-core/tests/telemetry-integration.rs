use hyperinfer_core::{TelemetryConsumer, UsageRecord};
use testcontainers::{runners::AsyncRunner, ContainerAsync};
use testcontainers_modules::redis::Redis;

async fn setup_redis() -> (String, ContainerAsync<Redis>) {
    let redis = Redis::default()
        .start()
        .await
        .expect("Failed to start Redis container");
    let port = redis.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", port);
    (redis_url, redis)
}

#[tokio::test]
async fn test_telemetry_consumer_new() {
    let (redis_url, _container) = setup_redis().await;

    let result = TelemetryConsumer::new(&redis_url).await;
    assert!(result.is_ok(), "Should create consumer successfully");
}

#[tokio::test]
async fn test_telemetry_consumer_with_custom_settings() {
    let (redis_url, _container) = setup_redis().await;

    let result = TelemetryConsumer::new(&redis_url).await;
    assert!(result.is_ok(), "Should create consumer");

    let consumer = result
        .unwrap()
        .with_stream_key("custom:stream")
        .with_consumer_group("custom-group");

    // Can't directly verify fields, but verify it doesn't panic
    let _ = consumer;
}

#[tokio::test]
async fn test_telemetry_consumer_read_empty_stream() {
    let (redis_url, _container) = setup_redis().await;

    let consumer = TelemetryConsumer::new(&redis_url)
        .await
        .expect("Failed to create consumer");

    let records = consumer
        .read_single_batch()
        .await
        .expect("Failed to read batch");

    assert_eq!(records.len(), 0, "Should have no records in empty stream");
}

#[tokio::test]
async fn test_telemetry_consumer_read_records() {
    let (redis_url, _container) = setup_redis().await;

    // Push some test data to the stream
    let client = redis::Client::open(redis_url.as_str()).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");

    // Add test records to stream
    let _: String = redis::cmd("XADD")
        .arg("hyperinfer:telemetry")
        .arg("*")
        .arg("key")
        .arg("test-key-1")
        .arg("model")
        .arg("gpt-4")
        .arg("input_tokens")
        .arg("100")
        .arg("output_tokens")
        .arg("50")
        .arg("response_time_ms")
        .arg("250")
        .arg("timestamp")
        .arg("1700000000000")
        .query_async(&mut conn)
        .await
        .expect("Failed to add to stream");

    let _: String = redis::cmd("XADD")
        .arg("hyperinfer:telemetry")
        .arg("*")
        .arg("key")
        .arg("test-key-2")
        .arg("model")
        .arg("claude-3")
        .arg("input_tokens")
        .arg("200")
        .arg("output_tokens")
        .arg("100")
        .arg("response_time_ms")
        .arg("300")
        .arg("timestamp")
        .arg("1700000001000")
        .query_async(&mut conn)
        .await
        .expect("Failed to add to stream");

    // Read records
    let consumer = TelemetryConsumer::new(&redis_url)
        .await
        .expect("Failed to create consumer");

    let records = consumer
        .read_single_batch()
        .await
        .expect("Failed to read batch");

    assert_eq!(records.len(), 2, "Should have 2 records");
    assert_eq!(records[0].key, "test-key-1");
    assert_eq!(records[0].model, "gpt-4");
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
    assert_eq!(records[0].response_time_ms, 250);
    assert_eq!(records[0].timestamp, 1700000000000);

    assert_eq!(records[1].key, "test-key-2");
    assert_eq!(records[1].model, "claude-3");
    assert_eq!(records[1].input_tokens, 200);
    assert_eq!(records[1].output_tokens, 100);
    assert_eq!(records[1].response_time_ms, 300);
    assert_eq!(records[1].timestamp, 1700000001000);
}

#[tokio::test]
async fn test_telemetry_consumer_with_custom_stream() {
    let (redis_url, _container) = setup_redis().await;

    let client = redis::Client::open(redis_url.as_str()).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");

    // Add test record to custom stream
    let _: String = redis::cmd("XADD")
        .arg("custom:telemetry")
        .arg("*")
        .arg("key")
        .arg("test-key")
        .arg("model")
        .arg("gpt-4")
        .arg("input_tokens")
        .arg("150")
        .arg("output_tokens")
        .arg("75")
        .arg("response_time_ms")
        .arg("200")
        .arg("timestamp")
        .arg("1700000000000")
        .query_async(&mut conn)
        .await
        .expect("Failed to add to stream");

    let consumer = TelemetryConsumer::new(&redis_url)
        .await
        .expect("Failed to create consumer")
        .with_stream_key("custom:telemetry");

    let records = consumer
        .read_single_batch()
        .await
        .expect("Failed to read batch");

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].key, "test-key");
    assert_eq!(records[0].model, "gpt-4");
}

#[tokio::test]
async fn test_telemetry_consumer_start_consuming() {
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    let (redis_url, _container) = setup_redis().await;

    let client = redis::Client::open(redis_url.as_str()).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received);

    let consumer = TelemetryConsumer::new(&redis_url)
        .await
        .expect("Failed to create consumer");

    let cancellation_token = CancellationToken::new();
    let _handle = consumer
        .start_consuming(
            move |record: UsageRecord| {
                let received = Arc::clone(&received_clone);
                async move {
                    received.lock().await.push(record);
                    Ok(())
                }
            },
            cancellation_token.clone(),
        )
        .await
        .expect("Failed to start consuming");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let _: String = redis::cmd("XADD")
        .arg("hyperinfer:telemetry")
        .arg("*")
        .arg("key")
        .arg("consume-test-key")
        .arg("model")
        .arg("gpt-4")
        .arg("input_tokens")
        .arg("100")
        .arg("output_tokens")
        .arg("50")
        .arg("response_time_ms")
        .arg("250")
        .arg("timestamp")
        .arg("1700000000000")
        .query_async(&mut conn)
        .await
        .expect("Failed to add to stream");

    let timeout = tokio::time::timeout(tokio::time::Duration::from_secs(5), async {
        loop {
            let len = received.lock().await.len();
            if len >= 1 {
                return;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    })
    .await;

    assert!(timeout.is_ok(), "Timeout waiting for consumer to process");

    let records = received.lock().await;
    assert_eq!(records.len(), 1, "Should have consumed 1 record");
    assert_eq!(records[0].key, "consume-test-key");
    assert_eq!(records[0].model, "gpt-4");
}

#[tokio::test]
async fn test_telemetry_consumer_handles_malformed_data() {
    let (redis_url, _container) = setup_redis().await;

    let client = redis::Client::open(redis_url.as_str()).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");

    // Add malformed record (missing fields)
    let _: String = redis::cmd("XADD")
        .arg("hyperinfer:telemetry")
        .arg("*")
        .arg("key")
        .arg("test-key")
        .arg("model")
        .arg("gpt-4")
        // Missing tokens and other fields
        .query_async(&mut conn)
        .await
        .expect("Failed to add to stream");

    // Add valid record
    let _: String = redis::cmd("XADD")
        .arg("hyperinfer:telemetry")
        .arg("*")
        .arg("key")
        .arg("valid-key")
        .arg("model")
        .arg("gpt-4")
        .arg("input_tokens")
        .arg("100")
        .arg("output_tokens")
        .arg("50")
        .arg("response_time_ms")
        .arg("250")
        .arg("timestamp")
        .arg("1700000000000")
        .query_async(&mut conn)
        .await
        .expect("Failed to add to stream");

    let consumer = TelemetryConsumer::new(&redis_url)
        .await
        .expect("Failed to create consumer");

    let records = consumer
        .read_single_batch()
        .await
        .expect("Failed to read batch");

    // Should only get the valid record
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].key, "valid-key");
}

#[tokio::test]
async fn test_telemetry_consumer_large_batch() {
    let (redis_url, _container) = setup_redis().await;

    let client = redis::Client::open(redis_url.as_str()).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");

    // Add 50 records
    for i in 0..50 {
        let _: String = redis::cmd("XADD")
            .arg("hyperinfer:telemetry")
            .arg("*")
            .arg("key")
            .arg(format!("test-key-{}", i))
            .arg("model")
            .arg("gpt-4")
            .arg("input_tokens")
            .arg("100")
            .arg("output_tokens")
            .arg("50")
            .arg("response_time_ms")
            .arg("250")
            .arg("timestamp")
            .arg("1700000000000")
            .query_async(&mut conn)
            .await
            .expect("Failed to add to stream");
    }

    let consumer = TelemetryConsumer::new(&redis_url)
        .await
        .expect("Failed to create consumer");

    let records = consumer
        .read_single_batch()
        .await
        .expect("Failed to read batch");

    assert_eq!(records.len(), 50, "Should read all 50 records");
}

#[tokio::test]
async fn test_telemetry_consumer_zero_tokens() {
    let (redis_url, _container) = setup_redis().await;

    let client = redis::Client::open(redis_url.as_str()).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");

    let _: String = redis::cmd("XADD")
        .arg("hyperinfer:telemetry")
        .arg("*")
        .arg("key")
        .arg("test-key")
        .arg("model")
        .arg("gpt-4")
        .arg("input_tokens")
        .arg("0")
        .arg("output_tokens")
        .arg("0")
        .arg("response_time_ms")
        .arg("0")
        .arg("timestamp")
        .arg("0")
        .query_async(&mut conn)
        .await
        .expect("Failed to add to stream");

    let consumer = TelemetryConsumer::new(&redis_url)
        .await
        .expect("Failed to create consumer");

    let records = consumer
        .read_single_batch()
        .await
        .expect("Failed to read batch");

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input_tokens, 0);
    assert_eq!(records[0].output_tokens, 0);
    assert_eq!(records[0].response_time_ms, 0);
}
