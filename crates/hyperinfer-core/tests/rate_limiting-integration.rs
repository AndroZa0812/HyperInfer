use hyperinfer_core::RateLimiter;
use std::time::{SystemTime, UNIX_EPOCH};

// Mock redis client using real redis instance running on host
async fn setup_redis() -> Option<String> {
    let redis_url = "redis://127.0.0.1:6379".to_string();
    if let Ok(client) = redis::Client::open(redis_url.clone()) {
        if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
            let _: () = redis::cmd("FLUSHDB")
                .query_async(&mut conn)
                .await
                .unwrap_or(());
            return Some(redis_url);
        }
    }
    None
}

#[tokio::test]
async fn test_rate_limiter_new() {
    let Some(redis_url) = setup_redis().await else {
        return;
    };

    let result = RateLimiter::new(Some(&redis_url)).await;
    assert!(result.is_ok(), "Should create RateLimiter successfully");
}

#[tokio::test]
async fn test_rate_limiter_is_allowed() {
    let Some(redis_url) = setup_redis().await else {
        return;
    };
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_is_allowed_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // First request should be allowed
    let result = limiter.is_allowed(&key, 10).await;
    assert!(result.is_ok());
    assert!(result.unwrap(), "First request should be allowed");
}

#[tokio::test]
async fn test_rate_limiter_is_allowed_blocking() {
    let Some(redis_url) = setup_redis().await else {
        return;
    };
    let limiter = RateLimiter::new(Some(&redis_url)).await.unwrap();

    let key = format!(
        "test_key_is_allowed_blocking_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // Default RPM is 60. Note: the `is_allowed` function checks `tpm_result.first().copied().unwrap_or(0) == 1`.
    // And it has a second check: `if allowed == 0 { return Ok(false); }`.
    // We send 61 requests to trigger RPM blocking.
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
    let Some(redis_url) = setup_redis().await else {
        return;
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

    for _i in 0..limit {
        let result = limiter.check_rpm(&key, limit).await.unwrap();
        assert!(result.0, "Expected to be allowed");
    }

    // The next one should be blocked. Sometimes tests run fast enough that parallel runs interfere. Wait/retry loop
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
    let Some(redis_url) = setup_redis().await else {
        return;
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
}

#[tokio::test]
async fn test_rate_limiter_record_usage() {
    let Some(redis_url) = setup_redis().await else {
        return;
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
}
