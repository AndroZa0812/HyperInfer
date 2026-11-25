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
        _model: &str,
        _active: Option<bool>,
    ) -> Result<Vec<hyperinfer_core::Deployment>, hyperinfer_core::DbError> {
        Ok(vec![])
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
