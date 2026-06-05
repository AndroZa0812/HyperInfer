use hyperinfer_router::{RedisConfig, RedisRoutingState, RoutingState};
use std::time::Duration;
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

fn test_config() -> RedisConfig {
    RedisConfig {
        alpha: 0.3,
        allowed_fails: 3,
        cooldown_secs: 30,
        tpm_ttl_secs: 60,
        latency_ttl_secs: 600,
        failures_ttl_secs: 300,
    }
}

#[tokio::test]
async fn test_new_state_has_zero_metrics() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    let metrics = state.get_metrics("deploy-new").await.unwrap();
    assert_eq!(metrics.latency_ewma_ms, 0.0);
    assert_eq!(metrics.in_flight, 0);
    assert_eq!(metrics.tpm_used, 0);
    assert_eq!(metrics.rpm_used, 0);
    assert_eq!(metrics.total_requests, 0);
    assert_eq!(metrics.total_failures, 0);
}

#[tokio::test]
async fn test_request_start_increments_in_flight_and_rpm() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    state.record_request_start("deploy-start").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let metrics = state.get_metrics("deploy-start").await.unwrap();
    assert_eq!(metrics.in_flight, 1);
    assert_eq!(metrics.rpm_used, 1);
}

#[tokio::test]
async fn test_success_decrements_in_flight_and_updates_latency() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    state.record_request_start("deploy-success").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    state
        .record_request_success("deploy-success", 150.0, 500)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let metrics = state.get_metrics("deploy-success").await.unwrap();
    assert_eq!(metrics.in_flight, 0);
    assert!((metrics.latency_ewma_ms - 150.0).abs() < 0.01);
    assert_eq!(metrics.tpm_used, 500);
    assert_eq!(metrics.total_requests, 1);
}

#[tokio::test]
async fn test_ewma_convergence() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    state.record_request_start("deploy-ewma").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    state
        .record_request_success("deploy-ewma", 100.0, 0)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let metrics = state.get_metrics("deploy-ewma").await.unwrap();
    assert!(
        (metrics.latency_ewma_ms - 100.0).abs() < 0.01,
        "First sample should be used directly"
    );

    state.record_request_start("deploy-ewma").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    state
        .record_request_success("deploy-ewma", 200.0, 0)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let metrics = state.get_metrics("deploy-ewma").await.unwrap();
    let expected = 0.3 * 200.0 + 0.7 * 100.0;
    assert!(
        (metrics.latency_ewma_ms - expected).abs() < 0.01,
        "EWMA should be {} but got {}",
        expected,
        metrics.latency_ewma_ms
    );
}

#[tokio::test]
async fn test_failure_increments_counter() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    state.record_request_start("deploy-fail").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = state.record_request_failure("deploy-fail").await.unwrap();
    assert_eq!(result.failure_count, 1);
    assert!(!result.cooldown_triggered);

    let metrics = state.get_metrics("deploy-fail").await.unwrap();
    assert_eq!(metrics.in_flight, 0);
    assert_eq!(metrics.total_failures, 1);
}

#[tokio::test]
async fn test_cooldown_triggered_after_allowed_fails() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    for _ in 0..2 {
        state.record_request_start("deploy-cooldown").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let result = state
            .record_request_failure("deploy-cooldown")
            .await
            .unwrap();
        assert!(!result.cooldown_triggered);
    }

    state.record_request_start("deploy-cooldown").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = state
        .record_request_failure("deploy-cooldown")
        .await
        .unwrap();
    assert_eq!(result.failure_count, 3);
    assert!(result.cooldown_triggered);

    let is_cooled = state.is_cooled_down("deploy-cooldown").await.unwrap();
    assert!(is_cooled, "Deployment should be in cooldown");
}

#[tokio::test]
async fn test_success_resets_failure_counter() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    for _ in 0..2 {
        state.record_request_start("deploy-reset").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        state.record_request_failure("deploy-reset").await.unwrap();
    }

    state.record_request_start("deploy-reset").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    state
        .record_request_success("deploy-reset", 100.0, 0)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    state.record_request_start("deploy-reset").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = state.record_request_failure("deploy-reset").await.unwrap();
    assert_eq!(
        result.failure_count, 1,
        "Failure counter should reset to 1 after success"
    );
    assert!(!result.cooldown_triggered);
}

#[tokio::test]
async fn test_get_all_metrics_batch() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    state.record_request_start("batch-a").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    state
        .record_request_success("batch-a", 100.0, 200)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    state.record_request_start("batch-b").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    state
        .record_request_success("batch-b", 200.0, 400)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let ids = vec!["batch-a", "batch-b", "batch-missing"];
    let all_metrics = state.get_all_metrics(&ids).await.unwrap();

    assert_eq!(all_metrics.len(), 3);

    let a = all_metrics.get("batch-a").unwrap();
    assert!((a.latency_ewma_ms - 100.0).abs() < 0.01);
    assert_eq!(a.tpm_used, 200);
    assert_eq!(a.total_requests, 1);

    let b = all_metrics.get("batch-b").unwrap();
    assert!((b.latency_ewma_ms - 200.0).abs() < 0.01);
    assert_eq!(b.tpm_used, 400);
    assert_eq!(b.total_requests, 1);

    let missing = all_metrics.get("batch-missing").unwrap();
    assert_eq!(missing.latency_ewma_ms, 0.0);
    assert_eq!(missing.in_flight, 0);
    assert_eq!(missing.total_requests, 0);
}

#[tokio::test]
async fn test_concurrent_in_flight_accuracy() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, test_config())
        .await
        .unwrap();

    let mut handles = Vec::new();
    for _ in 0..50 {
        let s = state.clone();
        handles.push(tokio::spawn(async move {
            s.record_request_start("deploy-concurrent").await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
            s.record_request_success("deploy-concurrent", 50.0, 10)
                .await
                .unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    let metrics = state.get_metrics("deploy-concurrent").await.unwrap();
    assert_eq!(
        metrics.in_flight, 0,
        "All in-flight requests should be decremented"
    );
    assert_eq!(metrics.total_requests, 50, "All 50 requests should succeed");
    assert_eq!(
        metrics.tpm_used, 500,
        "Total tokens should be 50 * 10 = 500"
    );
}
