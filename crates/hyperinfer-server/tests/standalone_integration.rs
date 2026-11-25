//! Standalone mode integration tests (no Docker/Postgres/Redis required).

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use axum_test::TestServer;
use hyperinfer_core::hash_key;
use hyperinfer_core::{Config, ConfigStore, CreateDeploymentRequest, Database, DbError};
use hyperinfer_server::routes::build_router;
use hyperinfer_server::standalone::{
    auto_detect_providers_from_env, bootstrap_standalone, detected_to_deployments,
    InMemoryConfigStore, InMemoryDb,
};
use tokio::sync::RwLock;

#[tokio::test]
async fn test_in_memory_db_crud_and_constraints() {
    let db = InMemoryDb::new();

    let team = db.create_team("Test Team", 1000).await.unwrap();
    assert!(!team.id.is_empty());
    let err = db.create_team("Test Team", 2000).await.unwrap_err();
    assert!(matches!(err, DbError::UniqueViolation(_)));

    let user = db
        .create_user(&team.id, "alice@example.com", "admin", None)
        .await
        .unwrap();
    assert_eq!(user.email, "alice@example.com");
    let err = db
        .create_user(&team.id, "alice@example.com", "member", None)
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::UniqueViolation(_)));

    let key_hash = hash_key("test-api-key");
    let api_key = db
        .create_api_key(&key_hash, &user.id, &team.id, Some("test".to_string()))
        .await
        .unwrap();
    let found = db.get_api_key_by_hash(&key_hash).await.unwrap().unwrap();
    assert_eq!(found.id, api_key.id);
    let deactivated = db.deactivate_api_key(&api_key.id).await.unwrap();
    assert!(!deactivated.is_active);

    let dep_req = CreateDeploymentRequest {
        name: "test-dep".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o-mini".to_string(),
        api_key_ref: Some("test-key".to_string()),
        base_url: "https://api.openai.com".to_string(),
        is_active: true,
        weight: 1,
        priority: 0,
        max_tpm: None,
        max_rpm: None,
        cost_per_1k_input_tokens: None,
        cost_per_1k_output_tokens: None,
        metadata: None,
        sort_order: None,
    };
    let dep = db.create_deployment(dep_req.clone()).await.unwrap();
    assert_eq!(db.list_deployments("", None).await.unwrap().len(), 1);
    db.update_deployment(&dep.id, dep_req.clone())
        .await
        .unwrap();
    db.delete_deployment(&dep.id).await.unwrap();
    assert_eq!(db.list_deployments("", None).await.unwrap().len(), 0);

    let cfg_req = hyperinfer_core::UpdateRoutingConfigRequest {
        strategy: Some("round-robin".to_string()),
        strategy_params: None,
        fallback_config: None,
        routing_groups: None,
    };
    let cfg = db.update_routing_config(cfg_req).await.unwrap();
    assert_eq!(cfg.strategy, "round-robin");
    assert_eq!(
        db.get_routing_config().await.unwrap().unwrap().strategy,
        "round-robin"
    );

    db.record_usage(&team.id, &api_key.id, "gpt-4o-mini", 10, 20, 100)
        .await
        .unwrap();
    let summary = db.get_usage_summary(Some(team.id)).await.unwrap();
    assert_eq!(summary.total_requests, 1);
    assert_eq!(summary.total_input_tokens, 10);
    assert_eq!(summary.total_output_tokens, 20);
}

#[tokio::test]
async fn test_in_memory_config_store_hot_reload() {
    let config = Arc::new(RwLock::new(Config {
        api_keys: HashMap::new(),
        routing_rules: Vec::new(),
        quotas: HashMap::new(),
        model_aliases: HashMap::new(),
        default_provider: None,
    }));
    let store = InMemoryConfigStore::new(config.clone());

    let initial = store.fetch_config().await.unwrap();
    assert!(initial.api_keys.is_empty());

    let mut updated = initial.clone();
    updated
        .api_keys
        .insert("key1".to_string(), "value1".to_string());
    store.publish_config_update(&updated).await.unwrap();

    let fetched = store.fetch_config().await.unwrap();
    assert_eq!(fetched.api_keys.get("key1"), Some(&"value1".to_string()));
    assert!(store.ping().await.is_ok());
}

#[tokio::test]
async fn test_auto_detect_providers_from_env() {
    let mut env = HashMap::new();
    env.insert("OPENAI_API_KEY".to_string(), "sk-test".to_string());
    env.insert("ANTHROPIC_API_KEY".to_string(), "sk-ant-test".to_string());
    env.insert("GEMINI_API_KEY".to_string(), "gemini-test".to_string());

    let detected = auto_detect_providers_from_env(&env);
    assert_eq!(detected.len(), 3);
    assert_eq!(detected[0].name, "openai");
    assert_eq!(detected[1].name, "anthropic");
    assert_eq!(detected[2].name, "gemini");

    let deployments = detected_to_deployments(&detected);
    assert!(!deployments.is_empty());
    assert!(deployments.iter().all(|d| d.is_active));
    assert!(deployments
        .iter()
        .any(|d| d.api_key_ref == Some("$OPENAI_API_KEY".to_string())
            && d.base_url == "https://api.openai.com"));
    assert!(deployments
        .iter()
        .any(|d| d.provider == "anthropic" && d.base_url == "https://api.anthropic.com"));
    assert!(deployments.iter().any(
        |d| d.provider == "gemini" && d.base_url == "https://generativelanguage.googleapis.com"
    ));

    assert!(auto_detect_providers_from_env(&HashMap::new()).is_empty());
}

#[tokio::test]
async fn test_standalone_healthz_deployments_and_dashboard_fallback() {
    let boot = bootstrap_standalone(false).await.unwrap();
    let cors = tower_http::cors::CorsLayer::permissive();
    let app = build_router(
        boot.state,
        hyperinfer_server::mcp::McpState::new(boot.jwt_secret.clone()),
        cors,
        false,
    );
    let server = TestServer::new(app);

    let health_res = server.get("/healthz").await;
    assert_eq!(health_res.status_code(), StatusCode::OK);
    let health_json: serde_json::Value = health_res.json::<serde_json::Value>();
    assert_eq!(health_json["status"], "ok");
    assert_eq!(health_json["database"], "ok");
    assert_eq!(health_json["redis"], "ok");

    let login_body =
        serde_json::json!({ "email": boot.admin_email, "password": boot.admin_password });
    let login_res = server.post("/v1/auth/login").json(&login_body).await;
    assert_eq!(login_res.status_code(), StatusCode::OK);
    let set_cookie = login_res
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie missing")
        .to_str()
        .unwrap();
    let token = set_cookie
        .split(";")
        .next()
        .unwrap()
        .split("=")
        .nth(1)
        .unwrap()
        .trim();
    assert!(!token.is_empty());

    let dep_res = server
        .get("/v1/deployments?model=")
        .add_header("Cookie", format!("auth_token={token}"))
        .await;
    assert_eq!(dep_res.status_code(), StatusCode::OK);

    let spa_res = server.get("/").await;
    assert!(
        spa_res.status_code() == StatusCode::OK || spa_res.status_code() == StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_standalone_proxy_chat_completions_to_local_mock() {
    async fn mock_chat_completions() -> impl IntoResponse {
        (
            StatusCode::OK,
            [("Content-Type", "application/json")],
            serde_json::json!({
                "id": "chatcmpl-test", "object": "chat.completion", "created": 1700000000,
                "model": "mock-gpt",
                "choices": [{ "index": 0, "message": { "role": "assistant", "content": "hello from standalone mock" }, "finish_reason": "stop" }],
                "usage": { "prompt_tokens": 5, "completion_tokens": 4, "total_tokens": 9 }
            }).to_string(),
        )
    }

    let mock_app = Router::new().route("/v1/chat/completions", post(mock_chat_completions));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let boot = bootstrap_standalone(false).await.unwrap();
    let dep_req = CreateDeploymentRequest {
        name: "mock-gpt".to_string(),
        provider: "openai".to_string(),
        model: "mock-gpt".to_string(),
        api_key_ref: Some("test-key".to_string()),
        base_url: format!("http://{mock_addr}/v1"),
        is_active: true,
        weight: 1,
        priority: 0,
        max_tpm: None,
        max_rpm: None,
        cost_per_1k_input_tokens: None,
        cost_per_1k_output_tokens: None,
        metadata: None,
        sort_order: None,
    };
    boot.state.db.create_deployment(dep_req).await.unwrap();

    let cors = tower_http::cors::CorsLayer::permissive();
    let app = build_router(
        boot.state,
        hyperinfer_server::mcp::McpState::new(boot.jwt_secret.clone()),
        cors,
        false,
    );
    let server = TestServer::new(app);

    let chat_body = serde_json::json!({ "model": "mock-gpt", "messages": [{ "role": "user", "content": "hello" }] });
    let chat_res = server
        .post("/v1/chat/completions")
        .add_header("Authorization", format!("Bearer {}", boot.api_key))
        .json(&chat_body)
        .await;
    assert_eq!(chat_res.status_code(), StatusCode::OK);
    let chat_json: serde_json::Value = chat_res.json::<serde_json::Value>();
    let content = chat_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap();
    assert_eq!(content, "hello from standalone mock");
}

#[tokio::test]
async fn test_probe_ollama_local_mock() {
    async fn mock_ollama_tags() -> impl IntoResponse {
        (
            StatusCode::OK,
            [("Content-Type", "application/json")],
            serde_json::json!({
                "models": [
                    { "name": "qwen2.5-coder:latest" },
                    { "name": "llama3.2:latest" }
                ]
            })
            .to_string(),
        )
    }

    let app = Router::new().route("/api/tags", axum::routing::get(mock_ollama_tags));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let detected = hyperinfer_server::standalone::probe_ollama(&format!("http://{addr}"))
        .await
        .expect("should detect local mock Ollama");
    assert_eq!(detected.name, "ollama");
    assert_eq!(detected.provider, "openai");
    assert_eq!(detected.base_url, Some(format!("http://{addr}/v1")));
    assert_eq!(
        detected.models,
        vec!["qwen2.5-coder:latest", "llama3.2:latest"]
    );
}

#[tokio::test]
async fn test_standalone_proxy_chat_completions_streaming_to_local_mock() {
    async fn mock_stream_completions() -> impl IntoResponse {
        let body = concat!(
            "data: {\"id\":\"chatcmpl-stream-1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"mock-stream\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hello \"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-stream-1\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"mock-stream\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"stream\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
            "data: [DONE]\n\n"
        );
        (
            StatusCode::OK,
            [("Content-Type", "text/event-stream")],
            body,
        )
    }

    let mock_app = Router::new().route("/v1/chat/completions", post(mock_stream_completions));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let boot = bootstrap_standalone(false).await.unwrap();
    let dep_req = CreateDeploymentRequest {
        name: "mock-stream".to_string(),
        provider: "openai".to_string(),
        model: "mock-stream".to_string(),
        api_key_ref: Some("test-key".to_string()),
        base_url: format!("http://{mock_addr}/v1"),
        is_active: true,
        weight: 1,
        priority: 0,
        max_tpm: None,
        max_rpm: None,
        cost_per_1k_input_tokens: None,
        cost_per_1k_output_tokens: None,
        metadata: None,
        sort_order: None,
    };
    boot.state.db.create_deployment(dep_req).await.unwrap();

    let cors = tower_http::cors::CorsLayer::permissive();
    let app = build_router(
        boot.state,
        hyperinfer_server::mcp::McpState::new(boot.jwt_secret.clone()),
        cors,
        false,
    );
    let server = TestServer::new(app);

    let chat_body = serde_json::json!({
        "model": "mock-stream",
        "stream": true,
        "messages": [{ "role": "user", "content": "stream please" }]
    });
    let res = server
        .post("/v1/chat/completions")
        .add_header("Authorization", format!("Bearer {}", boot.api_key))
        .json(&chat_body)
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let sse = res.text();
    assert!(sse.contains("hello "));
    assert!(sse.contains("stream"));
}
