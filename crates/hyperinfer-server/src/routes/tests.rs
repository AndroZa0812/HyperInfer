use super::keys::*;
use super::routing::*;
use super::teams::*;
use super::users::*;
use super::*;
use crate::auth::{AuthClaims, RequireAdmin};
use crate::state::{build_cors_layer_from_string, hash_key, resolve_api_key, AppState};
use axum::extract::{Extension, Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use hyperinfer_core::Config;
use hyperinfer_core::{
    ApiKey, ConfigError, DbError, ModelAlias, PolicyUpdate, Quota, Team, UsageLog, UsageSummary,
    User,
};
use mockall::mock;
use mockall::predicate::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

mock! {
    pub Database {}

    impl Clone for Database {
        fn clone(&self) -> Self;
    }

    #[async_trait::async_trait]
    impl hyperinfer_core::Database for Database {
        async fn get_team(&self, id: &str) -> Result<Option<Team>, DbError>;
        async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, DbError>;
        async fn get_user(&self, id: &str) -> Result<Option<User>, DbError>;
        async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, DbError>;
        async fn create_user(&self, team_id: &str, email: &str, role: &str, password_hash: Option<String>) -> Result<User, DbError>;
        async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError>;
        async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DbError>;
        async fn create_api_key(&self, key_hash: &str, user_id: &str, team_id: &str, name: Option<String>) -> Result<ApiKey, DbError>;
        async fn deactivate_api_key(&self, id: &str) -> Result<ApiKey, DbError>;
        async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError>;
        async fn create_model_alias(&self, team_id: &str, alias: &str, target_model: &str, provider: &str) -> Result<ModelAlias, DbError>;
        async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError>;
        async fn create_quota(&self, team_id: &str, rpm_limit: i32, tpm_limit: i32) -> Result<Quota, DbError>;
        async fn record_usage(&self, team_id: &str, api_key_id: &str, model: &str, input_tokens: i32, output_tokens: i32, response_time_ms: i64) -> Result<UsageLog, DbError>;
        async fn count_users_by_role(&self, role: &str) -> Result<i64, DbError>;
        async fn update_password_hash(&self, user_id: &str, password_hash: &str) -> Result<(), DbError>;
        async fn list_deployments(&self, model: &str, is_active: Option<bool>) -> Result<Vec<hyperinfer_core::Deployment>, DbError>;
        async fn get_deployment(&self, id: &str) -> Result<Option<hyperinfer_core::Deployment>, DbError>;
        async fn create_deployment(&self, req: hyperinfer_core::CreateDeploymentRequest) -> Result<hyperinfer_core::Deployment, DbError>;
        async fn update_deployment(&self, id: &str, req: hyperinfer_core::CreateDeploymentRequest) -> Result<hyperinfer_core::Deployment, DbError>;
        async fn delete_deployment(&self, id: &str) -> Result<(), DbError>;
        async fn get_routing_config(&self) -> Result<Option<hyperinfer_core::RoutingConfig>, DbError>;
        async fn update_routing_config(&self, req: hyperinfer_core::UpdateRoutingConfigRequest) -> Result<hyperinfer_core::RoutingConfig, DbError>;
        async fn list_teams(&self) -> Result<Vec<Team>, DbError>;
        async fn list_api_keys(&self, team_id: Option<String>) -> Result<Vec<ApiKey>, DbError>;
        async fn get_usage_summary(&self, team_id: Option<String>) -> Result<UsageSummary, DbError>;
        async fn ping(&self) -> Result<(), DbError>;
    }
}

mock! {
    pub ConfigStore {}

    impl Clone for ConfigStore {
        fn clone(&self) -> Self;
    }

    #[async_trait::async_trait]
    impl hyperinfer_core::ConfigStore for ConfigStore {
        async fn fetch_config(&self) -> Result<Config, ConfigError>;
        async fn publish_config_update(&self, config: &Config) -> Result<(), ConfigError>;
        async fn publish_policy_update(&self, update: &PolicyUpdate) -> Result<(), ConfigError>;
        async fn ping(&self) -> Result<(), ConfigError>;
    }
}

fn create_test_state() -> AppState<MockDatabase, MockConfigStore> {
    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    AppState {
        config: Arc::new(RwLock::new(config)),
        db: MockDatabase::new(),
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    }
}

#[tokio::test]
async fn test_config_sync() {
    let state = create_test_state();
    let response = config_sync(State(state)).await;
    let json = response.into_response();
    assert_eq!(json.status(), StatusCode::OK);
}

fn admin_claims() -> AuthClaims {
    AuthClaims {
        sub: "admin-123".to_string(),
        email: "admin@test.com".to_string(),
        role: "admin".to_string(),
        team_id: "team-admin".to_string(),
        exp: 9999999999,
    }
}

fn member_claims(team_id: &str) -> AuthClaims {
    AuthClaims {
        sub: "member-123".to_string(),
        email: "member@test.com".to_string(),
        role: "member".to_string(),
        team_id: team_id.to_string(),
        exp: 9999999999,
    }
}

#[tokio::test]
async fn test_get_team_not_found() {
    let mut db = MockDatabase::new();
    db.expect_get_team()
        .with(eq("nonexistent-id"))
        .times(1)
        .returning(|_| Ok(None));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let response = get_team(
        State(state),
        Extension(admin_claims()),
        Path("nonexistent-id".to_string()),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_team_found() {
    use chrono::Utc;

    let mut db = MockDatabase::new();
    let now = Utc::now();
    let team = Team {
        id: "test-team-id".to_string(),
        name: "Test Team".to_string(),
        budget_cents: 10000,
        created_at: now,
        updated_at: now,
    };
    let team_clone = team.clone();
    db.expect_get_team()
        .with(eq("test-team-id"))
        .times(1)
        .returning(move |_| Ok(Some(team_clone.clone())));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let response = get_team(
        State(state),
        Extension(member_claims("test-team-id")),
        Path("test-team-id".to_string()),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_team() {
    use chrono::Utc;

    let mut db = MockDatabase::new();
    let now = Utc::now();
    let team = Team {
        id: "new-team-id".to_string(),
        name: "New Team".to_string(),
        budget_cents: 5000,
        created_at: now,
        updated_at: now,
    };
    db.expect_create_team()
        .with(eq("New Team"), eq(5000i64))
        .times(1)
        .returning(move |_, _| Ok(team.clone()));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let admin = RequireAdmin(AuthClaims {
        sub: "admin-123".to_string(),
        email: "admin@test.com".to_string(),
        role: "admin".to_string(),
        team_id: "team-123".to_string(),
        exp: 9999999999,
    });

    let response = create_team(
        State(state),
        admin,
        Json(CreateTeamRequest {
            name: "New Team".to_string(),
            budget_cents: 5000,
        }),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_user_not_found() {
    let mut db = MockDatabase::new();
    db.expect_get_user()
        .with(eq("nonexistent-user"))
        .times(1)
        .returning(|_| Ok(None));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let response = get_user(
        State(state),
        Extension(admin_claims()),
        Path("nonexistent-user".to_string()),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_api_key_not_found() {
    let mut db = MockDatabase::new();
    db.expect_get_api_key()
        .with(eq("nonexistent-key"))
        .times(1)
        .returning(|_| Ok(None));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let response = get_api_key(
        State(state),
        Extension(admin_claims()),
        Path("nonexistent-key".to_string()),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_model_alias_not_found() {
    let mut db = MockDatabase::new();
    db.expect_get_model_alias()
        .with(eq("nonexistent-alias"))
        .times(1)
        .returning(|_| Ok(None));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let response = get_model_alias(
        State(state),
        Extension(admin_claims()),
        Path("nonexistent-alias".to_string()),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_quota_not_found() {
    let mut db = MockDatabase::new();
    db.expect_get_quota()
        .with(eq("nonexistent-team"))
        .times(1)
        .returning(|_| Ok(None));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let response = get_quota(
        State(state),
        Extension(admin_claims()),
        Path("nonexistent-team".to_string()),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_team_database_error() {
    let mut db = MockDatabase::new();
    db.expect_get_team()
        .with(eq("error-id"))
        .times(1)
        .returning(|_| Err(DbError::Sqlx(sqlx::Error::Protocol("test error".into()))));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let response = get_team(
        State(state),
        Extension(admin_claims()),
        Path("error-id".to_string()),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_idor_cross_tenant_get_endpoints_forbidden() {
    use chrono::Utc;

    let now = Utc::now();

    // 1. get_team rejects cross-tenant member before touching DB
    let resp = get_team(
        State(create_test_state()),
        Extension(member_claims("team-a")),
        Path("team-b".to_string()),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 2. get_quota rejects cross-tenant member before touching DB
    let resp = get_quota(
        State(create_test_state()),
        Extension(member_claims("team-a")),
        Path("team-b".to_string()),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 3. get_user rejects user belonging to another team
    let mut db = MockDatabase::new();
    db.expect_get_user().times(1).returning(move |_| {
        Ok(Some(User {
            id: "u-b".to_string(),
            team_id: "team-b".to_string(),
            email: "b@example.com".to_string(),
            role: "member".to_string(),
            password_hash: None,
            created_at: now,
        }))
    });
    let mut state = create_test_state();
    state.db = db;
    let resp = get_user(
        State(state),
        Extension(member_claims("team-a")),
        Path("u-b".to_string()),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 4. get_api_key rejects API key belonging to another team
    let mut db = MockDatabase::new();
    db.expect_get_api_key().times(1).returning(move |_| {
        Ok(Some(ApiKey {
            id: "k-b".to_string(),
            key_hash: "hash".to_string(),
            user_id: "u-b".to_string(),
            team_id: "team-b".to_string(),
            name: None,
            is_active: true,
            created_at: now,
            expires_at: None,
            budget_cents: None,
            spent_cents: 0,
            allowed_models: None,
            rpm_limit: None,
            tpm_limit: None,
        }))
    });
    let mut state = create_test_state();
    state.db = db;
    let resp = get_api_key(
        State(state),
        Extension(member_claims("team-a")),
        Path("k-b".to_string()),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 5. get_model_alias rejects alias belonging to another team
    let mut db = MockDatabase::new();
    db.expect_get_model_alias().times(1).returning(move |_| {
        Ok(Some(ModelAlias {
            id: "a-b".to_string(),
            team_id: "team-b".to_string(),
            alias: "fast".to_string(),
            target_model: "gpt-4o-mini".to_string(),
            provider: "openai".to_string(),
            created_at: now,
        }))
    });
    let mut state = create_test_state();
    state.db = db;
    let resp = get_model_alias(
        State(state),
        Extension(member_claims("team-a")),
        Path("a-b".to_string()),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn test_hash_key() {
    let key = "test-api-key";
    let hash1 = hash_key(key);
    let hash2 = hash_key(key);

    // Same input should produce same hash
    assert_eq!(hash1, hash2);

    // Hash should be hex string
    assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));

    // Different inputs should produce different hashes
    let different_hash = hash_key("different-key");
    assert_ne!(hash1, different_hash);
}

#[test]
fn test_hash_key_empty_string() {
    let hash = hash_key("");
    assert!(!hash.is_empty());
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_hash_key_special_characters() {
    let key = "test-key-with-!@#$%^&*()";
    let hash = hash_key(key);
    assert!(!hash.is_empty());
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_hash_key_unicode() {
    let key = "test-key-with-unicode-🔑";
    let hash = hash_key(key);
    assert!(!hash.is_empty());
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_hash_key_long_string() {
    let key = "a".repeat(10000);
    let hash = hash_key(&key);
    assert!(!hash.is_empty());
    // SHA-256 produces 64 hex characters
    assert_eq!(hash.len(), 64);
}

#[tokio::test]
async fn test_resolve_api_key_found() {
    use chrono::Utc;

    let mut db = MockDatabase::new();
    let now = Utc::now();
    let api_key = ApiKey {
        id: "key-id".to_string(),
        key_hash: hash_key("test-key"),
        user_id: "user-id".to_string(),
        team_id: "team-id".to_string(),
        name: None,
        is_active: true,
        created_at: now,
        expires_at: None,
        budget_cents: None,
        spent_cents: 0,
        allowed_models: None,
        rpm_limit: None,
        tpm_limit: None,
    };
    let api_key_clone = api_key.clone();

    db.expect_get_api_key_by_hash()
        .withf(|hash: &str| hash == hash_key("test-key"))
        .times(1)
        .returning(move |_| Ok(Some(api_key_clone.clone())));

    let result = resolve_api_key(&db, "test-key").await;
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert!(resolved.is_some());
    let (team_id, key_id) = resolved.unwrap();
    assert_eq!(team_id, "team-id");
    assert_eq!(key_id, "key-id");
}

#[tokio::test]
async fn test_create_user_success() {
    use chrono::Utc;

    let mut db = MockDatabase::new();
    let now = Utc::now();
    let user = User {
        id: "new-user-id".to_string(),
        team_id: "team-id".to_string(),
        email: "new@example.com".to_string(),
        role: "member".to_string(),
        password_hash: None,
        created_at: now,
    };
    db.expect_create_user()
        .with(
            eq("team-id"),
            eq("new@example.com"),
            eq("member"),
            eq(None::<String>),
        )
        .times(1)
        .returning(move |_, _, _, _| Ok(user.clone()));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let admin = RequireAdmin(AuthClaims {
        sub: "admin-123".to_string(),
        email: "admin@test.com".to_string(),
        role: "admin".to_string(),
        team_id: "team-id".to_string(),
        exp: 9999999999,
    });

    let response = create_user(
        State(state),
        admin,
        Json(CreateUserRequest {
            team_id: "team-id".to_string(),
            email: "new@example.com".to_string(),
            role: "member".to_string(),
        }),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_api_key_success() {
    use chrono::Utc;

    let mut db = MockDatabase::new();
    let now = Utc::now();

    let expected_hash = hash_key("test-secret-key");

    let api_key = ApiKey {
        id: "new-key-id".to_string(),
        key_hash: expected_hash.clone(),
        user_id: "user-id".to_string(),
        team_id: "team-id".to_string(),
        name: Some("Test Key".to_string()),
        is_active: true,
        created_at: now,
        expires_at: None,
        budget_cents: None,
        spent_cents: 0,
        allowed_models: None,
        rpm_limit: None,
        tpm_limit: None,
    };
    db.expect_create_api_key()
        .with(
            eq(expected_hash),
            eq("user-id"),
            eq("team-id"),
            eq(Some("Test Key".to_string())),
        )
        .times(1)
        .returning(move |_, _, _, _| Ok(api_key.clone()));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let admin = RequireAdmin(AuthClaims {
        sub: "admin-123".to_string(),
        email: "admin@test.com".to_string(),
        role: "admin".to_string(),
        team_id: "team-id".to_string(),
        exp: 9999999999,
    });

    let response = create_api_key(
        State(state),
        admin,
        Json(CreateApiKeyRequest {
            key: Some("test-secret-key".to_string()),
            user_id: Some("user-id".to_string()),
            team_id: "team-id".to_string(),
            name: Some("Test Key".to_string()),
        }),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_api_key_defaults_key_and_user_id() {
    use chrono::Utc;

    let mut state = create_test_state();
    let now = Utc::now();

    state
        .db
        .expect_create_api_key()
        .withf(|key_hash, user_id, team_id, name| {
            !key_hash.is_empty()
                && user_id == "admin-123"
                && team_id == "team-id"
                && name.as_deref() == Some("Dashboard Key")
        })
        .times(1)
        .returning(move |key_hash, user_id, team_id, name| {
            Ok(ApiKey {
                id: "auto-key-id".to_string(),
                key_hash: key_hash.to_string(),
                user_id: user_id.to_string(),
                team_id: team_id.to_string(),
                name,
                is_active: true,
                created_at: now,
                expires_at: None,
                budget_cents: None,
                spent_cents: 0,
                allowed_models: None,
                rpm_limit: None,
                tpm_limit: None,
            })
        });

    let admin = RequireAdmin(AuthClaims {
        sub: "admin-123".to_string(),
        email: "admin@test.com".to_string(),
        role: "admin".to_string(),
        team_id: "team-id".to_string(),
        exp: 9999999999,
    });

    let req: CreateApiKeyRequest = serde_json::from_value(serde_json::json!({
        "team_id": "team-id",
        "name": "Dashboard Key"
    }))
    .unwrap();

    let resp = create_api_key(State(state), admin, Json(req))
        .await
        .into_response();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], "auto-key-id");
    assert!(json["key"]
        .as_str()
        .map(|k| k.starts_with("hi-"))
        .unwrap_or(false));
    assert!(json.get("key_hash").is_none());
}

#[tokio::test]
async fn test_create_model_alias_success() {
    use chrono::Utc;

    let mut db = MockDatabase::new();
    let now = Utc::now();
    let alias = ModelAlias {
        id: "alias-id".to_string(),
        team_id: "team-id".to_string(),
        alias: "gpt-4-fast".to_string(),
        target_model: "gpt-4-turbo".to_string(),
        provider: "openai".to_string(),
        created_at: now,
    };
    db.expect_create_model_alias()
        .with(
            eq("team-id"),
            eq("gpt-4-fast"),
            eq("gpt-4-turbo"),
            eq("openai"),
        )
        .times(1)
        .returning(move |_, _, _, _| Ok(alias.clone()));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let admin = RequireAdmin(AuthClaims {
        sub: "admin-123".to_string(),
        email: "admin@test.com".to_string(),
        role: "admin".to_string(),
        team_id: "team-id".to_string(),
        exp: 9999999999,
    });

    let response = create_model_alias(
        State(state),
        admin,
        Json(CreateModelAliasRequest {
            team_id: "team-id".to_string(),
            alias: "gpt-4-fast".to_string(),
            target_model: "gpt-4-turbo".to_string(),
            provider: "openai".to_string(),
        }),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_quota_success() {
    use chrono::Utc;

    let mut db = MockDatabase::new();
    let now = Utc::now();
    let quota = Quota {
        id: "quota-id".to_string(),
        team_id: "team-id".to_string(),
        rpm_limit: 100,
        tpm_limit: 10000,
        updated_at: now,
    };
    db.expect_create_quota()
        .with(eq("team-id"), eq(100i32), eq(10000i32))
        .times(1)
        .returning(move |_, _, _| Ok(quota.clone()));

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let admin = RequireAdmin(AuthClaims {
        sub: "admin-123".to_string(),
        email: "admin@test.com".to_string(),
        role: "admin".to_string(),
        team_id: "team-id".to_string(),
        exp: 9999999999,
    });

    let response = create_quota(
        State(state),
        admin,
        Json(CreateQuotaRequest {
            team_id: "team-id".to_string(),
            rpm_limit: 100,
            tpm_limit: 10000,
        }),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_team_unique_violation() {
    let mut db = MockDatabase::new();
    db.expect_create_team().times(1).returning(|_, _| {
        Err(DbError::UniqueViolation(
            "Team name already exists".to_string(),
        ))
    });

    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state: AppState<MockDatabase, MockConfigStore> = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: MockConfigStore::new(),
        admin_token: Arc::new("test-token".to_string()),
        jwt_secret: Arc::new("test-jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };

    let admin = RequireAdmin(AuthClaims {
        sub: "admin-123".to_string(),
        email: "admin@test.com".to_string(),
        role: "admin".to_string(),
        team_id: "team-id".to_string(),
        exp: 9999999999,
    });

    let response = create_team(
        State(state),
        admin,
        Json(CreateTeamRequest {
            name: "Duplicate Team".to_string(),
            budget_cents: 5000,
        }),
    )
    .await;
    let resp = response.into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[test]
fn test_build_cors_layer_from_string() {
    // Test valid origins
    let valid_cors =
        build_cors_layer_from_string(Some("http://localhost:3000,https://example.com"));
    assert!(valid_cors.is_ok());

    // Test None
    let none_cors = build_cors_layer_from_string(None);
    assert!(none_cors.is_err());
    assert_eq!(
        none_cors.unwrap_err(),
        "ALLOWED_ORIGINS must be set to a non-empty value."
    );

    // Test empty string
    let empty_cors = build_cors_layer_from_string(Some(""));
    assert!(empty_cors.is_err());
    assert_eq!(
        empty_cors.unwrap_err(),
        "ALLOWED_ORIGINS must contain at least one valid origin."
    );

    // Test whitespace string
    let whitespace_cors = build_cors_layer_from_string(Some("   "));
    assert!(whitespace_cors.is_err());
    assert_eq!(
        whitespace_cors.unwrap_err(),
        "ALLOWED_ORIGINS must contain at least one valid origin."
    );
}

fn cloneable_mock_db() -> MockDatabase {
    let mut db = MockDatabase::new();
    db.expect_clone().returning(cloneable_mock_db);
    db
}

fn cloneable_mock_config_store() -> MockConfigStore {
    let mut cm = MockConfigStore::new();
    cm.expect_clone().returning(cloneable_mock_config_store);
    cm
}

#[test]
fn test_build_router_instantiates() {
    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state = AppState {
        config: Arc::new(RwLock::new(config)),
        db: cloneable_mock_db(),
        config_manager: cloneable_mock_config_store(),
        admin_token: Arc::new("admin-token".to_string()),
        jwt_secret: Arc::new("jwt-secret".to_string()),
        proxy_router: None,
        rate_limiter: None,
    };
    let mcp_state = crate::mcp::McpState::new("jwt-secret".to_string());
    let cors = build_cors_layer_from_string(Some("http://localhost:3000")).unwrap();
    let _router = build_router(state, mcp_state, cors, false);
}

#[tokio::test]
async fn test_list_teams_admin() {
    let mut state = create_test_state();
    let team = Team {
        id: "team-1".to_string(),
        name: "Test Team".to_string(),
        budget_cents: 10000,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state
        .db
        .expect_list_teams()
        .times(1)
        .returning(move || Ok(vec![team.clone()]));

    let res = list_teams(State(state), Extension(admin_claims()))
        .await
        .into_response();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_teams_non_admin() {
    let mut state = create_test_state();
    let team = Team {
        id: "team-1".to_string(),
        name: "Test Team".to_string(),
        budget_cents: 10000,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state
        .db
        .expect_get_team()
        .with(eq("team-1"))
        .times(1)
        .returning(move |_| Ok(Some(team.clone())));

    let res = list_teams(State(state), Extension(member_claims("team-1")))
        .await
        .into_response();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_api_keys_admin() {
    let mut state = create_test_state();
    state
        .db
        .expect_list_api_keys()
        .with(eq(None))
        .times(1)
        .returning(|_| Ok(vec![]));

    let res = list_api_keys(
        State(state),
        Extension(admin_claims()),
        Query(HashMap::new()),
    )
    .await
    .into_response();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_api_keys_non_admin_scoped() {
    let mut state = create_test_state();
    state
        .db
        .expect_list_api_keys()
        .with(eq(Some("team-1".to_string())))
        .times(1)
        .returning(|_| Ok(vec![]));

    let mut query = HashMap::new();
    query.insert("team_id".to_string(), "other-team".to_string());
    let res = list_api_keys(
        State(state),
        Extension(member_claims("team-1")),
        Query(query),
    )
    .await
    .into_response();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_usage_summary_admin() {
    let mut state = create_test_state();
    state
        .db
        .expect_get_usage_summary()
        .with(eq(None))
        .times(1)
        .returning(|_| Ok(UsageSummary::default()));

    let res = get_usage_summary(
        State(state),
        Extension(admin_claims()),
        Query(HashMap::new()),
    )
    .await
    .into_response();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_usage_summary_non_admin_scoped() {
    let mut state = create_test_state();
    state
        .db
        .expect_get_usage_summary()
        .with(eq(Some("team-1".to_string())))
        .times(1)
        .returning(|_| Ok(UsageSummary::default()));

    let mut query = HashMap::new();
    query.insert("team_id".to_string(), "other-team".to_string());
    let res = get_usage_summary(
        State(state),
        Extension(member_claims("team-1")),
        Query(query),
    )
    .await
    .into_response();
    assert_eq!(res.status(), StatusCode::OK);
}
