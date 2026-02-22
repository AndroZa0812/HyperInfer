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
    /// Creates a new SqlxDb that uses the provided Postgres connection pool.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlx::PgPool;
    /// // Create a lazy connection pool (does not establish network connections immediately).
    /// let pool = PgPool::connect_lazy("postgres://user:password@localhost/dbname");
    /// let db = SqlxDb::new(pool);
    /// ```
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Database for SqlxDb {
    /// Fetches a team by its UUID string.
    ///
    /// Attempts to parse `id` as a UUID; if parsing fails this returns `DbError::InvalidUuid`.
    ///
    /// # Arguments
    ///
    /// * `id` - The team's UUID string.
    ///
    /// # Returns
    ///
    /// `Some(Team)` if a team with the given id exists, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(db: &SqlxDb) -> Result<(), Box<dyn std::error::Error>> {
    /// let maybe = db.get_team("550e8400-e29b-41d4-a716-446655440000").await?;
    /// if let Some(team) = maybe {
    ///     println!("{}", team.name);
    /// }
    /// # Ok(()) }
    /// ```
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

    /// Creates a new team record with the specified name and budget and returns the created team.
    ///
    /// The returned `Team` is populated with the database-assigned `id` and the `created_at` / `updated_at` timestamps.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // assuming `db` is a ready `SqlxDb` instance connected to the database
    /// let team = db.create_team("Acme Corp", 1_000_00).await.unwrap();
    /// assert_eq!(team.name, "Acme Corp");
    /// ```
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

    /// Fetches a user by UUID string and maps the database row to a domain `User`.
    ///
    /// The `id` parameter must be a UUID string; if a matching row is found it is converted
    /// into a `User` with stringified UUID fields.
    ///
    /// # Arguments
    ///
    /// * `id` - UUID string identifying the user to fetch.
    ///
    /// # Returns
    ///
    /// `Some(User)` if a user with the given id exists, `None` if no matching row is found.
    ///
    /// # Errors
    ///
    /// Returns `DbError::InvalidUuid` if `id` is not a valid UUID. Other database errors are
    /// returned as `DbError` variants.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(db: &SqlxDb) -> Result<(), DbError> {
    /// let maybe_user = db.get_user("00000000-0000-0000-0000-000000000000").await?;
    /// if let Some(user) = maybe_user {
    ///     assert_eq!(user.id, "00000000-0000-0000-0000-000000000000");
    /// }
    /// # Ok(())
    /// # }
    /// ```
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

    /// Creates a new user associated with the given team.
    ///
    /// The `team_id` must be a UUID string; the function inserts a row into `users` and returns
    /// the newly created `User` model populated from the database `RETURNING` values.
    ///
    /// Returns `DbError::InvalidUuid(team_id.to_string())` if `team_id` is not a valid UUID.
    /// Other database failures are returned as `DbError`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use hyperinfer_server::db::SqlxDb;
    /// # use hyperinfer_core::db::User;
    /// # async fn example(db: &SqlxDb) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = db.create_user("550e8400-e29b-41d4-a716-446655440000", "alice@example.com", "member").await?;
    /// println!("created user id = {}", user.id);
    /// # Ok(()) }
    /// ```
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

    /// Fetches an API key by its UUID string and returns the corresponding `ApiKey` when found.
    ///
    /// Returns `Err(DbError::InvalidUuid(_))` if `id` is not a valid UUID string. Database failures
    /// are returned as other `DbError` variants.
    ///
    /// # Returns
    ///
    /// `Some(ApiKey)` if a matching API key exists, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use crates::db::SqlxDb; // adjust path to your SqlxDb type
    /// # async fn _example(db: &SqlxDb) -> Result<(), Box<dyn std::error::Error>> {
    /// let maybe_key = db.get_api_key("3fa85f64-5717-4562-b3fc-2c963f66afa6").await?;
    /// if let Some(api_key) = maybe_key {
    ///     println!("found api key: {}", api_key.id);
    /// }
    /// # Ok(()) }
    /// ```
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

    /// Create a new API key record associated with the given user and team.
    ///
    /// Parses `user_id` and `team_id` as UUIDs, inserts a new row into `api_keys`, and returns the created `ApiKey`.
    ///
    /// # Errors
    ///
    /// - `DbError::InvalidUuid` if `user_id` or `team_id` is not a valid UUID.
    /// - Other `DbError` variants may be returned for database-related failures.
    ///
    /// # Examples
    ///
    /// ```
    /// // Example (async context):
    /// // let api_key = db.create_api_key(
    /// //     "hashed_value",
    /// //     "00000000-0000-0000-0000-000000000000",
    /// //     "00000000-0000-0000-0000-000000000001",
    /// //     Some("my key".to_string()),
    /// // ).await?;
    /// // assert_eq!(api_key.name.as_deref(), Some("my key"));
    /// ```
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

    /// Fetches a model alias by its UUID string.
    ///
    /// Parses `id` as a UUID and returns the corresponding `ModelAlias` if found.
    ///
    /// # Returns
    ///
    /// `Some(ModelAlias)` if a row with the given UUID exists, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use futures::executor::block_on;
    /// # use crate::{SqlxDb, DbError};
    /// # let db: SqlxDb = todo!();
    /// let alias = block_on(db.get_model_alias("3fa85f64-5717-4562-b3fc-2c963f66afa6"));
    /// match alias {
    ///     Ok(Some(model_alias)) => println!("Found alias: {}", model_alias.alias),
    ///     Ok(None) => println!("No alias found"),
    ///     Err(e) => eprintln!("DB error: {:?}", e),
    /// }
    /// ```
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

    /// Creates a new model alias for a team.
    ///
    /// On success returns the created `ModelAlias` with its `id` and `team_id` as strings and the `created_at` timestamp populated.
    /// Returns `DbError::InvalidUuid` if `team_id` is not a valid UUID; other database failures are returned as other `DbError` variants.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::str::FromStr;
    /// # async fn run_example() -> Result<(), Box<dyn std::error::Error>> {
    /// let db: crate::SqlxDb = unimplemented!(); // obtain a configured SqlxDb
    /// let created = db
    ///     .create_model_alias("550e8400-e29b-41d4-a716-446655440000", "my-alias", "gpt-4", "openai")
    ///     .await?;
    /// assert_eq!(created.alias, "my-alias");
    /// assert_eq!(created.target_model, "gpt-4");
    /// # Ok(()) }
    /// ```
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

    /// Fetches the quota record for the given team UUID string.
    ///
    /// Parses `team_id` as a UUID and returns the associated `Quota` if one exists for that team.
    /// Returns `Err(DbError::InvalidUuid(_))` when `team_id` is not a valid UUID string.
    ///
    /// # Returns
    ///
    /// `Some(Quota)` with the team's quota when found, `None` if no quota exists for the team.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use hyperinfer_server::db::SqlxDb;
    /// # use hyperinfer_core::DbError;
    /// # async fn example(db: &SqlxDb) -> Result<(), DbError> {
    /// let team_id = "3fa85f64-5717-4562-b3fc-2c963f66afa6";
    /// let quota_opt = db.get_quota(team_id).await?;
    /// if let Some(quota) = quota_opt {
    ///     println!("RPM limit: {}", quota.rpm_limit);
    /// }
    /// # Ok(())
    /// # }
    /// ```
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

    /// Creates a quota record for the specified team and returns the persisted Quota.
    ///
    /// The `team_id` argument must be a UUID string; if parsing fails the call returns `DbError::InvalidUuid`.
    ///
    /// # Returns
    ///
    /// `Quota` containing the inserted row's fields: `id` and `team_id` as strings, `rpm_limit`, `tpm_limit`, and `updated_at`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hyperinfer_server::db::SqlxDb;
    /// # use hyperinfer_core::models::Quota;
    /// # async fn _example(db: &SqlxDb) {
    /// let quota: Quota = db.create_quota("3fa85f64-5717-4562-b3fc-2c963f66afa6", 100, 1000).await.unwrap();
    /// assert_eq!(quota.rpm_limit, 100);
    /// assert_eq!(quota.tpm_limit, 1000);
    /// # }
    /// ```
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

#[derive(Clone)]
pub struct RedisConfigStore {
    manager: hyperinfer_core::redis::ConfigManager,
}

impl RedisConfigStore {
    /// Creates a Redis-backed configuration store by initializing a `ConfigManager` with the given Redis URL.
    ///
    /// # Parameters
    ///
    /// - `redis_url`: Redis connection URL (for example, `redis://localhost:6379`).
    ///
    /// # Returns
    ///
    /// `Ok(RedisConfigStore)` containing an initialized manager on success, or `Err(hyperinfer_core::ConfigError::Other(_))` with a stringified error message if initialization fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn doc() -> Result<(), Box<dyn std::error::Error>> {
    /// let store = RedisConfigStore::new("redis://localhost:6379").await?;
    /// // use `store`...
    /// # Ok(()) }
    /// ```
    pub async fn new(redis_url: &str) -> Result<Self, hyperinfer_core::ConfigError> {
        let manager = hyperinfer_core::redis::ConfigManager::new(redis_url)
            .await
            .map_err(|e| hyperinfer_core::ConfigError::Other(e.to_string()))?;
        Ok(Self { manager })
    }
}

// TODO: ConfigManager returns Box<dyn Error>, so all errors are mapped to ConfigError::Other.
// Consider updating ConfigManager to return ConfigError directly for proper error variant
// propagation (Redis vs Serialization errors).
#[async_trait]
impl ConfigStore for RedisConfigStore {
    /// Fetches the current configuration from Redis.
    ///
    /// On failure, converts the underlying manager error into `hyperinfer_core::ConfigError::Other` using the error's string representation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// #[tokio::test]
    /// async fn fetch_config_example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let store = RedisConfigStore::new("redis://127.0.0.1/").await?;
    ///     let config = store.fetch_config().await?;
    ///     // use `config`...
    ///     let _ = config;
    ///     Ok(())
    /// }
    /// ```
    async fn fetch_config(&self) -> Result<hyperinfer_core::Config, hyperinfer_core::ConfigError> {
        self.manager
            .fetch_config()
            .await
            .map_err(|e| hyperinfer_core::ConfigError::Other(e.to_string()))
    }

    /// Publishes a configuration update to the underlying config manager.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn run() -> Result<(), hyperinfer_core::ConfigError> {
    /// // `store` is a RedisConfigStore created via `RedisConfigStore::new`.
    /// let store = /* RedisConfigStore::new(...).await? */ unimplemented!();
    /// let config = hyperinfer_core::Config::default();
    /// store.publish_config_update(&config).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn publish_config_update(
        &self,
        config: &hyperinfer_core::Config,
    ) -> Result<(), hyperinfer_core::ConfigError> {
        self.manager
            .publish_config_update(config)
            .await
            .map_err(|e| hyperinfer_core::ConfigError::Other(e.to_string()))
    }

    /// Publishes a policy update to the configured Redis-backed ConfigManager.
    ///
    /// On success the update is published and the method returns `Ok(())`. If the underlying
    /// manager fails the error is converted to `hyperinfer_core::ConfigError::Other` containing
    /// the manager's error message.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # async fn example(store: &RedisConfigStore, update: &PolicyUpdate) {
    /// store.publish_policy_update(update).await?;
    /// # }
    /// ```
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