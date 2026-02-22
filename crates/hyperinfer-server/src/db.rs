use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyperinfer_core::{
    ApiKey, ConfigStore, Database, DbError, ModelAlias, PolicyUpdate, Quota, Team, User,
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

        Ok(result.map(|r| Team {
            id: r.id.to_string(),
            name: r.name,
            budget_cents: r.budget_cents,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, DbError> {
        let result: TeamRow = sqlx::query_as(
            "INSERT INTO teams (name, budget_cents) VALUES ($1, $2) RETURNING id, name, budget_cents, created_at, updated_at"
        )
        .bind(name)
        .bind(budget_cents)
        .fetch_one(&self.pool)
        .await?;

        Ok(Team {
            id: result.id.to_string(),
            name: result.name,
            budget_cents: result.budget_cents,
            created_at: result.created_at,
            updated_at: result.updated_at,
        })
    }

    async fn get_user(&self, id: &str) -> Result<Option<User>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid(id.to_string()))?;
        let result: Option<UserRow> =
            sqlx::query_as("SELECT id, team_id, email, role, created_at FROM users WHERE id = $1")
                .bind(uuid)
                .fetch_optional(&self.pool)
                .await?;

        Ok(result.map(|r| User {
            id: r.id.to_string(),
            team_id: r.team_id.to_string(),
            email: r.email,
            role: r.role,
            created_at: r.created_at,
        }))
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

        Ok(User {
            id: result.id.to_string(),
            team_id: result.team_id.to_string(),
            email: result.email,
            role: result.role,
            created_at: result.created_at,
        })
    }

    async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid(id.to_string()))?;
        let result: Option<ApiKeyRow> = sqlx::query_as(
            "SELECT id, key_hash, user_id, team_id, name, is_active, created_at, expires_at FROM api_keys WHERE id = $1"
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|r| ApiKey {
            id: r.id.to_string(),
            key_hash: r.key_hash,
            user_id: r.user_id.to_string(),
            team_id: r.team_id.to_string(),
            name: r.name,
            is_active: r.is_active,
            created_at: r.created_at,
            expires_at: r.expires_at,
        }))
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

        Ok(ApiKey {
            id: result.id.to_string(),
            key_hash: result.key_hash,
            user_id: result.user_id.to_string(),
            team_id: result.team_id.to_string(),
            name: result.name,
            is_active: result.is_active,
            created_at: result.created_at,
            expires_at: result.expires_at,
        })
    }

    async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid(id.to_string()))?;
        let result: Option<ModelAliasRow> = sqlx::query_as(
            "SELECT id, team_id, alias, target_model, provider, created_at FROM model_aliases WHERE id = $1"
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|r| ModelAlias {
            id: r.id.to_string(),
            team_id: r.team_id.to_string(),
            alias: r.alias,
            target_model: r.target_model,
            provider: r.provider,
            created_at: r.created_at,
        }))
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

        Ok(ModelAlias {
            id: result.id.to_string(),
            team_id: result.team_id.to_string(),
            alias: result.alias,
            target_model: result.target_model,
            provider: result.provider,
            created_at: result.created_at,
        })
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

        Ok(result.map(|r| Quota {
            id: r.id.to_string(),
            team_id: r.team_id.to_string(),
            rpm_limit: r.rpm_limit,
            tpm_limit: r.tpm_limit,
            updated_at: r.updated_at,
        }))
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

        Ok(Quota {
            id: result.id.to_string(),
            team_id: result.team_id.to_string(),
            rpm_limit: result.rpm_limit,
            tpm_limit: result.tpm_limit,
            updated_at: result.updated_at,
        })
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

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct UserRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    email: String,
    role: String,
    created_at: DateTime<Utc>,
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

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct ModelAliasRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    alias: String,
    target_model: String,
    provider: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct QuotaRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    rpm_limit: i32,
    tpm_limit: i32,
    updated_at: DateTime<Utc>,
}

pub struct RedisConfigStore {
    manager: hyperinfer_core::redis::ConfigManager,
}

impl RedisConfigStore {
    pub async fn new(redis_url: &str) -> Result<Self, hyperinfer_core::ConfigError> {
        let manager = hyperinfer_core::redis::ConfigManager::new(redis_url)
            .await
            .map_err(|e| hyperinfer_core::ConfigError::Other(e.to_string()))?;
        Ok(Self { manager })
    }
}

#[async_trait]
impl ConfigStore for RedisConfigStore {
    async fn fetch_config(&self) -> Result<hyperinfer_core::Config, hyperinfer_core::ConfigError> {
        self.manager
            .fetch_config()
            .await
            .map_err(|e| hyperinfer_core::ConfigError::Other(e.to_string()))
    }

    async fn publish_config_update(
        &self,
        config: &hyperinfer_core::Config,
    ) -> Result<(), hyperinfer_core::ConfigError> {
        self.manager
            .publish_config_update(config)
            .await
            .map_err(|e| hyperinfer_core::ConfigError::Other(e.to_string()))
    }

    async fn publish_policy_update(
        &self,
        update: &PolicyUpdate,
    ) -> Result<(), hyperinfer_core::ConfigError> {
        self.manager
            .publish_policy_update(update)
            .await
            .map_err(|e| hyperinfer_core::ConfigError::Other(e.to_string()))
    }
}

impl Clone for RedisConfigStore {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
        }
    }
}
