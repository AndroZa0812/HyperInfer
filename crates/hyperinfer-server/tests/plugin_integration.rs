//! Integration tests for GatewayPlugin pipeline and built-in RedactionPlugin (HYP-67).

use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use axum_test::TestServer;
use chrono::Utc;
use hyperinfer_core::{
    hash_key, ApiKey, ChatMessage, ChatRequest, ChatResponse, Choice, Config, ConfigError,
    ConfigStore, CreateDeploymentRequest, Database, DbError, Deployment, GatewayPlugin, ModelAlias,
    PluginAction, PluginContext, PluginPipeline, PolicyUpdate, Quota, RedactionPlugin,
    RoutingConfig, Team, UpdateRoutingConfigRequest, Usage, UsageLog, User,
};
use tokio::sync::RwLock;

#[derive(Clone)]
struct StubDb {
    valid_key_hash: String,
}

#[async_trait]
impl Database for StubDb {
    async fn get_team(&self, _id: &str) -> Result<Option<Team>, DbError> {
        Ok(None)
    }
    async fn create_team(&self, _name: &str, _budget: i64) -> Result<Team, DbError> {
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
        _tid: &str,
        _e: &str,
        _r: &str,
        _p: Option<String>,
    ) -> Result<User, DbError> {
        Err(DbError::NotFound)
    }
    async fn update_password_hash(&self, _uid: &str, _p: &str) -> Result<(), DbError> {
        Ok(())
    }
    async fn count_users_by_role(&self, _r: &str) -> Result<i64, DbError> {
        Ok(0)
    }
    async fn get_api_key(&self, _id: &str) -> Result<Option<ApiKey>, DbError> {
        Ok(None)
    }
    async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DbError> {
        if key_hash == self.valid_key_hash {
            Ok(Some(ApiKey {
                id: "key-1".into(),
                key_hash: key_hash.into(),
                user_id: "user-1".into(),
                team_id: "team-1".into(),
                name: Some("test".into()),
                is_active: true,
                created_at: Utc::now(),
                expires_at: None,
                budget_cents: None,
                spent_cents: 0,
                allowed_models: None,
                rpm_limit: None,
                tpm_limit: None,
            }))
        } else {
            Ok(None)
        }
    }
    async fn create_api_key(
        &self,
        _kh: &str,
        _u: &str,
        _t: &str,
        _n: Option<String>,
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
        _t: &str,
        _a: &str,
        _m: &str,
        _p: &str,
    ) -> Result<ModelAlias, DbError> {
        Err(DbError::NotFound)
    }
    async fn get_quota(&self, _t: &str) -> Result<Option<Quota>, DbError> {
        Ok(None)
    }
    async fn create_quota(&self, _t: &str, _r: i32, _tp: i32) -> Result<Quota, DbError> {
        Err(DbError::NotFound)
    }
    async fn record_usage(
        &self,
        _t: &str,
        _a: &str,
        _m: &str,
        _i: i32,
        _o: i32,
        _ms: i64,
    ) -> Result<UsageLog, DbError> {
        Err(DbError::NotFound)
    }
    async fn list_deployments(
        &self,
        _m: &str,
        _a: Option<bool>,
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

#[async_trait]
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

struct GuardrailPlugin;

#[async_trait]
impl GatewayPlugin for GuardrailPlugin {
    fn name(&self) -> &str {
        "guardrail"
    }

    async fn pre_request(&self, _ctx: &mut PluginContext, req: &mut ChatRequest) -> PluginAction {
        if req.messages.iter().any(|m| m.content.contains("forbidden")) {
            return PluginAction::ShortCircuit {
                status_code: 400,
                message: "Blocked by guardrail".to_string(),
            };
        }
        PluginAction::Continue
    }
}

struct EchoPlugin;

#[async_trait]
impl GatewayPlugin for EchoPlugin {
    fn name(&self) -> &str {
        "echo"
    }

    async fn pre_request(&self, _ctx: &mut PluginContext, req: &mut ChatRequest) -> PluginAction {
        let echoed = req
            .messages
            .first()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        PluginAction::Respond(ChatResponse {
            id: "resp-plugin".into(),
            model: req.model.clone(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage::assistant(format!(
                    "Echo: {echoed} | secret sk-ant-abcdefghijklmnop"
                )),
                finish_reason: Some("stop".into()),
            }],
            usage: Usage::default(),
        })
    }
}

fn build_server(plugins: PluginPipeline) -> TestServer {
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
        admin_token: Arc::new("test-token".into()),
        jwt_secret: Arc::new("test-jwt-secret".into()),
        proxy_router: None,
        rate_limiter: None,
        plugins,
    };
    let cors = tower_http::cors::CorsLayer::permissive();
    let mcp_state = hyperinfer_server::mcp::McpState::new("test-jwt-secret");
    let app = hyperinfer_server::build_router(state, mcp_state, cors, false);
    TestServer::new(app)
}

#[tokio::test]
async fn test_guardrail_plugin_short_circuits_with_400() {
    let plugins = PluginPipeline::new().with_plugin(Arc::new(GuardrailPlugin));
    let server = build_server(plugins);

    let res = server
        .post("/v1/chat/completions")
        .add_header("Authorization", "Bearer sk-test-123")
        .json(&serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "this is forbidden content"}]
        }))
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = res.json();
    assert_eq!(body["error"], "Blocked by guardrail");
}

#[tokio::test]
async fn test_redaction_and_respond_plugins_pipeline() {
    let plugins = PluginPipeline::new()
        .with_plugin(Arc::new(RedactionPlugin::with_defaults()))
        .with_plugin(Arc::new(EchoPlugin));
    let server = build_server(plugins);

    let res = server
        .post("/v1/chat/completions")
        .add_header("Authorization", "Bearer sk-test-123")
        .json(&serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "My email is alice@example.com"}]
        }))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: serde_json::Value = res.json();
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(content.contains("[REDACTED_EMAIL]"));
    assert!(!content.contains("alice@example.com"));
    assert!(content.contains("[REDACTED_API_KEY]"));
    assert!(!content.contains("sk-ant-abcdefghijklmnop"));
}
