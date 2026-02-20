use sqlx::PgPool;
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_team(&self, id: &str) -> Result<Option<Team>, sqlx::Error> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, Team>("SELECT id, name, budget_cents, created_at, updated_at FROM teams WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, sqlx::Error> {
        sqlx::query_as::<_, Team>(
            "INSERT INTO teams (name, budget_cents) VALUES ($1, $2) RETURNING id, name, budget_cents, created_at, updated_at"
        )
        .bind(name)
        .bind(budget_cents)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_user(&self, id: &str) -> Result<Option<User>, sqlx::Error> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, User>("SELECT id, team_id, email, role, created_at FROM users WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create_user(&self, team_id: &str, email: &str, role: &str) -> Result<User, sqlx::Error> {
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, User>(
            "INSERT INTO users (team_id, email, role) VALUES ($1, $2, $3) RETURNING id, team_id, email, role, created_at"
        )
        .bind(team_uuid)
        .bind(email)
        .bind(role)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, sqlx::Error> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, ApiKey>("SELECT id, key_hash, user_id, team_id, name, is_active, created_at, expires_at FROM api_keys WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create_api_key(&self, key_hash: &str, user_id: &str, team_id: &str, name: Option<&str>) -> Result<ApiKey, sqlx::Error> {
        let user_uuid = uuid::Uuid::parse_str(user_id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, ApiKey>(
            "INSERT INTO api_keys (key_hash, user_id, team_id, name) VALUES ($1, $2, $3, $4) RETURNING id, key_hash, user_id, team_id, name, is_active, created_at, expires_at"
        )
        .bind(key_hash)
        .bind(user_uuid)
        .bind(team_uuid)
        .bind(name)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, sqlx::Error> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, ModelAlias>("SELECT id, team_id, alias, target_model, provider, created_at FROM model_aliases WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create_model_alias(&self, team_id: &str, alias: &str, target_model: &str, provider: &str) -> Result<ModelAlias, sqlx::Error> {
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, ModelAlias>(
            "INSERT INTO model_aliases (team_id, alias, target_model, provider) VALUES ($1, $2, $3, $4) RETURNING id, team_id, alias, target_model, provider, created_at"
        )
        .bind(team_uuid)
        .bind(alias)
        .bind(target_model)
        .bind(provider)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, sqlx::Error> {
        let uuid = uuid::Uuid::parse_str(team_id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, Quota>("SELECT id, team_id, rpm_limit, tpm_limit, budget_cents, updated_at FROM quotas WHERE team_id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create_quota(&self, team_id: &str, rpm_limit: i32, tpm_limit: i32, budget_cents: i64) -> Result<Quota, sqlx::Error> {
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| sqlx::Error::Protocol("Invalid UUID".into()))?;
        sqlx::query_as::<_, Quota>(
            "INSERT INTO quotas (team_id, rpm_limit, tpm_limit, budget_cents) VALUES ($1, $2, $3, $4) RETURNING id, team_id, rpm_limit, tpm_limit, budget_cents, updated_at"
        )
        .bind(team_uuid)
        .bind(rpm_limit)
        .bind(tpm_limit)
        .bind(budget_cents)
        .fetch_one(&self.pool)
        .await
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Team {
    pub id: uuid::Uuid,
    pub name: String,
    pub budget_cents: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct User {
    pub id: uuid::Uuid,
    pub team_id: uuid::Uuid,
    pub email: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ApiKey {
    pub id: uuid::Uuid,
    pub key_hash: String,
    pub user_id: uuid::Uuid,
    pub team_id: uuid::Uuid,
    pub name: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ModelAlias {
    pub id: uuid::Uuid,
    pub team_id: uuid::Uuid,
    pub alias: String,
    pub target_model: String,
    pub provider: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Quota {
    pub id: uuid::Uuid,
    pub team_id: uuid::Uuid,
    pub rpm_limit: i32,
    pub tpm_limit: i32,
    pub budget_cents: i64,
    pub updated_at: DateTime<Utc>,
}
