use std::sync::Arc;

use axum_test::TestServer;
use chrono::Utc;
use hyperinfer_core::{
    hash_key, ApiKey, Config, ConfigError, ConfigStore, CreateDeploymentRequest, Database, DbError,
    Deployment, ModelAlias, PolicyUpdate, Quota, RoutingConfig, Team, UpdateRoutingConfigRequest,
    UsageLog, User,
};
use tokio::sync::RwLock;

#[derive(Clone)]
struct StubDb {
    valid_key_hash: String,
    deployments: Vec<Deployment>,
}

#[async_trait::async_trait]
impl Database for StubDb {
    async fn get_team(&self, _id: &str) -> Result<Option<Team>, DbError> {
        Ok(None)
    }
    async fn create_team(&self, _name: &str, _budget_cents: i64) -> Result<Team, DbError> {
        Err(DbError::NotFound)
    }
    async fn get_user(&self, _id: &str) -> Result<Option<User>, DbError> {
        Ok(None)
    }
    async fn get_user_by_email(&self, _email: &str) -> Result<Option<User>, DbError> {
        Ok(None)
    }
    async fn create_user(
        &self,
        _team_id: &str,
        _email: &str,
        _role: &str,
        _password_hash: Option<String>,
    ) -> Result<User, DbError> {
        Err(DbError::NotFound)
    }
    async fn update_password_hash(
        &self,
        _user_id: &str,
        _password_hash: &str,
    ) -> Result<(), DbError> {
        Ok(())
    }
    async fn count_users_by_role(&self, _role: &str) -> Result<i64, DbError> {
        Ok(0)
    }
    async fn get_api_key(&self, _id: &str) -> Result<Option<ApiKey>, DbError> {
        Ok(None)
    }
    async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DbError> {
        if key_hash == self.valid_key_hash {
            Ok(Some(ApiKey {
                id: "key-1".to_string(),
                key_hash: key_hash.to_string(),
                user_id: "user-1".to_string(),
                team_id: "team-1".to_string(),
                name: Some("test".to_string()),
                is_active: true,
                created_at: Utc::now(),
                expires_at: None,
            }))
        } else {
            Ok(None)
        }
    }
    async fn create_api_key(
        &self,
        _key_hash: &str,
        _user_id: &str,
        _team_id: &str,
        _name: Option<String>,
    ) -> Result<ApiKey, DbError> {
        Err(DbError::NotFound)
    }
    async fn deactivate_api_key(&self, _id: &str) -> Result<ApiKey, DbError> {
        Err(DbError::NotFound)
    }
    async fn get_model_alias(&self, _id: &str) -> Result<Option<ModelAlias>, DbError> {
        Ok(None)
    }
    async fn create_model_alias(
        &self,
        _team_id: &str,
        _alias: &str,
        _target_model: &str,
        _provider: &str,
    ) -> Result<ModelAlias, DbError> {
        Err(DbError::NotFound)
    }
    async fn get_quota(&self, _team_id: &str) -> Result<Option<Quota>, DbError> {
        Ok(None)
    }
    async fn create_quota(
        &self,
        _team_id: &str,
        _rpm_limit: i32,
        _tpm_limit: i32,
    ) -> Result<Quota, DbError> {
        Err(DbError::NotFound)
    }
    async fn record_usage(
        &self,
        team_id: &str,
        api_key_id: &str,
        model: &str,
        input_tokens: i32,
        output_tokens: i32,
        response_time_ms: i64,
    ) -> Result<UsageLog, DbError> {
        Ok(UsageLog {
            id: "usage-1".to_string(),
            team_id: team_id.to_string(),
            api_key_id: api_key_id.to_string(),
            model: model.to_string(),
            input_tokens,
            output_tokens,
            response_time_ms,
            recorded_at: Utc::now(),
        })
    }
    async fn list_deployments(
        &self,
        model: &str,
        is_active: Option<bool>,
    ) -> Result<Vec<Deployment>, DbError> {
        Ok(self
            .deployments
            .iter()
            .filter(|d| model.is_empty() || d.model == model)
            .filter(|d| is_active.is_none_or(|flag| d.is_active == flag))
            .cloned()
            .collect())
    }
    async fn get_deployment(&self, _id: &str) -> Result<Option<Deployment>, DbError> {
        Ok(None)
    }
    async fn create_deployment(
        &self,
        _req: CreateDeploymentRequest,
    ) -> Result<Deployment, DbError> {
        Err(DbError::NotFound)
    }
    async fn update_deployment(
        &self,
        _id: &str,
        _req: CreateDeploymentRequest,
    ) -> Result<Deployment, DbError> {
        Err(DbError::NotFound)
    }
    async fn delete_deployment(&self, _id: &str) -> Result<(), DbError> {
        Ok(())
    }
    async fn get_routing_config(&self) -> Result<Option<RoutingConfig>, DbError> {
        Ok(None)
    }
    async fn update_routing_config(
        &self,
        _req: UpdateRoutingConfigRequest,
    ) -> Result<RoutingConfig, DbError> {
        Err(DbError::NotFound)
    }
    async fn ping(&self) -> Result<(), DbError> {
        Ok(())
    }
}

#[derive(Clone)]
struct StubConfigStore;

#[async_trait::async_trait]
impl ConfigStore for StubConfigStore {
    async fn fetch_config(&self) -> Result<Config, ConfigError> {
        Ok(Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        })
    }
    async fn publish_config_update(&self, _config: &Config) -> Result<(), ConfigError> {
        Ok(())
    }
    async fn publish_policy_update(&self, _update: &PolicyUpdate) -> Result<(), ConfigError> {
        Ok(())
    }
    async fn ping(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

fn make_server() -> TestServer {
    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state = hyperinfer_server::AppState {
        config: Arc::new(RwLock::new(config)),
        db: StubDb {
            valid_key_hash: hash_key("sk-test-123"),
            deployments: vec![],
        },
        config_manager: StubConfigStore,
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
        plugins: hyperinfer_core::PluginPipeline::new(),
    };
    let cors = tower_http::cors::CorsLayer::permissive();
    let mcp_state = hyperinfer_server::mcp::McpState::new("test-jwt-secret");
    let app = hyperinfer_server::build_router(state, mcp_state, cors, false);
    TestServer::new(app)
}

#[tokio::test]
async fn test_anthropic_messages_auth_and_routing() {
    let server = make_server();

    let res = server
        .post("/v1/messages")
        .json(&serde_json::json!({"model": "claude-3", "messages": []}))
        .await;
    assert_eq!(res.status_code(), 401);

    let res = server
        .post("/v1/messages")
        .add_header("x-api-key", "sk-test-123")
        .json(&serde_json::json!({"model": "claude-3", "messages": []}))
        .await;
    assert_eq!(res.status_code(), 404);
}

#[tokio::test]
async fn test_openai_responses_auth_and_routing() {
    let server = make_server();

    let res = server
        .post("/v1/responses")
        .json(&serde_json::json!({"model": "gpt-4", "input": "hello"}))
        .await;
    assert_eq!(res.status_code(), 401);

    let res = server
        .post("/v1/responses")
        .add_header("authorization", "Bearer sk-test-123")
        .json(&serde_json::json!({"model": "gpt-4", "input": "hello"}))
        .await;
    assert_eq!(res.status_code(), 404);
}

#[tokio::test]
async fn test_gemini_generate_content_auth_and_action_validation() {
    let server = make_server();

    let res = server
        .post("/v1beta/models/gemini-2.5-flash:invalidAction")
        .json(&serde_json::json!({"contents": []}))
        .await;
    assert_eq!(res.status_code(), 400);

    let res = server
        .post("/v1beta/models/gemini-2.5-flash:generateContent")
        .json(&serde_json::json!({"contents": []}))
        .await;
    assert_eq!(res.status_code(), 401);

    let res = server
        .post("/v1beta/models/gemini-2.5-flash:generateContent")
        .add_header("x-goog-api-key", "sk-test-123")
        .json(&serde_json::json!({"contents": []}))
        .await;
    assert_eq!(res.status_code(), 404);

    let res = server
        .post("/v1beta/models/gemini-2.5-flash:generateContent?key=sk-test-123")
        .json(&serde_json::json!({"contents": []}))
        .await;
    assert_eq!(res.status_code(), 404);
}

#[tokio::test]
async fn test_deployment_mutators_applied_on_select() {
    let dep = Deployment {
        id: "dep-1".to_string(),
        name: "test-dep".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4".to_string(),
        api_key_ref: "ref-1".to_string(),
        base_url: "https://api.openai.com".to_string(),
        is_active: true,
        weight: 1,
        priority: 0,
        max_tpm: None,
        max_rpm: None,
        cost_per_1k_input_tokens: None,
        cost_per_1k_output_tokens: None,
        metadata: serde_json::json!({
            "mutators": {
                "modelNameOverride": "gpt-4o-mini-2024-07-18",
                "max_tokens_cap": 256,
                "temperature_override": 0.2
            }
        }),
        sort_order: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let mutators = hyperinfer_server::mutators::DeploymentMutators::from_metadata(&dep.metadata);
    assert_eq!(
        mutators.model_name_override,
        Some("gpt-4o-mini-2024-07-18".to_string())
    );
    assert_eq!(mutators.max_tokens_cap, Some(256));
    assert_eq!(mutators.temperature_override, Some(0.2));

    let mut req = hyperinfer_core::ChatRequest {
        model: "gpt-4".to_string(),
        messages: vec![],
        max_tokens: Some(1024),
        temperature: Some(0.8),
        stream: None,
        stop: None,
        tools: None,
        tool_choice: None,
        response_format: None,
    };
    mutators.apply_to_request(&mut req);
    assert_eq!(req.model, "gpt-4o-mini-2024-07-18");
    assert_eq!(req.max_tokens, Some(256));
    assert_eq!(req.temperature, Some(0.2));
}

#[derive(Clone)]
struct RecordingMockProvider {
    seen_requests: Arc<RwLock<Vec<hyperinfer_core::ChatRequest>>>,
}

#[async_trait::async_trait]
impl hyperinfer_providers::LlmProvider for RecordingMockProvider {
    fn name(&self) -> &str {
        "other"
    }

    async fn chat(
        &self,
        request: &hyperinfer_core::ChatRequest,
        _api_key: &str,
    ) -> Result<hyperinfer_core::ChatResponse, hyperinfer_core::HyperInferError> {
        self.seen_requests.write().await.push(request.clone());
        Ok(hyperinfer_core::ChatResponse {
            id: "chatcmpl-mock-1".to_string(),
            model: request.model.clone(),
            choices: vec![hyperinfer_core::Choice {
                index: 0,
                message: hyperinfer_core::ChatMessage::assistant("hello from mock provider"),
                finish_reason: Some("stop".to_string()),
            }],
            usage: hyperinfer_core::Usage {
                input_tokens: 12,
                output_tokens: 6,
            },
        })
    }

    fn stream(
        &self,
        request: &hyperinfer_core::ChatRequest,
        _api_key: &str,
    ) -> std::pin::Pin<
        Box<
            dyn futures::Stream<
                    Item = Result<hyperinfer_core::ChatChunk, hyperinfer_core::HyperInferError>,
                > + Send
                + 'static,
        >,
    > {
        let model = request.model.clone();
        let chunks = vec![
            Ok(hyperinfer_core::ChatChunk {
                id: "chunk-1".to_string(),
                model: model.clone(),
                delta: "Hello ".to_string(),
                finish_reason: None,
                usage: None,
                tool_calls: None,
            }),
            Ok(hyperinfer_core::ChatChunk {
                id: "chunk-1".to_string(),
                model,
                delta: "world".to_string(),
                finish_reason: Some("stop".to_string()),
                usage: Some(hyperinfer_core::Usage {
                    input_tokens: 8,
                    output_tokens: 4,
                }),
                tool_calls: None,
            }),
        ];
        Box::pin(futures::stream::iter(chunks))
    }
}

async fn make_e2e_server(
    deployments: Vec<Deployment>,
) -> (TestServer, Arc<RwLock<Vec<hyperinfer_core::ChatRequest>>>) {
    let seen = Arc::new(RwLock::new(Vec::new()));
    let proxy_router = Arc::new(hyperinfer_server::proxy::ProxyRouter::new_in_memory().await);
    proxy_router.providers().register(RecordingMockProvider {
        seen_requests: seen.clone(),
    });
    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state = hyperinfer_server::AppState {
        config: Arc::new(RwLock::new(config)),
        db: StubDb {
            valid_key_hash: hash_key("sk-test-123"),
            deployments,
        },
        config_manager: StubConfigStore,
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: Some(proxy_router),
        rate_limiter: None,
        plugins: hyperinfer_core::PluginPipeline::new(),
    };
    let cors = tower_http::cors::CorsLayer::permissive();
    let mcp_state = hyperinfer_server::mcp::McpState::new("test-jwt-secret");
    let app = hyperinfer_server::build_router(state, mcp_state, cors, false);
    (TestServer::new(app), seen)
}

fn mock_openai_deployment(model: &str, metadata: serde_json::Value) -> Deployment {
    Deployment {
        id: format!("dep-{model}"),
        name: format!("dep-{model}"),
        provider: "other".to_string(),
        model: model.to_string(),
        api_key_ref: "test-key".to_string(),
        base_url: "https://api.openai.com".to_string(),
        is_active: true,
        weight: 1,
        priority: 0,
        max_tpm: None,
        max_rpm: None,
        cost_per_1k_input_tokens: None,
        cost_per_1k_output_tokens: None,
        metadata,
        sort_order: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[tokio::test]
async fn test_multi_sdk_e2e_anthropic_responses_gemini_and_mutators() {
    let dep = mock_openai_deployment(
        "virtual-smart",
        serde_json::json!({
            "mutators": {
                "modelNameOverride": "gpt-4o-actual",
                "max_tokens_cap": 128
            }
        }),
    );
    let (server, seen) = make_e2e_server(vec![dep]).await;

    // 1. Anthropic POST /v1/messages (non-streaming) + mutator verification
    let res = server
        .post("/v1/messages")
        .add_header("x-api-key", "sk-test-123")
        .json(&serde_json::json!({
            "model": "virtual-smart",
            "max_tokens": 2048,
            "system": "Be concise",
            "messages": [{ "role": "user", "content": "Hi" }]
        }))
        .await;
    assert_eq!(res.status_code(), 200);
    let body: serde_json::Value = res.json();
    assert_eq!(body["type"], "message");
    assert_eq!(body["model"], "virtual-smart");
    assert_eq!(body["stop_reason"], "end_turn");
    assert_eq!(body["content"][0]["text"], "hello from mock provider");
    assert_eq!(body["usage"]["input_tokens"], 12);

    let recorded = seen.read().await;
    assert_eq!(recorded[0].model, "gpt-4o-actual");
    assert_eq!(recorded[0].max_tokens, Some(128));
    drop(recorded);

    // 2. Anthropic POST /v1/messages (SSE streaming)
    let stream_res = server
        .post("/v1/messages")
        .add_header("x-api-key", "sk-test-123")
        .json(&serde_json::json!({
            "model": "virtual-smart",
            "stream": true,
            "messages": [{ "role": "user", "content": "Stream hi" }]
        }))
        .await;
    assert_eq!(stream_res.status_code(), 200);
    let sse_text = stream_res.text();
    assert!(sse_text.contains("event: message_start"));
    assert!(sse_text.contains("event: content_block_delta"));
    assert!(sse_text.contains("event: content_block_stop"));
    assert!(sse_text.contains("event: message_delta"));
    assert!(sse_text.contains("event: message_stop"));

    // 3. OpenAI Responses POST /v1/responses
    let resp_res = server
        .post("/v1/responses")
        .add_header("authorization", "Bearer sk-test-123")
        .json(&serde_json::json!({
            "model": "virtual-smart",
            "input": "Hello responses"
        }))
        .await;
    assert_eq!(resp_res.status_code(), 200);
    let resp_body: serde_json::Value = resp_res.json();
    assert_eq!(resp_body["object"], "response");
    assert_eq!(resp_body["status"], "completed");
    assert_eq!(resp_body["output_text"], "hello from mock provider");

    // 4. Gemini POST /v1beta/models/{model}:generateContent
    let gem_res = server
        .post("/v1beta/models/virtual-smart:generateContent")
        .add_header("x-goog-api-key", "sk-test-123")
        .json(&serde_json::json!({
            "contents": [{ "role": "user", "parts": [{ "text": "Hello gemini" }] }]
        }))
        .await;
    assert_eq!(gem_res.status_code(), 200);
    let gem_body: serde_json::Value = gem_res.json();
    assert_eq!(gem_body["candidates"][0]["content"]["role"], "model");
    assert_eq!(
        gem_body["candidates"][0]["content"]["parts"][0]["text"],
        "hello from mock provider"
    );
    assert_eq!(gem_body["candidates"][0]["finishReason"], "STOP");
    assert_eq!(gem_body["usageMetadata"]["promptTokenCount"], 12);
}
