use hyperinfer_core::RateLimiter;
use std::time::{SystemTime, UNIX_EPOCH};

async fn setup_redis() -> Option<String> {
    let url = "redis://127.0.0.1:6379";
    let client = redis::Client::open(url).ok()?;
    client.get_multiplexed_async_connection().await.ok()?;
    Some(url.to_string())
}

async fn cleanup_key(redis_url: &str, key: &str) {
    let client = redis::Client::open(redis_url).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");
    let _: () = redis::cmd("DEL")
        .arg(&[
            format!("hyperinfer:rate:rpm:{}", key),
            format!("hyperinfer:usage:tokens:{}", key),
            format!("hyperinfer:usage:requests:{}", key),
        ])
        .query_async(&mut conn)
        .await
        .unwrap_or(());
}

#[tokio::test]
async fn test_rate_limiter_new() {
    let redis_url = match setup_redis().await {
        Some(url) => url,
        None => return,
    };

    let result = RateLimiter::new(Some(&redis_url)).await;
    assert!(result.is_ok(), "Should create RateLimiter successfully");
}

#[tokio::test]
async fn test_rate_limiter_is_allowed() {
    let redis_url = match setup_redis().await {
        Some(url) => url,
        None => return,
    };
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

    cleanup_key(&redis_url, &key).await;
}

#[tokio::test]
async fn test_rate_limiter_is_allowed_blocking() {
    let redis_url = match setup_redis().await {
        Some(url) => url,
        None => return,
    };
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_is_allowed_blocking_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // Default RPM is 60.
    for _i in 0..60 {
        let result = limiter.is_allowed(&key, 10).await.unwrap();
        assert!(result, "Expected request to be allowed within rate limits");
    }

    // 61st request should be blocked.
    let result = limiter.is_allowed(&key, 10).await.unwrap();
    assert!(!result, "Expected to be blocked due to RPM limit");

    cleanup_key(&redis_url, &key).await;
}

#[tokio::test]
async fn test_rate_limiter_check_rpm() {
    let redis_url = match setup_redis().await {
        Some(url) => url,
        None => return,
    };
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_check_rpm_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let limit = 5;

    for i in 0..limit {
        let result = limiter.check_rpm(&key, limit).await.unwrap();
        assert!(result.0, "Expected to be allowed");
        assert_eq!(result.1, limit - i - 1, "Remaining count should decrease");
    }

    // The next one should be blocked
    let result = limiter.check_rpm(&key, limit).await.unwrap();
    assert!(!result.0, "Expected to be blocked");
    assert_eq!(result.1, 0, "Remaining count should be 0");

    cleanup_key(&redis_url, &key).await;
}

#[tokio::test]
async fn test_rate_limiter_check_tpm() {
    let redis_url = match setup_redis().await {
        Some(url) => url,
        None => return,
    };
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_check_tpm_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let limit = 10000;

    // Check with 50 tokens
    let result = limiter.check_tpm(&key, limit, 50).await.unwrap();
    assert!(result, "Should allow 50 tokens when limit is 10000");

    cleanup_key(&redis_url, &key).await;
}

#[tokio::test]
async fn test_rate_limiter_record_usage() {
    let redis_url = match setup_redis().await {
        Some(url) => url,
        None => return,
    };
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_record_usage_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // Record usage
    let result = limiter.record_usage(&key, 150).await;
    assert!(result.is_ok(), "Should record usage successfully");

    let result = limiter.record_usage(&key, 50).await;
    assert!(result.is_ok(), "Should record usage successfully");

    // Query Redis directly to verify
    let client = redis::Client::open(redis_url.as_str()).expect("Failed to create client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect");

    let tokens_used: u64 = redis::cmd("GET")
        .arg(format!("hyperinfer:usage:tokens:{}", key))
        .query_async(&mut conn)
        .await
        .unwrap();

    assert_eq!(
        tokens_used, 200,
        "Tokens used should be sum of recorded amounts"
    );

    let requests_made: u64 = redis::cmd("GET")
        .arg(format!("hyperinfer:usage:requests:{}", key))
        .query_async(&mut conn)
        .await
        .unwrap();

    assert_eq!(
        requests_made, 2,
        "Requests made should reflect number of calls"
    );

    cleanup_key(&redis_url, &key).await;
}
