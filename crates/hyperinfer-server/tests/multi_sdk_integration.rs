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
        _team_id: &str,
        _api_key_id: &str,
        _model: &str,
        _input_tokens: i32,
        _output_tokens: i32,
        _response_time_ms: i64,
    ) -> Result<UsageLog, DbError> {
        Err(DbError::NotFound)
    }
    async fn list_deployments(
        &self,
        _model: &str,
        _is_active: Option<bool>,
    ) -> Result<Vec<Deployment>, DbError> {
        Ok(vec![])
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
        },
        config_manager: StubConfigStore,
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
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
