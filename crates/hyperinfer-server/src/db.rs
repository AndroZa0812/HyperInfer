use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyperinfer_core::{
    ApiKey, ConfigStore, Database, DbError, ModelAlias, PolicyUpdate, Quota, Team, UsageLog, User,
};
use serde::Serialize;
use sqlx::PgPool;

#[derive(Clone)]
pub struct SqlxDb {
    pool: PgPool,
}

impl SqlxDb {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Database for SqlxDb {
    async fn get_team(&self, id: &str) -> Result<Option<Team>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid(id.to_string()))?;
        let result: Option<TeamRow> = sqlx::query_as(
            "SELECT id, name, budget_cents, created_at, updated_at FROM teams WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(Team::from))
    }

    async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, DbError> {
        let result: TeamRow = match sqlx::query_as(
            "INSERT INTO teams (name, budget_cents) VALUES ($1, $2) RETURNING id, name, budget_cents, created_at, updated_at"
        )
        .bind(name)
        .bind(budget_cents)
        .fetch_one(&self.pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                if e.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false) {
                    return Err(DbError::UniqueViolation(format!(
                        "Team with name '{}' already exists",
                        name
                    )));
                }
                return Err(DbError::Sqlx(e));
            }
        };

        Ok(Team::from(result))
    }

    async fn get_user(&self, id: &str) -> Result<Option<User>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid(id.to_string()))?;
        let result: Option<UserRow> =
            sqlx::query_as("SELECT id, team_id, email, role, created_at FROM users WHERE id = $1")
                .bind(uuid)
                .fetch_optional(&self.pool)
                .await?;

        Ok(result.map(User::from))
    }

    async fn create_user(&self, team_id: &str, email: &str, role: &str) -> Result<User, DbError> {
        let team_uuid = uuid::Uuid::parse_str(team_id)
            .map_err(|_| DbError::InvalidUuid(team_id.to_string()))?;
        let result: UserRow = sqlx::query_as(
            "INSERT INTO users (team_id, email, role) VALUES ($1, $2, $3) RETURNING id, team_id, email, role, created_at"
        )
        .bind(team_uuid)
        .bind(email)
        .bind(role)
        .fetch_one(&self.pool)
        .await?;

        Ok(User::from(result))
    }

    async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid(id.to_string()))?;
        let result: Option<ApiKeyRow> = sqlx::query_as(
            "SELECT id, key_hash, user_id, team_id, name, is_active, created_at, expires_at FROM api_keys WHERE id = $1"
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(ApiKey::from))
    }

    async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DbError> {
        let result: Option<ApiKeyRow> = sqlx::query_as(
            "SELECT id, key_hash, user_id, team_id, name, is_active, created_at, expires_at FROM api_keys WHERE key_hash = $1 AND is_active = true"
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(ApiKey::from))
    }

    async fn create_api_key(
        &self,
        key_hash: &str,
        user_id: &str,
        team_id: &str,
        name: Option<String>,
    ) -> Result<ApiKey, DbError> {
        let user_uuid = uuid::Uuid::parse_str(user_id)
            .map_err(|_| DbError::InvalidUuid(user_id.to_string()))?;
        let team_uuid = uuid::Uuid::parse_str(team_id)
            .map_err(|_| DbError::InvalidUuid(team_id.to_string()))?;
        let result: ApiKeyRow = sqlx::query_as(
            "INSERT INTO api_keys (key_hash, user_id, team_id, name) VALUES ($1, $2, $3, $4) RETURNING id, key_hash, user_id, team_id, name, is_active, created_at, expires_at"
        )
        .bind(key_hash)
        .bind(user_uuid)
        .bind(team_uuid)
        .bind(name.as_deref())
        .fetch_one(&self.pool)
        .await?;

        Ok(ApiKey::from(result))
    }

    async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid(id.to_string()))?;
        let result: Option<ModelAliasRow> = sqlx::query_as(
            "SELECT id, team_id, alias, target_model, provider, created_at FROM model_aliases WHERE id = $1"
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(ModelAlias::from))
    }

    async fn create_model_alias(
        &self,
        team_id: &str,
        alias: &str,
        target_model: &str,
        provider: &str,
    ) -> Result<ModelAlias, DbError> {
        let team_uuid = uuid::Uuid::parse_str(team_id)
            .map_err(|_| DbError::InvalidUuid(team_id.to_string()))?;
        let result: ModelAliasRow = sqlx::query_as(
            "INSERT INTO model_aliases (team_id, alias, target_model, provider) VALUES ($1, $2, $3, $4) RETURNING id, team_id, alias, target_model, provider, created_at"
        )
        .bind(team_uuid)
        .bind(alias)
        .bind(target_model)
        .bind(provider)
        .fetch_one(&self.pool)
        .await?;

        Ok(ModelAlias::from(result))
    }

    async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError> {
        let uuid = uuid::Uuid::parse_str(team_id)
            .map_err(|_| DbError::InvalidUuid(team_id.to_string()))?;
        let result: Option<QuotaRow> = sqlx::query_as(
            "SELECT id, team_id, rpm_limit, tpm_limit, updated_at FROM quotas WHERE team_id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(Quota::from))
    }

    async fn create_quota(
        &self,
        team_id: &str,
        rpm_limit: i32,
        tpm_limit: i32,
    ) -> Result<Quota, DbError> {
        let team_uuid = uuid::Uuid::parse_str(team_id)
            .map_err(|_| DbError::InvalidUuid(team_id.to_string()))?;
        let result: QuotaRow = sqlx::query_as(
            "INSERT INTO quotas (team_id, rpm_limit, tpm_limit) VALUES ($1, $2, $3) RETURNING id, team_id, rpm_limit, tpm_limit, updated_at"
        )
        .bind(team_uuid)
        .bind(rpm_limit)
        .bind(tpm_limit)
        .fetch_one(&self.pool)
        .await?;

        Ok(Quota::from(result))
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
        let team_uuid = uuid::Uuid::parse_str(team_id)
            .map_err(|_| DbError::InvalidUuid(team_id.to_string()))?;
        let api_key_uuid = uuid::Uuid::parse_str(api_key_id)
            .map_err(|_| DbError::InvalidUuid(api_key_id.to_string()))?;

        let result: UsageLogRow = sqlx::query_as(
            "INSERT INTO usage_logs (team_id, api_key_id, model, input_tokens, output_tokens, response_time_ms) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id, team_id, api_key_id, model, input_tokens, output_tokens, response_time_ms, recorded_at"
        )
        .bind(team_uuid)
        .bind(api_key_uuid)
        .bind(model)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(response_time_ms)
        .fetch_one(&self.pool)
        .await?;

        Ok(UsageLog::from(result))
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct TeamRow {
    id: uuid::Uuid,
    name: String,
    budget_cents: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<TeamRow> for Team {
    fn from(row: TeamRow) -> Self {
        Team {
            id: row.id.to_string(),
            name: row.name,
            budget_cents: row.budget_cents,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct UserRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    email: String,
    role: String,
    created_at: DateTime<Utc>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User {
            id: row.id.to_string(),
            team_id: row.team_id.to_string(),
            email: row.email,
            role: row.role,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct ApiKeyRow {
    id: uuid::Uuid,
    key_hash: String,
    user_id: uuid::Uuid,
    team_id: uuid::Uuid,
    name: Option<String>,
    is_active: bool,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}

impl From<ApiKeyRow> for ApiKey {
    fn from(row: ApiKeyRow) -> Self {
        ApiKey {
            id: row.id.to_string(),
            key_hash: row.key_hash,
            user_id: row.user_id.to_string(),
            team_id: row.team_id.to_string(),
            name: row.name,
            is_active: row.is_active,
            created_at: row.created_at,
            expires_at: row.expires_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct ModelAliasRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    alias: String,
    target_model: String,
    provider: String,
    created_at: DateTime<Utc>,
}

impl From<ModelAliasRow> for ModelAlias {
    fn from(row: ModelAliasRow) -> Self {
        ModelAlias {
            id: row.id.to_string(),
            team_id: row.team_id.to_string(),
            alias: row.alias,
            target_model: row.target_model,
            provider: row.provider,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct QuotaRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    rpm_limit: i32,
    tpm_limit: i32,
    updated_at: DateTime<Utc>,
}

impl From<QuotaRow> for Quota {
    fn from(row: QuotaRow) -> Self {
        Quota {
            id: row.id.to_string(),
            team_id: row.team_id.to_string(),
            rpm_limit: row.rpm_limit,
            tpm_limit: row.tpm_limit,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct UsageLogRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    api_key_id: uuid::Uuid,
    model: String,
    input_tokens: i32,
    output_tokens: i32,
    response_time_ms: i64,
    recorded_at: DateTime<Utc>,
}

impl From<UsageLogRow> for UsageLog {
    fn from(row: UsageLogRow) -> Self {
        UsageLog {
            id: row.id.to_string(),
            team_id: row.team_id.to_string(),
            api_key_id: row.api_key_id.to_string(),
            model: row.model,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            response_time_ms: row.response_time_ms,
            recorded_at: row.recorded_at,
        }
    }
}

#[derive(Clone)]
pub struct RedisConfigStore {
    manager: hyperinfer_core::redis::ConfigManager,
}

impl RedisConfigStore {
    pub async fn new(redis_url: &str) -> Result<Self, hyperinfer_core::ConfigError> {
        let manager = hyperinfer_core::redis::ConfigManager::new(redis_url).await?;
        Ok(Self { manager })
    }

    pub async fn subscribe_to_config_updates(
        &self,
        config: std::sync::Arc<tokio::sync::RwLock<hyperinfer_core::Config>>,
    ) -> Result<tokio::task::JoinHandle<()>, hyperinfer_core::ConfigError> {
        self.manager.subscribe_to_config_updates(config).await
    }
}

#[async_trait]
impl ConfigStore for RedisConfigStore {
    async fn fetch_config(&self) -> Result<hyperinfer_core::Config, hyperinfer_core::ConfigError> {
        self.manager.fetch_config().await
    }

    async fn publish_config_update(
        &self,
        config: &hyperinfer_core::Config,
    ) -> Result<(), hyperinfer_core::ConfigError> {
        self.manager.publish_config_update(config).await
    }

    async fn publish_policy_update(
        &self,
        update: &PolicyUpdate,
    ) -> Result<(), hyperinfer_core::ConfigError> {
        self.manager.publish_policy_update(update).await
    }
}
