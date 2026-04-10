use hyperinfer_core::{RateLimiter, USAGE_REQUESTS_KEY_PREFIX, USAGE_TOKENS_KEY_PREFIX};
use std::time::{SystemTime, UNIX_EPOCH};
use testcontainers::{core::IntoContainerPort, runners::AsyncRunner, GenericImage};
use testcontainers_modules::redis::REDIS_PORT;

async fn setup_redis() -> (String, testcontainers::ContainerAsync<GenericImage>) {
    let redis = GenericImage::new("redis", "7.2")
        .with_exposed_port(REDIS_PORT.tcp())
        .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
            "Ready to accept connections",
        ))
        .start()
        .await
        .expect("Failed to start Redis container");
    let port = redis.get_host_port_ipv4(REDIS_PORT).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", port);
    (redis_url, redis)
}

#[tokio::test]
async fn test_rate_limiter_new() {
    let (redis_url, _container) = setup_redis().await;

    let result = RateLimiter::new(Some(&redis_url)).await;
    assert!(result.is_ok(), "Should create RateLimiter successfully");
}

#[tokio::test]
async fn test_rate_limiter_is_allowed() {
    let (redis_url, _container) = setup_redis().await;
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_is_allowed_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let result = limiter.is_allowed(&key, 10).await;
    assert!(result.is_ok());
    assert!(result.unwrap(), "First request should be allowed");
}

#[tokio::test]
async fn test_rate_limiter_is_allowed_blocking() {
    let (redis_url, _container) = setup_redis().await;
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_is_allowed_blocking_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let mut blocked = false;
    for _i in 0..65 {
        let result = limiter.is_allowed(&key, 10).await.unwrap();
        if !result {
            blocked = true;
            break;
        }
    }

    assert!(blocked, "Expected to be blocked due to RPM limit");
}

#[tokio::test]
async fn test_rate_limiter_check_rpm() {
    let (redis_url, _container) = setup_redis().await;
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_check_rpm_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let limit = 5;

    for _i in 0..limit {
        let result = limiter.check_rpm(&key, limit).await.unwrap();
        assert!(result.0, "Expected to be allowed");
    }

    let mut blocked = false;
    for _i in 0..15 {
        let result = limiter.check_rpm(&key, limit).await.unwrap();
        if !result.0 {
            blocked = true;
            break;
        }
    }
    assert!(blocked, "Expected to be blocked");
}

#[tokio::test]
async fn test_rate_limiter_check_tpm() {
    let (redis_url, _container) = setup_redis().await;
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_check_tpm_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let limit = 10000;

    let result = limiter.check_tpm(&key, limit, 50).await.unwrap();
    assert!(result, "Should allow 50 tokens when limit is 10000");

    let result = limiter.check_tpm(&key, limit, 15000).await.unwrap();
    assert!(!result, "Should deny when tokens exceed limit");
}

#[tokio::test]
async fn test_rate_limiter_record_usage() {
    let (redis_url, _container) = setup_redis().await;
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_record_usage_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let result = limiter.record_usage(&key, 150).await;
    assert!(result.is_ok(), "Should record usage successfully");

    let result = limiter.record_usage(&key, 50).await;
    assert!(result.is_ok(), "Should record usage successfully");

    let client = redis::Client::open(redis_url.as_str()).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");

    let tokens_used: u64 = redis::cmd("GET")
        .arg(format!("{}{}", USAGE_TOKENS_KEY_PREFIX, key))
        .query_async(&mut conn)
        .await
        .unwrap();

    assert_eq!(
        tokens_used, 200,
        "Tokens used should be sum of recorded amounts"
    );

    let requests_made: u64 = redis::cmd("GET")
        .arg(format!("{}{}", USAGE_REQUESTS_KEY_PREFIX, key))
        .query_async(&mut conn)
        .await
        .unwrap();

    assert_eq!(
        requests_made, 2,
        "Requests made should reflect number of calls"
    );
}
