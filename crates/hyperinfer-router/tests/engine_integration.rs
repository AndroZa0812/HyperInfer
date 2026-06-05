use hyperinfer_core::Provider;
use hyperinfer_router::{
    Deployment, GlobalLimits, RedisConfig, RedisRoutingState, RouterEngine, RoutingContext,
    RoutingState, UsageBased, WeightedShuffle,
};
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
    (format!("redis://127.0.0.1:{}", port), redis)
}

fn make_deployment(model: &str, id: &str, weight: u32) -> Deployment {
    let mut d = Deployment::new(
        model.to_string(),
        Provider::OpenAI,
        model.to_string(),
        format!("key-{}", id),
    );
    d.id = id.to_string();
    d.weight = weight;
    d
}

#[tokio::test]
async fn test_full_routing_flow_with_redis() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, RedisConfig::default())
        .await
        .unwrap();

    let engine = RouterEngine::new(GlobalLimits::default());
    engine
        .register_strategy(Box::new(WeightedShuffle::new()))
        .await;

    let dep_a = make_deployment("gpt-4", "dep-a", 3);
    let dep_b = make_deployment("gpt-4", "dep-b", 1);
    engine.add_deployment(dep_a).await;
    engine.add_deployment(dep_b).await;

    let ctx = RoutingContext::default();
    let result = engine
        .select_deployment("gpt-4", &state, &ctx)
        .await
        .unwrap();

    assert!(
        result.deployment.id == "dep-a" || result.deployment.id == "dep-b",
        "Expected one of dep-a or dep-b, got {}",
        result.deployment.id
    );
    assert_eq!(result.attempt, 1);
}

#[tokio::test]
async fn test_metrics_affect_routing() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, RedisConfig::default())
        .await
        .unwrap();

    let engine = RouterEngine::new(GlobalLimits::default());
    engine.register_strategy(Box::new(UsageBased::new())).await;
    engine.set_default_strategy("usage-based").await;

    let mut dep_a = make_deployment("gpt-4", "dep-a", 1);
    dep_a.tpm_limit = Some(10000);
    let mut dep_b = make_deployment("gpt-4", "dep-b", 1);
    dep_b.tpm_limit = Some(10000);
    engine.add_deployment(dep_a).await;
    engine.add_deployment(dep_b).await;

    state.record_request_start("dep-a").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    state
        .record_request_success("dep-a", 100.0, 9000)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let ctx = RoutingContext::default();
    let result = engine
        .select_deployment("gpt-4", &state, &ctx)
        .await
        .unwrap();

    assert_eq!(
        result.deployment.id, "dep-b",
        "dep_b should be selected since dep_a has high utilization"
    );
}

#[tokio::test]
async fn test_cooldown_removes_from_pool() {
    let (redis_url, _container) = setup_redis().await;
    let config = RedisConfig {
        allowed_fails: 2,
        cooldown_secs: 10,
        ..RedisConfig::default()
    };
    let state = RedisRoutingState::new(&redis_url, config).await.unwrap();

    let engine = RouterEngine::new(GlobalLimits::default());
    engine
        .register_strategy(Box::new(WeightedShuffle::new()))
        .await;

    let dep_a = make_deployment("gpt-4", "dep-a", 1);
    let dep_b = make_deployment("gpt-4", "dep-b", 1);
    engine.add_deployment(dep_a).await;
    engine.add_deployment(dep_b).await;

    for _ in 0..2 {
        state.record_request_start("dep-a").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        state.record_request_failure("dep-a").await.unwrap();
    }

    let is_cooled = state.is_cooled_down("dep-a").await.unwrap();
    assert!(is_cooled, "dep-a should be in cooldown");

    let ctx = RoutingContext::default();
    let result = engine
        .select_deployment("gpt-4", &state, &ctx)
        .await
        .unwrap();

    assert_eq!(
        result.deployment.id, "dep-b",
        "dep_b should be selected since dep_a is in cooldown"
    );
}

#[tokio::test]
async fn test_concurrent_routing_decisions() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, RedisConfig::default())
        .await
        .unwrap();

    let engine = std::sync::Arc::new(RouterEngine::new(GlobalLimits::default()));
    engine
        .register_strategy(Box::new(WeightedShuffle::new()))
        .await;

    for i in 0..5 {
        let dep = make_deployment("gpt-4", &format!("dep-{}", i), 1);
        engine.add_deployment(dep).await;
    }

    let mut handles = Vec::new();
    for _ in 0..100 {
        let engine = engine.clone();
        let state = state.clone();
        handles.push(tokio::spawn(async move {
            let ctx = RoutingContext::default();
            engine
                .select_deployment("gpt-4", &state, &ctx)
                .await
                .unwrap()
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }

    assert_eq!(
        results.len(),
        100,
        "All 100 routing decisions should succeed"
    );
    for r in &results {
        assert!(
            r.deployment.id.starts_with("dep-"),
            "Unexpected deployment id: {}",
            r.deployment.id
        );
    }
}

#[tokio::test]
async fn test_engine_passthrough_with_redis() {
    let (redis_url, _container) = setup_redis().await;
    let state = RedisRoutingState::new(&redis_url, RedisConfig::default())
        .await
        .unwrap();

    let engine = RouterEngine::new(GlobalLimits::default());
    engine
        .register_strategy(Box::new(WeightedShuffle::new()))
        .await;

    let dep = make_deployment("gpt-4", "dep-passthrough", 1);
    engine.add_deployment(dep).await;

    let ctx = RoutingContext::default();
    let result = engine
        .select_deployment("gpt-4", &state, &ctx)
        .await
        .unwrap();
    assert_eq!(result.deployment.id, "dep-passthrough");

    state.record_request_start("dep-passthrough").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    engine
        .record_success("dep-passthrough", 200.0, 300, &state)
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let metrics = state.get_metrics("dep-passthrough").await.unwrap();
    assert_eq!(metrics.total_requests, 1);
    assert!((metrics.latency_ewma_ms - 200.0).abs() < 1.0);
    assert_eq!(metrics.tpm_used, 300);
}
