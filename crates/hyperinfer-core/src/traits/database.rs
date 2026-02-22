use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DbError;

#[async_trait]
pub trait Database: Clone + Send + Sync + 'static {
    async fn get_team(&self, id: &str) -> Result<Option<Team>, DbError>;
    async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, DbError>;
    async fn get_user(&self, id: &str) -> Result<Option<User>, DbError>;
    async fn create_user(&self, team_id: &str, email: &str, role: &str) -> Result<User, DbError>;
    async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError>;
    async fn create_api_key(
        &self,
        key_hash: &str,
        user_id: &str,
        team_id: &str,
        name: Option<String>,
    ) -> Result<ApiKey, DbError>;
    async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError>;
    async fn create_model_alias(
        &self,
        team_id: &str,
        alias: &str,
        target_model: &str,
        provider: &str,
    ) -> Result<ModelAlias, DbError>;
    async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError>;
    async fn create_quota(
        &self,
        team_id: &str,
        rpm_limit: i32,
        tpm_limit: i32,
    ) -> Result<Quota, DbError>;
    async fn record_usage(
        &self,
        team_id: &str,
        api_key_id: &str,
        model: &str,
        input_tokens: i32,
        output_tokens: i32,
        response_time_ms: i64,
    ) -> Result<UsageLog, DbError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub budget_cents: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub team_id: String,
    pub email: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub key_hash: String,
    pub user_id: String,
    pub team_id: String,
    pub name: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAlias {
    pub id: String,
    pub team_id: String,
    pub alias: String,
    pub target_model: String,
    pub provider: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quota {
    pub id: String,
    pub team_id: String,
    pub rpm_limit: i32,
    pub tpm_limit: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageLog {
    pub id: String,
    pub team_id: String,
    pub api_key_id: String,
    pub model: String,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub response_time_ms: i64,
    pub recorded_at: DateTime<Utc>,
}
