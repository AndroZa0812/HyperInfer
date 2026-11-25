use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use chrono::{Duration, Utc};
use hyperinfer_core::{ApiKey, Database};
use hyperinfer_server::proxy::validate_api_key;

#[derive(Clone)]
struct StubDb {
    api_key: Arc<Option<ApiKey>>,
    spent_cents: Arc<AtomicI64>,
}

impl StubDb {
    fn new(key: ApiKey) -> Self {
        Self {
            api_key: Arc::new(Some(key.clone())),
            spent_cents: Arc::new(AtomicI64::new(key.spent_cents)),
        }
    }
}

#[async_trait::async_trait]
impl Database for StubDb {
    async fn get_api_key_by_hash(
        &self,
        _key_hash: &str,
    ) -> Result<Option<ApiKey>, hyperinfer_core::DbError> {
        Ok(self.api_key.as_ref().clone())
    }
    async fn record_api_key_spend(
        &self,
        _api_key_id: &str,
        cents: i64,
    ) -> Result<(), hyperinfer_core::DbError> {
        self.spent_cents.fetch_add(cents, Ordering::SeqCst);
        Ok(())
    }
    async fn get_team(
        &self,
        _id: &str,
    ) -> Result<Option<hyperinfer_core::Team>, hyperinfer_core::DbError> {
        Ok(None)
    }
    async fn create_team(
        &self,
        _name: &str,
        _budget: i64,
    ) -> Result<hyperinfer_core::Team, hyperinfer_core::DbError> {
        Ok(hyperinfer_core::Team {
            id: "t".into(),
            name: "t".into(),
            budget_cents: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
    async fn get_user(
        &self,
        _id: &str,
    ) -> Result<Option<hyperinfer_core::User>, hyperinfer_core::DbError> {
        Ok(None)
    }
    async fn get_user_by_email(
        &self,
        _email: &str,
    ) -> Result<Option<hyperinfer_core::User>, hyperinfer_core::DbError> {
        Ok(None)
    }
    async fn create_user(
        &self,
        _tid: &str,
        _email: &str,
        _role: &str,
        _ph: Option<String>,
    ) -> Result<hyperinfer_core::User, hyperinfer_core::DbError> {
        Ok(hyperinfer_core::User {
            id: "u".into(),
            team_id: "t".into(),
            email: "u@t.com".into(),
            role: "admin".into(),
            password_hash: None,
            created_at: Utc::now(),
        })
    }
    async fn update_password_hash(
        &self,
        _uid: &str,
        _ph: &str,
    ) -> Result<(), hyperinfer_core::DbError> {
        Ok(())
    }
    async fn count_users_by_role(&self, _role: &str) -> Result<i64, hyperinfer_core::DbError> {
        Ok(0)
    }
    async fn get_api_key(&self, _id: &str) -> Result<Option<ApiKey>, hyperinfer_core::DbError> {
        Ok(None)
    }
    async fn create_api_key(
        &self,
        _kh: &str,
        _uid: &str,
        _tid: &str,
        _name: Option<String>,
    ) -> Result<ApiKey, hyperinfer_core::DbError> {
        Ok(ApiKey {
            id: "k".into(),
            key_hash: "h".into(),
            user_id: "u".into(),
            team_id: "t".into(),
            name: None,
            is_active: true,
            created_at: Utc::now(),
            expires_at: None,
            budget_cents: None,
            spent_cents: 0,
            allowed_models: None,
            rpm_limit: None,
            tpm_limit: None,
        })
    }
    async fn deactivate_api_key(&self, _id: &str) -> Result<ApiKey, hyperinfer_core::DbError> {
        Ok(ApiKey {
            id: "k".into(),
            key_hash: "h".into(),
            user_id: "u".into(),
            team_id: "t".into(),
            name: None,
            is_active: false,
            created_at: Utc::now(),
            expires_at: None,
            budget_cents: None,
            spent_cents: 0,
            allowed_models: None,
            rpm_limit: None,
            tpm_limit: None,
        })
    }
    async fn get_model_alias(
        &self,
        _id: &str,
    ) -> Result<Option<hyperinfer_core::ModelAlias>, hyperinfer_core::DbError> {
        Ok(None)
    }
    async fn create_model_alias(
        &self,
        _tid: &str,
        _alias: &str,
        _target: &str,
        _provider: &str,
    ) -> Result<hyperinfer_core::ModelAlias, hyperinfer_core::DbError> {
        Ok(hyperinfer_core::ModelAlias {
            id: "a".into(),
            team_id: "t".into(),
            alias: "a".into(),
            target_model: "m".into(),
            provider: "p".into(),
            created_at: Utc::now(),
        })
    }
    async fn get_quota(
        &self,
        _tid: &str,
    ) -> Result<Option<hyperinfer_core::Quota>, hyperinfer_core::DbError> {
        Ok(None)
    }
    async fn create_quota(
        &self,
        _tid: &str,
        _rpm: i32,
        _tpm: i32,
    ) -> Result<hyperinfer_core::Quota, hyperinfer_core::DbError> {
        Ok(hyperinfer_core::Quota {
            id: "q".into(),
            team_id: "t".into(),
            rpm_limit: 100,
            tpm_limit: 1000,
            updated_at: Utc::now(),
        })
    }
    async fn record_usage(
        &self,
        _tid: &str,
        _aid: &str,
        _model: &str,
        _it: i32,
        _ot: i32,
        _ms: i64,
    ) -> Result<hyperinfer_core::UsageLog, hyperinfer_core::DbError> {
        Ok(hyperinfer_core::UsageLog {
            id: "ul".into(),
            team_id: "t".into(),
            api_key_id: "k".into(),
            model: "m".into(),
            input_tokens: 0,
            output_tokens: 0,
            response_time_ms: 0,
            recorded_at: Utc::now(),
        })
    }
    async fn list_deployments(
        &self,
        model: &str,
        _active: Option<bool>,
    ) -> Result<Vec<hyperinfer_core::Deployment>, hyperinfer_core::DbError> {
        Ok(vec![hyperinfer_core::Deployment {
            id: "dep-1".into(),
            name: "dep-1".into(),
            provider: "other".into(),
            model: model.into(),
            api_key_ref: "test-key".into(),
            base_url: "https://api.openai.com".into(),
            is_active: true,
            weight: 1,
            priority: 0,
            max_tpm: None,
            max_rpm: None,
            cost_per_1k_input_tokens: None,
            cost_per_1k_output_tokens: None,
            metadata: serde_json::json!({}),
            sort_order: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }])
    }
    async fn get_deployment(
        &self,
        _id: &str,
    ) -> Result<Option<hyperinfer_core::Deployment>, hyperinfer_core::DbError> {
        Ok(None)
    }
    async fn create_deployment(
        &self,
        _req: hyperinfer_core::CreateDeploymentRequest,
    ) -> Result<hyperinfer_core::Deployment, hyperinfer_core::DbError> {
        Ok(hyperinfer_core::Deployment {
            id: "d".into(),
            name: "d".into(),
            provider: "p".into(),
            model: "m".into(),
            api_key_ref: "k".into(),
            base_url: "u".into(),
            is_active: true,
            weight: 1,
            priority: 0,
            max_tpm: None,
            max_rpm: None,
            cost_per_1k_input_tokens: None,
            cost_per_1k_output_tokens: None,
            metadata: serde_json::json!({}),
            sort_order: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
    async fn update_deployment(
        &self,
        _id: &str,
        _req: hyperinfer_core::CreateDeploymentRequest,
    ) -> Result<hyperinfer_core::Deployment, hyperinfer_core::DbError> {
        Ok(hyperinfer_core::Deployment {
            id: "d".into(),
            name: "d".into(),
            provider: "p".into(),
            model: "m".into(),
            api_key_ref: "k".into(),
            base_url: "u".into(),
            is_active: true,
            weight: 1,
            priority: 0,
            max_tpm: None,
            max_rpm: None,
            cost_per_1k_input_tokens: None,
            cost_per_1k_output_tokens: None,
            metadata: serde_json::json!({}),
            sort_order: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
    async fn delete_deployment(&self, _id: &str) -> Result<(), hyperinfer_core::DbError> {
        Ok(())
    }
    async fn get_routing_config(
        &self,
    ) -> Result<Option<hyperinfer_core::RoutingConfig>, hyperinfer_core::DbError> {
        Ok(None)
    }
    async fn update_routing_config(
        &self,
        _req: hyperinfer_core::UpdateRoutingConfigRequest,
    ) -> Result<hyperinfer_core::RoutingConfig, hyperinfer_core::DbError> {
        Ok(hyperinfer_core::RoutingConfig {
            strategy: "weighted-shuffle".into(),
            strategy_params: serde_json::json!({}),
            fallback_config: serde_json::json!({}),
            routing_groups: serde_json::json!([]),
            updated_at: Utc::now(),
        })
    }
    async fn ping(&self) -> Result<(), hyperinfer_core::DbError> {
        Ok(())
    }
}

fn make_key() -> ApiKey {
    ApiKey {
        id: "key-1".into(),
        key_hash: "hash-1".into(),
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
    }
}

#[tokio::test]
async fn test_expired_key_rejected() {
    let mut key = make_key();
    key.expires_at = Some(Utc::now() - Duration::hours(1));
    let db = StubDb::new(key);
    let result = validate_api_key(&db, "sk-test").await;
    assert!(result.is_err());
    match result {
        Err(code) => assert_eq!(code, 401),
        Ok(_) => panic!("expected error"),
    }
}

#[tokio::test]
async fn test_model_not_allowed() {
    let mut key = make_key();
    key.allowed_models = Some(vec!["gpt-4".into()]);
    let db = StubDb::new(key);
    let auth = validate_api_key(&db, "sk-test").await.unwrap();
    let result = auth.check_request_allowed("gpt-3.5-turbo");
    assert!(result.is_err());
    let (code, msg) = result.unwrap_err();
    assert_eq!(code, 403);
    assert!(msg.contains("not allowed"));
}

#[tokio::test]
async fn test_budget_exceeded() {
    let mut key = make_key();
    key.budget_cents = Some(100);
    key.spent_cents = 100;
    let db = StubDb::new(key);
    let auth = validate_api_key(&db, "sk-test").await.unwrap();
    let result = auth.check_request_allowed("gpt-4");
    assert!(result.is_err());
    let (code, msg) = result.unwrap_err();
    assert_eq!(code, 402);
    assert!(msg.contains("budget"));
}

#[tokio::test]
async fn test_weighted_cost_accounting() {
    let calc = hyperinfer_core::CostCalculator::with_defaults();
    // Use 1M tokens so costs are in whole cents and differences are clear
    let normal = hyperinfer_core::WeightedUsage {
        input_tokens: 1_000_000,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
    };
    let cached = hyperinfer_core::WeightedUsage {
        input_tokens: 0,
        cached_input_tokens: 1_000_000,
        output_tokens: 0,
        reasoning_tokens: 0,
    };
    let output = hyperinfer_core::WeightedUsage {
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 1_000_000,
        reasoning_tokens: 0,
    };
    let reasoning = hyperinfer_core::WeightedUsage {
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 1_000_000,
    };

    let cost_normal = calc.calculate_cost_cents_ceil("gpt-4", &normal);
    let cost_cached = calc.calculate_cost_cents_ceil("gpt-4", &cached);
    let cost_output = calc.calculate_cost_cents_ceil("gpt-4", &output);
    let cost_reasoning = calc.calculate_cost_cents_ceil("gpt-4", &reasoning);

    assert!(cost_cached < cost_normal, "Cached input should cost less");
    assert!(cost_output > 0, "Output tokens should have a cost");
    assert!(cost_reasoning > 0, "Reasoning tokens should have a cost");
}

#[derive(Clone)]
struct StubConfigStore;

#[async_trait::async_trait]
impl hyperinfer_core::ConfigStore for StubConfigStore {
    async fn fetch_config(&self) -> Result<hyperinfer_core::Config, hyperinfer_core::ConfigError> {
        Ok(hyperinfer_core::Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        })
    }
    async fn publish_config_update(
        &self,
        _config: &hyperinfer_core::Config,
    ) -> Result<(), hyperinfer_core::ConfigError> {
        Ok(())
    }
    async fn publish_policy_update(
        &self,
        _update: &hyperinfer_core::PolicyUpdate,
    ) -> Result<(), hyperinfer_core::ConfigError> {
        Ok(())
    }
    async fn ping(&self) -> Result<(), hyperinfer_core::ConfigError> {
        Ok(())
    }
}

#[derive(Clone)]
struct MockUsageProvider;

#[async_trait::async_trait]
impl hyperinfer_providers::LlmProvider for MockUsageProvider {
    fn name(&self) -> &str {
        "other"
    }

    async fn chat(
        &self,
        request: &hyperinfer_core::ChatRequest,
        _api_key: &str,
    ) -> Result<hyperinfer_core::ChatResponse, hyperinfer_core::HyperInferError> {
        Ok(hyperinfer_core::ChatResponse {
            id: "chatcmpl-vk-1".to_string(),
            model: request.model.clone(),
            choices: vec![hyperinfer_core::Choice {
                index: 0,
                message: hyperinfer_core::ChatMessage::assistant("ok"),
                finish_reason: Some("stop".to_string()),
            }],
            usage: hyperinfer_core::Usage {
                input_tokens: 100_000,
                output_tokens: 50_000,
            },
        })
    }

    fn stream(
        &self,
        _request: &hyperinfer_core::ChatRequest,
        _api_key: &str,
    ) -> std::pin::Pin<
        Box<
            dyn futures::Stream<
                    Item = Result<hyperinfer_core::ChatChunk, hyperinfer_core::HyperInferError>,
                > + Send
                + 'static,
        >,
    > {
        Box::pin(futures::stream::empty())
    }
}

async fn build_vk_server(db: StubDb) -> axum_test::TestServer {
    let proxy_router = Arc::new(hyperinfer_server::proxy::ProxyRouter::new_in_memory().await);
    proxy_router.providers().register(MockUsageProvider);
    let config = hyperinfer_core::Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state = hyperinfer_server::AppState {
        config: Arc::new(tokio::sync::RwLock::new(config)),
        db,
        config_manager: StubConfigStore,
        admin_token: Arc::new("admin".to_string()),
        jwt_secret: Arc::new("secret".to_string()),
        proxy_router: Some(proxy_router),
        rate_limiter: None,
        plugins: hyperinfer_core::PluginPipeline::new(),
    };
    let app = hyperinfer_server::build_router(
        state,
        hyperinfer_server::mcp::McpState::new("secret"),
        tower_http::cors::CorsLayer::permissive(),
        false,
    );
    axum_test::TestServer::new(app)
}

#[tokio::test]
async fn test_http_virtual_key_enforcement_and_spend_charging() {
    // 1. Expired key -> 401
    let mut expired = make_key();
    expired.expires_at = Some(Utc::now() - Duration::hours(1));
    let server = build_vk_server(StubDb::new(expired)).await;
    let res = server
        .post("/v1/chat/completions")
        .add_header("Authorization", "Bearer sk-test")
        .json(&serde_json::json!({ "model": "gpt-4o", "messages": [{ "role": "user", "content": "hi" }] }))
        .await;
    assert_eq!(res.status_code(), 401);

    // 2. Disallowed model -> 403
    let mut restricted = make_key();
    restricted.allowed_models = Some(vec!["gpt-4o-mini".to_string()]);
    let server = build_vk_server(StubDb::new(restricted)).await;
    let res = server
        .post("/v1/chat/completions")
        .add_header("Authorization", "Bearer sk-test")
        .json(&serde_json::json!({ "model": "gpt-4o", "messages": [{ "role": "user", "content": "hi" }] }))
        .await;
    assert_eq!(res.status_code(), 403);

    // 3. Exhausted budget -> 402
    let mut over_budget = make_key();
    over_budget.budget_cents = Some(50);
    over_budget.spent_cents = 50;
    let server = build_vk_server(StubDb::new(over_budget)).await;
    let res = server
        .post("/v1/chat/completions")
        .add_header("Authorization", "Bearer sk-test")
        .json(&serde_json::json!({ "model": "gpt-4o", "messages": [{ "role": "user", "content": "hi" }] }))
        .await;
    assert_eq!(res.status_code(), 402);

    // 4. Allowed key -> 200 OK and post-response weighted cost charged
    let mut valid = make_key();
    valid.budget_cents = Some(10_000);
    valid.allowed_models = Some(vec!["gpt-4o".to_string()]);
    let db = StubDb::new(valid);
    let spent_ref = db.spent_cents.clone();
    let server = build_vk_server(db).await;
    let res = server
        .post("/v1/chat/completions")
        .add_header("Authorization", "Bearer sk-test")
        .json(&serde_json::json!({ "model": "gpt-4o", "messages": [{ "role": "user", "content": "hi" }] }))
        .await;
    assert_eq!(res.status_code(), 200);
    // gpt-4o: 100k input @ 250c/1M (25c) + 50k output @ 1000c/1M (50c) = 75 cents
    assert_eq!(spent_ref.load(Ordering::SeqCst), 75);
}
