use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString},
    Argon2, PasswordVerifier,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyperinfer_core::{
    ApiKey, ConfigStore, CreateDeploymentRequest, CreateMemoryRequest, Database, DbError,
    Deployment, ListMemoriesFilter, Memory, MemorySearchResult, MemoryUpdateEvent, ModelAlias,
    NewUsageLog, PolicyUpdate, Quota, RoutingConfig, SearchMemoriesRequest, Team,
    UpdateMemoryRequest, UpdateRoutingConfigRequest, UsageLog, UsageSummary, User,
};
use serde::Serialize;
use sqlx::PgPool;

pub fn hash_password(password: &str) -> Result<String, DbError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| DbError::Sqlx(sqlx::Error::Protocol(e.to_string())))?
        .to_string();
    Ok(password_hash)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed_hash = PasswordHash::new(hash).ok();
    if let Some(ph) = parsed_hash {
        Argon2::default()
            .verify_password(password.as_bytes(), &ph)
            .is_ok()
    } else {
        false
    }
}

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
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
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
                    return Err(DbError::UniqueViolation("Resource already exists".to_string()));
                }
                return Err(DbError::Sqlx(e));
            }
        };

        Ok(Team::from(result))
    }

    async fn get_user(&self, id: &str) -> Result<Option<User>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result: Option<UserRow> = sqlx::query_as(
            "SELECT id, team_id, email, role, password_hash, created_at FROM users WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(User::from))
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, DbError> {
        let result: Option<UserRow> = sqlx::query_as(
            "SELECT id, team_id, email, role, password_hash, created_at FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(User::from))
    }

    async fn create_user(
        &self,
        team_id: &str,
        email: &str,
        role: &str,
        password_hash: Option<String>,
    ) -> Result<User, DbError> {
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;
        let result: UserRow = sqlx::query_as(
            "INSERT INTO users (team_id, email, role, password_hash) VALUES ($1, $2, $3, $4) RETURNING id, team_id, email, role, password_hash, created_at"
        )
        .bind(team_uuid)
        .bind(email)
        .bind(role)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false) {
                DbError::UniqueViolation("Resource already exists".to_string())
            } else {
                DbError::Sqlx(e)
            }
        })?;

        Ok(User::from(result))
    }

    async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result: Option<ApiKeyRow> = sqlx::query_as(
            "SELECT id, key_hash, user_id, team_id, name, is_active, created_at, expires_at, budget_cents, spent_cents, allowed_models, rpm_limit, tpm_limit FROM api_keys WHERE id = $1"
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(ApiKey::from))
    }

    async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DbError> {
        let result: Option<ApiKeyRow> = sqlx::query_as(
            "SELECT id, key_hash, user_id, team_id, name, is_active, created_at, expires_at, budget_cents, spent_cents, allowed_models, rpm_limit, tpm_limit FROM api_keys WHERE key_hash = $1 AND is_active = true"
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
        let user_uuid = uuid::Uuid::parse_str(user_id).map_err(|_| DbError::InvalidUuid)?;
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;
        let result: ApiKeyRow = sqlx::query_as(
            "INSERT INTO api_keys (key_hash, user_id, team_id, name) VALUES ($1, $2, $3, $4) RETURNING id, key_hash, user_id, team_id, name, is_active, created_at, expires_at, budget_cents, spent_cents, allowed_models, rpm_limit, tpm_limit"
        )
        .bind(key_hash)
        .bind(user_uuid)
        .bind(team_uuid)
        .bind(name.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false) {
                DbError::UniqueViolation("Resource already exists".to_string())
            } else {
                DbError::Sqlx(e)
            }
        })?;

        Ok(ApiKey::from(result))
    }

    async fn deactivate_api_key(&self, id: &str) -> Result<ApiKey, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result: ApiKeyRow = sqlx::query_as(
            "UPDATE api_keys SET is_active = false WHERE id = $1 RETURNING id, key_hash, user_id, team_id, name, is_active, created_at, expires_at, budget_cents, spent_cents, allowed_models, rpm_limit, tpm_limit"
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?;

        Ok(ApiKey::from(result))
    }

    async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
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
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;
        let result: ModelAliasRow = sqlx::query_as(
            "INSERT INTO model_aliases (team_id, alias, target_model, provider) VALUES ($1, $2, $3, $4) RETURNING id, team_id, alias, target_model, provider, created_at"
        )
        .bind(team_uuid)
        .bind(alias)
        .bind(target_model)
        .bind(provider)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false) {
                DbError::UniqueViolation("Resource already exists".to_string())
            } else {
                DbError::Sqlx(e)
            }
        })?;

        Ok(ModelAlias::from(result))
    }

    async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError> {
        let uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;
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
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;
        let result: QuotaRow = sqlx::query_as(
            "INSERT INTO quotas (team_id, rpm_limit, tpm_limit) VALUES ($1, $2, $3) RETURNING id, team_id, rpm_limit, tpm_limit, updated_at"
        )
        .bind(team_uuid)
        .bind(rpm_limit)
        .bind(tpm_limit)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false) {
                DbError::UniqueViolation("Resource already exists".to_string())
            } else {
                DbError::Sqlx(e)
            }
        })?;

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
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;
        let api_key_uuid = uuid::Uuid::parse_str(api_key_id).map_err(|_| DbError::InvalidUuid)?;

        let result: UsageLogRow = sqlx::query_as(
            "INSERT INTO usage_logs (team_id, api_key_id, model, input_tokens, output_tokens, response_time_ms) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, team_id, api_key_id, model, input_tokens, output_tokens, response_time_ms, recorded_at, \
                       status_code, cached_input_tokens, reasoning_tokens, cost_cents, cache_hit, failover_used, deployment_name, stop_reason"
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

    async fn record_usage_batch(&self, entries: &[NewUsageLog]) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }

        let parsed: Vec<(uuid::Uuid, uuid::Uuid, &NewUsageLog)> = entries
            .iter()
            .map(|entry| {
                let team_uuid =
                    uuid::Uuid::parse_str(&entry.team_id).map_err(|_| DbError::InvalidUuid)?;
                let api_key_uuid =
                    uuid::Uuid::parse_str(&entry.api_key_id).map_err(|_| DbError::InvalidUuid)?;
                Ok((team_uuid, api_key_uuid, entry))
            })
            .collect::<Result<_, DbError>>()?;

        let mut builder = sqlx::QueryBuilder::new(
            "INSERT INTO usage_logs (team_id, api_key_id, model, input_tokens, output_tokens, response_time_ms) ",
        );
        builder.push_values(parsed, |mut b, (team_uuid, api_key_uuid, entry)| {
            b.push_bind(team_uuid)
                .push_bind(api_key_uuid)
                .push_bind(&entry.model)
                .push_bind(entry.input_tokens)
                .push_bind(entry.output_tokens)
                .push_bind(entry.response_time_ms);
        });

        builder.build().execute(&self.pool).await?;
        Ok(())
    }

    async fn count_users_by_role(&self, role: &str) -> Result<i64, DbError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = $1")
            .bind(role)
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    async fn update_password_hash(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), DbError> {
        let user_uuid = uuid::Uuid::parse_str(user_id).map_err(|_| DbError::InvalidUuid)?;
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(password_hash)
            .bind(user_uuid)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_deployments(
        &self,
        model: &str,
        is_active: Option<bool>,
    ) -> Result<Vec<Deployment>, DbError> {
        let result: Vec<DeploymentRow> = match is_active {
            Some(active) => sqlx::query_as(
                "SELECT id, name, provider, model, api_key_ref, base_url, is_active, weight, priority, max_tpm, max_rpm, cost_per_1k_input_tokens, cost_per_1k_output_tokens, metadata, sort_order, created_at, updated_at FROM deployments WHERE model = $1 AND is_active = $2 ORDER BY sort_order ASC, created_at ASC"
            )
            .bind(model)
            .bind(active)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query_as(
                "SELECT id, name, provider, model, api_key_ref, base_url, is_active, weight, priority, max_tpm, max_rpm, cost_per_1k_input_tokens, cost_per_1k_output_tokens, metadata, sort_order, created_at, updated_at FROM deployments WHERE model = $1 ORDER BY sort_order ASC, created_at ASC"
            )
            .bind(model)
            .fetch_all(&self.pool)
            .await?,
        };

        Ok(result.into_iter().map(Deployment::from).collect())
    }

    async fn get_deployment(&self, id: &str) -> Result<Option<Deployment>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result: Option<DeploymentRow> = sqlx::query_as(
            "SELECT id, name, provider, model, api_key_ref, base_url, is_active, weight, priority, max_tpm, max_rpm, cost_per_1k_input_tokens, cost_per_1k_output_tokens, metadata, sort_order, created_at, updated_at FROM deployments WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(Deployment::from))
    }

    async fn create_deployment(&self, req: CreateDeploymentRequest) -> Result<Deployment, DbError> {
        let result: DeploymentRow = sqlx::query_as(
            "INSERT INTO deployments (name, provider, model, api_key_ref, base_url, is_active, weight, priority, max_tpm, max_rpm, cost_per_1k_input_tokens, cost_per_1k_output_tokens, metadata, sort_order) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING id, name, provider, model, api_key_ref, base_url, is_active, weight, priority, max_tpm, max_rpm, cost_per_1k_input_tokens, cost_per_1k_output_tokens, metadata, sort_order, created_at, updated_at"
        )
        .bind(&req.name)
        .bind(&req.provider)
        .bind(&req.model)
        .bind(req.api_key_ref.as_deref().unwrap_or(""))
        .bind(&req.base_url)
        .bind(req.is_active)
        .bind(req.weight)
        .bind(req.priority)
        .bind(req.max_tpm)
        .bind(req.max_rpm)
        .bind(req.cost_per_1k_input_tokens)
        .bind(req.cost_per_1k_output_tokens)
        .bind(req.metadata.unwrap_or(serde_json::json!({})))
        .bind(req.sort_order.unwrap_or(0))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .map(|db| db.is_unique_violation())
                .unwrap_or(false)
            {
                DbError::UniqueViolation("Resource already exists".to_string())
            } else {
                DbError::Sqlx(e)
            }
        })?;

        Ok(Deployment::from(result))
    }

    async fn update_deployment(
        &self,
        id: &str,
        req: CreateDeploymentRequest,
    ) -> Result<Deployment, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result: DeploymentRow = sqlx::query_as(
            "UPDATE deployments SET name = $1, provider = $2, model = $3, api_key_ref = $4, base_url = $5, is_active = $6, weight = $7, priority = $8, max_tpm = $9, max_rpm = $10, cost_per_1k_input_tokens = $11, cost_per_1k_output_tokens = $12, metadata = $13, sort_order = $14 WHERE id = $15 RETURNING id, name, provider, model, api_key_ref, base_url, is_active, weight, priority, max_tpm, max_rpm, cost_per_1k_input_tokens, cost_per_1k_output_tokens, metadata, sort_order, created_at, updated_at"
        )
        .bind(&req.name)
        .bind(&req.provider)
        .bind(&req.model)
        .bind(req.api_key_ref.as_deref().unwrap_or(""))
        .bind(&req.base_url)
        .bind(req.is_active)
        .bind(req.weight)
        .bind(req.priority)
        .bind(req.max_tpm)
        .bind(req.max_rpm)
        .bind(req.cost_per_1k_input_tokens)
        .bind(req.cost_per_1k_output_tokens)
        .bind(req.metadata.unwrap_or(serde_json::json!({})))
        .bind(req.sort_order.unwrap_or(0))
        .bind(uuid)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .map(|db| db.is_unique_violation())
                .unwrap_or(false)
            {
                DbError::UniqueViolation("Resource already exists".to_string())
            } else {
                DbError::Sqlx(e)
            }
        })?;

        Ok(Deployment::from(result))
    }

    async fn delete_deployment(&self, id: &str) -> Result<(), DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result = sqlx::query("DELETE FROM deployments WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn get_routing_config(&self) -> Result<Option<RoutingConfig>, DbError> {
        let result: Option<RoutingConfigRow> = sqlx::query_as(
            "SELECT id, strategy, strategy_params, fallback_config, routing_groups, updated_at FROM routing_config WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(RoutingConfig::from))
    }

    async fn update_routing_config(
        &self,
        req: UpdateRoutingConfigRequest,
    ) -> Result<RoutingConfig, DbError> {
        let existing = self
            .get_routing_config()
            .await?
            .unwrap_or_else(|| RoutingConfig {
                strategy: "weighted-shuffle".to_string(),
                strategy_params: serde_json::json!({}),
                fallback_config: serde_json::json!({}),
                routing_groups: serde_json::json!([]),
                updated_at: chrono::Utc::now(),
            });

        let strategy = req.strategy.unwrap_or(existing.strategy);
        let strategy_params = req.strategy_params.unwrap_or(existing.strategy_params);
        let fallback_config = req.fallback_config.unwrap_or(existing.fallback_config);
        let routing_groups = req.routing_groups.unwrap_or(existing.routing_groups);

        let result: RoutingConfigRow = sqlx::query_as(
            "UPDATE routing_config SET strategy = $1, strategy_params = $2, fallback_config = $3, routing_groups = $4, updated_at = NOW() WHERE id = 1 RETURNING id, strategy, strategy_params, fallback_config, routing_groups, updated_at",
        )
        .bind(&strategy)
        .bind(&strategy_params)
        .bind(&fallback_config)
        .bind(&routing_groups)
        .fetch_one(&self.pool)
        .await?;

        Ok(RoutingConfig::from(result))
    }

    async fn ping(&self) -> Result<(), DbError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn list_teams(&self) -> Result<Vec<Team>, DbError> {
        let rows: Vec<TeamRow> = sqlx::query_as(
            "SELECT id, name, budget_cents, created_at, updated_at FROM teams ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Team::from).collect())
    }

    async fn list_api_keys(&self, team_id: Option<String>) -> Result<Vec<ApiKey>, DbError> {
        let rows: Vec<ApiKeyRow> = match team_id.as_deref() {
            Some(tid) => {
                let uuid = uuid::Uuid::parse_str(tid).map_err(|_| DbError::InvalidUuid)?;
                sqlx::query_as(
                    "SELECT id, key_hash, user_id, team_id, name, is_active, created_at, expires_at, budget_cents, spent_cents, allowed_models, rpm_limit, tpm_limit FROM api_keys WHERE team_id = $1 ORDER BY created_at DESC",
                )
                .bind(uuid)
                .fetch_all(&self.pool)
                .await?
            }
            None => sqlx::query_as(
                "SELECT id, key_hash, user_id, team_id, name, is_active, created_at, expires_at, budget_cents, spent_cents, allowed_models, rpm_limit, tpm_limit FROM api_keys ORDER BY created_at DESC",
            )
            .fetch_all(&self.pool)
            .await?,
        };
        Ok(rows.into_iter().map(ApiKey::from).collect())
    }

    async fn get_usage_summary(&self, team_id: Option<String>) -> Result<UsageSummary, DbError> {
        let row: (i64, i64, i64, f64) = match team_id.as_deref() {
            Some(tid) => {
                let uuid = uuid::Uuid::parse_str(tid).map_err(|_| DbError::InvalidUuid)?;
                sqlx::query_as(
                    "SELECT COUNT(*)::BIGINT, COALESCE(SUM(input_tokens), 0)::BIGINT, COALESCE(SUM(output_tokens), 0)::BIGINT, COALESCE(AVG(response_time_ms)::DOUBLE PRECISION, 0.0) FROM usage_logs WHERE team_id = $1",
                )
                .bind(uuid)
                .fetch_one(&self.pool)
                .await?
            }
            None => sqlx::query_as(
                "SELECT COUNT(*)::BIGINT, COALESCE(SUM(input_tokens), 0)::BIGINT, COALESCE(SUM(output_tokens), 0)::BIGINT, COALESCE(AVG(response_time_ms)::DOUBLE PRECISION, 0.0) FROM usage_logs",
            )
            .fetch_one(&self.pool)
            .await?,
        };
        Ok(UsageSummary {
            total_requests: row.0,
            total_input_tokens: row.1,
            total_output_tokens: row.2,
            avg_response_time_ms: row.3,
        })
    }

    async fn record_api_key_spend(&self, api_key_id: &str, cents: i64) -> Result<(), DbError> {
        let uuid = uuid::Uuid::parse_str(api_key_id).map_err(|_| DbError::InvalidUuid)?;
        sqlx::query("UPDATE api_keys SET spent_cents = spent_cents + $1 WHERE id = $2")
            .bind(cents)
            .bind(uuid)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_team(&self, id: &str, name: &str, budget_cents: i64) -> Result<Team, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result: Option<TeamRow> = sqlx::query_as(
            "UPDATE teams SET name = $2, budget_cents = $3 WHERE id = $1 RETURNING id, name, budget_cents, created_at, updated_at",
        )
        .bind(uuid)
        .bind(name)
        .bind(budget_cents)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false) {
                DbError::UniqueViolation("Team name already exists".to_string())
            } else {
                DbError::Sqlx(e)
            }
        })?;

        result.map(Team::from).ok_or(DbError::NotFound)
    }

    async fn delete_team(&self, id: &str) -> Result<(), DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let res = sqlx::query("DELETE FROM teams WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn list_users(&self, team_id: Option<String>) -> Result<Vec<User>, DbError> {
        let rows: Vec<UserRow> = match team_id.as_deref() {
            Some(tid) => {
                let uuid = uuid::Uuid::parse_str(tid).map_err(|_| DbError::InvalidUuid)?;
                sqlx::query_as(
                    "SELECT id, team_id, email, role, password_hash, created_at FROM users WHERE team_id = $1 ORDER BY created_at ASC",
                )
                .bind(uuid)
                .fetch_all(&self.pool)
                .await?
            }
            None => sqlx::query_as(
                "SELECT id, team_id, email, role, password_hash, created_at FROM users ORDER BY created_at ASC",
            )
            .fetch_all(&self.pool)
            .await?,
        };
        Ok(rows.into_iter().map(User::from).collect())
    }

    async fn update_user_role(&self, id: &str, role: &str) -> Result<User, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result: Option<UserRow> = sqlx::query_as(
            "UPDATE users SET role = $2 WHERE id = $1 RETURNING id, team_id, email, role, password_hash, created_at",
        )
        .bind(uuid)
        .bind(role)
        .fetch_optional(&self.pool)
        .await?;

        result.map(User::from).ok_or(DbError::NotFound)
    }

    async fn delete_user(&self, id: &str) -> Result<(), DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let res = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn list_model_aliases(&self, team_id: &str) -> Result<Vec<ModelAlias>, DbError> {
        let uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;
        let rows: Vec<ModelAliasRow> = sqlx::query_as(
            "SELECT id, team_id, alias, target_model, provider, created_at FROM model_aliases WHERE team_id = $1 ORDER BY created_at DESC",
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ModelAlias::from).collect())
    }

    async fn delete_model_alias(&self, id: &str) -> Result<(), DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let res = sqlx::query("DELETE FROM model_aliases WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn upsert_quota(
        &self,
        team_id: &str,
        rpm_limit: i32,
        tpm_limit: i32,
    ) -> Result<Quota, DbError> {
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;
        let result: QuotaRow = sqlx::query_as(
            "INSERT INTO quotas (team_id, rpm_limit, tpm_limit) VALUES ($1, $2, $3) \
             ON CONFLICT (team_id) DO UPDATE SET rpm_limit = EXCLUDED.rpm_limit, tpm_limit = EXCLUDED.tpm_limit \
             RETURNING id, team_id, rpm_limit, tpm_limit, updated_at",
        )
        .bind(team_uuid)
        .bind(rpm_limit)
        .bind(tpm_limit)
        .fetch_one(&self.pool)
        .await?;

        Ok(Quota::from(result))
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_api_key_limits(
        &self,
        id: &str,
        budget_cents: Option<i64>,
        allowed_models: Option<Vec<String>>,
        rpm_limit: Option<i32>,
        tpm_limit: Option<i32>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<ApiKey, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let result: Option<ApiKeyRow> = sqlx::query_as(
            "UPDATE api_keys SET budget_cents = $2, allowed_models = $3, rpm_limit = $4, tpm_limit = $5, expires_at = $6 \
             WHERE id = $1 \
             RETURNING id, key_hash, user_id, team_id, name, is_active, created_at, expires_at, budget_cents, spent_cents, allowed_models, rpm_limit, tpm_limit",
        )
        .bind(uuid)
        .bind(budget_cents)
        .bind(allowed_models)
        .bind(rpm_limit)
        .bind(tpm_limit)
        .bind(expires_at)
        .fetch_optional(&self.pool)
        .await?;

        result.map(ApiKey::from).ok_or(DbError::NotFound)
    }

    async fn create_memory(&self, req: CreateMemoryRequest) -> Result<Memory, DbError> {
        let team_uuid = uuid::Uuid::parse_str(&req.team_id).map_err(|_| DbError::InvalidUuid)?;
        let user_uuid = uuid::Uuid::parse_str(&req.user_id).map_err(|_| DbError::InvalidUuid)?;
        let supersedes_uuid = match req.supersedes_id.as_deref() {
            Some(sid) => Some(uuid::Uuid::parse_str(sid).map_err(|_| DbError::InvalidUuid)?),
            None => None,
        };

        let mut tx = self.pool.begin().await?;

        let row: MemoryRow = sqlx::query_as(
            "INSERT INTO memories (team_id, user_id, repo_id, scope, content, tags, status) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id, team_id, user_id, repo_id, scope, content, tags, status, \
                       valid_from, valid_to, superseded_by, created_at, updated_at",
        )
        .bind(team_uuid)
        .bind(user_uuid)
        .bind(&req.repo_id)
        .bind(&req.scope)
        .bind(&req.content)
        .bind(&req.tags)
        .bind(&req.status)
        .fetch_one(&mut *tx)
        .await?;

        if let Some(old_id) = supersedes_uuid {
            let updated = sqlx::query(
                "UPDATE memories \
                 SET valid_to = NOW(), status = 'expired', superseded_by = $1, updated_at = NOW() \
                 WHERE id = $2 AND team_id = $3 AND valid_to IS NULL",
            )
            .bind(row.id)
            .bind(old_id)
            .bind(team_uuid)
            .execute(&mut *tx)
            .await?;

            if updated.rows_affected() == 0 {
                return Err(DbError::NotFound);
            }
        }

        tx.commit().await?;
        Ok(Memory::from(row))
    }

    async fn get_memory(&self, id: &str) -> Result<Option<Memory>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let row: Option<MemoryRow> = sqlx::query_as(
            "SELECT id, team_id, user_id, repo_id, scope, content, tags, status, \
                    valid_from, valid_to, superseded_by, created_at, updated_at \
             FROM memories WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Memory::from))
    }

    async fn list_memories(&self, filter: ListMemoriesFilter) -> Result<Vec<Memory>, DbError> {
        let team_uuid = uuid::Uuid::parse_str(&filter.team_id).map_err(|_| DbError::InvalidUuid)?;
        let user_uuid = match filter.user_id.as_deref() {
            Some(uid) => Some(uuid::Uuid::parse_str(uid).map_err(|_| DbError::InvalidUuid)?),
            None => None,
        };

        let rows: Vec<MemoryRow> = sqlx::query_as(
            "SELECT id, team_id, user_id, repo_id, scope, content, tags, status, \
                    valid_from, valid_to, superseded_by, created_at, updated_at \
             FROM memories \
             WHERE team_id = $1 \
               AND ($2::uuid IS NULL OR scope != 'user' OR user_id = $2) \
               AND ($3::text IS NULL OR scope = $3) \
               AND ($4::text IS NULL OR repo_id = $4) \
               AND ($5::text IS NULL OR $5 = ANY(tags)) \
               AND ($6::text IS NULL OR status = $6) \
               AND ($7::bool OR (valid_to IS NULL AND status != 'expired')) \
             ORDER BY updated_at DESC \
             LIMIT $8 OFFSET $9",
        )
        .bind(team_uuid)
        .bind(user_uuid)
        .bind(&filter.scope)
        .bind(&filter.repo_id)
        .bind(&filter.tag)
        .bind(&filter.status)
        .bind(filter.include_expired)
        .bind(filter.limit)
        .bind(filter.offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Memory::from).collect())
    }

    async fn search_memories(
        &self,
        req: SearchMemoriesRequest,
    ) -> Result<Vec<MemorySearchResult>, DbError> {
        let team_uuid = uuid::Uuid::parse_str(&req.team_id).map_err(|_| DbError::InvalidUuid)?;
        let user_uuid = match req.user_id.as_deref() {
            Some(uid) => Some(uuid::Uuid::parse_str(uid).map_err(|_| DbError::InvalidUuid)?),
            None => None,
        };
        let tags_filter: Option<Vec<String>> = if req.tags.is_empty() {
            None
        } else {
            Some(req.tags.clone())
        };

        let rows: Vec<MemorySearchRow> = sqlx::query_as(
            "SELECT id, team_id, user_id, repo_id, scope, content, tags, status, \
                    valid_from, valid_to, superseded_by, created_at, updated_at, \
                    ts_rank_cd(tsv, plainto_tsquery('english', $1))::float8 AS score \
             FROM memories \
             WHERE team_id = $2 \
               AND valid_to IS NULL \
               AND status = 'active' \
               AND tsv @@ plainto_tsquery('english', $1) \
               AND ($3::uuid IS NULL OR scope != 'user' OR user_id = $3) \
               AND ($4::text IS NULL OR scope = $4) \
               AND ($5::text IS NULL OR repo_id = $5) \
               AND ($6::text[] IS NULL OR tags @> $6) \
             ORDER BY score DESC, updated_at DESC \
             LIMIT $7",
        )
        .bind(&req.query)
        .bind(team_uuid)
        .bind(user_uuid)
        .bind(&req.scope)
        .bind(&req.repo_id)
        .bind(&tags_filter)
        .bind(req.limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(MemorySearchResult::from).collect())
    }

    async fn update_memory(
        &self,
        id: &str,
        team_id: &str,
        req: UpdateMemoryRequest,
    ) -> Result<Memory, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;

        let row: Option<MemoryRow> = sqlx::query_as(
            "UPDATE memories \
             SET content = COALESCE($3, content), \
                 tags = COALESCE($4, tags), \
                 status = COALESCE($5, status), \
                 valid_to = CASE WHEN $5 = 'expired' THEN COALESCE(valid_to, NOW()) \
                                 WHEN $5 = 'active' THEN NULL \
                                 ELSE valid_to END, \
                 updated_at = NOW() \
             WHERE id = $1 AND team_id = $2 \
             RETURNING id, team_id, user_id, repo_id, scope, content, tags, status, \
                       valid_from, valid_to, superseded_by, created_at, updated_at",
        )
        .bind(uuid)
        .bind(team_uuid)
        .bind(&req.content)
        .bind(&req.tags)
        .bind(&req.status)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Memory::from).ok_or(DbError::NotFound)
    }

    async fn expire_memory(&self, id: &str, team_id: &str) -> Result<Memory, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| DbError::InvalidUuid)?;

        let row: Option<MemoryRow> = sqlx::query_as(
            "UPDATE memories \
             SET status = 'expired', valid_to = COALESCE(valid_to, NOW()), updated_at = NOW() \
             WHERE id = $1 AND team_id = $2 \
             RETURNING id, team_id, user_id, repo_id, scope, content, tags, status, \
                       valid_from, valid_to, superseded_by, created_at, updated_at",
        )
        .bind(uuid)
        .bind(team_uuid)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Memory::from).ok_or(DbError::NotFound)
    }

    async fn list_usage_logs(
        &self,
        filter: hyperinfer_core::ListUsageLogsFilter,
    ) -> Result<hyperinfer_core::UsageLogsPage, DbError> {
        let team_uuid = match filter.team_id.as_deref() {
            Some(tid) => Some(uuid::Uuid::parse_str(tid).map_err(|_| DbError::InvalidUuid)?),
            None => None,
        };
        let key_uuid = match filter.api_key_id.as_deref() {
            Some(kid) => Some(uuid::Uuid::parse_str(kid).map_err(|_| DbError::InvalidUuid)?),
            None => None,
        };
        let limit = filter.limit.clamp(1, 200);
        let offset = filter.offset.max(0);

        let mut count_qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*)::BIGINT FROM usage_logs WHERE TRUE");
        if let Some(tid) = team_uuid {
            count_qb.push(" AND team_id = ").push_bind(tid);
        }
        if let Some(kid) = key_uuid {
            count_qb.push(" AND api_key_id = ").push_bind(kid);
        }
        if let Some(ref model) = filter.model {
            count_qb
                .push(" AND model ILIKE ")
                .push_bind(format!("%{model}%"));
        }
        if let Some(code) = filter.status_code {
            count_qb.push(" AND status_code = ").push_bind(code);
        }
        if let Some(hit) = filter.cache_hit {
            count_qb.push(" AND cache_hit = ").push_bind(hit);
        }
        if let Some(from) = filter.from {
            count_qb.push(" AND recorded_at >= ").push_bind(from);
        }
        if let Some(to) = filter.to {
            count_qb.push(" AND recorded_at <= ").push_bind(to);
        }
        let (total,): (i64,) = count_qb.build_query_as().fetch_one(&self.pool).await?;

        let mut rows_qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            "SELECT id, team_id, api_key_id, model, input_tokens, output_tokens, response_time_ms, recorded_at, \
                    status_code, cached_input_tokens, reasoning_tokens, cost_cents, cache_hit, failover_used, deployment_name, stop_reason \
             FROM usage_logs WHERE TRUE",
        );
        if let Some(tid) = team_uuid {
            rows_qb.push(" AND team_id = ").push_bind(tid);
        }
        if let Some(kid) = key_uuid {
            rows_qb.push(" AND api_key_id = ").push_bind(kid);
        }
        if let Some(ref model) = filter.model {
            rows_qb
                .push(" AND model ILIKE ")
                .push_bind(format!("%{model}%"));
        }
        if let Some(code) = filter.status_code {
            rows_qb.push(" AND status_code = ").push_bind(code);
        }
        if let Some(hit) = filter.cache_hit {
            rows_qb.push(" AND cache_hit = ").push_bind(hit);
        }
        if let Some(from) = filter.from {
            rows_qb.push(" AND recorded_at >= ").push_bind(from);
        }
        if let Some(to) = filter.to {
            rows_qb.push(" AND recorded_at <= ").push_bind(to);
        }
        rows_qb
            .push(" ORDER BY recorded_at DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows: Vec<UsageLogRow> = rows_qb.build_query_as().fetch_all(&self.pool).await?;

        Ok(hyperinfer_core::UsageLogsPage {
            logs: rows.into_iter().map(UsageLog::from).collect(),
            total,
            limit,
            offset,
        })
    }

    async fn upsert_agent_session(
        &self,
        req: hyperinfer_core::UpsertAgentSessionRequest,
    ) -> Result<hyperinfer_core::AgentSession, DbError> {
        let team_uuid = uuid::Uuid::parse_str(&req.team_id).map_err(|_| DbError::InvalidUuid)?;
        let user_uuid = match req.user_id.as_deref() {
            Some(uid) if !uid.is_empty() => {
                Some(uuid::Uuid::parse_str(uid).map_err(|_| DbError::InvalidUuid)?)
            }
            _ => None,
        };
        let key_uuid = match req.api_key_id.as_deref() {
            Some(kid) if !kid.is_empty() => {
                Some(uuid::Uuid::parse_str(kid).map_err(|_| DbError::InvalidUuid)?)
            }
            _ => None,
        };
        let mcp_json = serde_json::to_value(&req.mcp_tool_calls)
            .map_err(|e| DbError::ValidationError(e.to_string()))?;
        let traj_json = serde_json::to_value(&req.trajectory)
            .map_err(|e| DbError::ValidationError(e.to_string()))?;
        let started_at = req.started_at.unwrap_or_else(Utc::now);

        let row: AgentSessionRow = sqlx::query_as(
            "INSERT INTO agent_sessions (
                session_key, team_id, user_id, api_key_id, agent_name, model, status, title,
                input_tokens, cached_input_tokens, output_tokens, reasoning_tokens,
                cost_cents, cache_savings_cents, cache_hits, total_latency_ms,
                mcp_tool_calls, trajectory, started_at, ended_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18, $19, $20
             )
             ON CONFLICT (team_id, session_key) DO UPDATE SET
                user_id = COALESCE(EXCLUDED.user_id, agent_sessions.user_id),
                api_key_id = COALESCE(EXCLUDED.api_key_id, agent_sessions.api_key_id),
                agent_name = EXCLUDED.agent_name,
                model = EXCLUDED.model,
                status = EXCLUDED.status,
                title = COALESCE(EXCLUDED.title, agent_sessions.title),
                input_tokens = EXCLUDED.input_tokens,
                cached_input_tokens = EXCLUDED.cached_input_tokens,
                output_tokens = EXCLUDED.output_tokens,
                reasoning_tokens = EXCLUDED.reasoning_tokens,
                cost_cents = EXCLUDED.cost_cents,
                cache_savings_cents = EXCLUDED.cache_savings_cents,
                cache_hits = EXCLUDED.cache_hits,
                total_latency_ms = EXCLUDED.total_latency_ms,
                mcp_tool_calls = EXCLUDED.mcp_tool_calls,
                trajectory = EXCLUDED.trajectory,
                ended_at = EXCLUDED.ended_at
             RETURNING id, session_key, team_id, user_id, api_key_id, agent_name, model, status, title,
                       input_tokens, cached_input_tokens, output_tokens, reasoning_tokens,
                       cost_cents, cache_savings_cents, cache_hits, total_latency_ms,
                       mcp_tool_calls, trajectory, started_at, ended_at",
        )
        .bind(&req.session_key)
        .bind(team_uuid)
        .bind(user_uuid)
        .bind(key_uuid)
        .bind(&req.agent_name)
        .bind(&req.model)
        .bind(&req.status)
        .bind(&req.title)
        .bind(req.input_tokens)
        .bind(req.cached_input_tokens)
        .bind(req.output_tokens)
        .bind(req.reasoning_tokens)
        .bind(req.cost_cents)
        .bind(req.cache_savings_cents)
        .bind(req.cache_hits)
        .bind(req.total_latency_ms)
        .bind(mcp_json)
        .bind(traj_json)
        .bind(started_at)
        .bind(req.ended_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(hyperinfer_core::AgentSession::from(row))
    }

    async fn list_agent_sessions(
        &self,
        filter: hyperinfer_core::ListAgentSessionsFilter,
    ) -> Result<hyperinfer_core::AgentSessionsPage, DbError> {
        let team_uuid = match filter.team_id.as_deref() {
            Some(tid) => Some(uuid::Uuid::parse_str(tid).map_err(|_| DbError::InvalidUuid)?),
            None => None,
        };
        let user_uuid = match filter.user_id.as_deref() {
            Some(uid) => Some(uuid::Uuid::parse_str(uid).map_err(|_| DbError::InvalidUuid)?),
            None => None,
        };
        let key_uuid = match filter.api_key_id.as_deref() {
            Some(kid) => Some(uuid::Uuid::parse_str(kid).map_err(|_| DbError::InvalidUuid)?),
            None => None,
        };
        let limit = filter.limit.clamp(1, 200);
        let offset = filter.offset.max(0);

        let mut count_qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*)::BIGINT FROM agent_sessions WHERE TRUE");
        if let Some(tid) = team_uuid {
            count_qb.push(" AND team_id = ").push_bind(tid);
        }
        if let Some(uid) = user_uuid {
            count_qb.push(" AND user_id = ").push_bind(uid);
        }
        if let Some(kid) = key_uuid {
            count_qb.push(" AND api_key_id = ").push_bind(kid);
        }
        if let Some(ref status) = filter.status {
            count_qb.push(" AND status = ").push_bind(status);
        }
        if let Some(ref model) = filter.model {
            count_qb
                .push(" AND model ILIKE ")
                .push_bind(format!("%{model}%"));
        }
        let (total,): (i64,) = count_qb.build_query_as().fetch_one(&self.pool).await?;

        let mut rows_qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            "SELECT id, session_key, team_id, user_id, api_key_id, agent_name, model, status, title, \
                    input_tokens, cached_input_tokens, output_tokens, reasoning_tokens, \
                    cost_cents, cache_savings_cents, cache_hits, total_latency_ms, \
                    mcp_tool_calls, '[]'::jsonb AS trajectory, started_at, ended_at \
             FROM agent_sessions WHERE TRUE",
        );
        if let Some(tid) = team_uuid {
            rows_qb.push(" AND team_id = ").push_bind(tid);
        }
        if let Some(uid) = user_uuid {
            rows_qb.push(" AND user_id = ").push_bind(uid);
        }
        if let Some(kid) = key_uuid {
            rows_qb.push(" AND api_key_id = ").push_bind(kid);
        }
        if let Some(ref status) = filter.status {
            rows_qb.push(" AND status = ").push_bind(status);
        }
        if let Some(ref model) = filter.model {
            rows_qb
                .push(" AND model ILIKE ")
                .push_bind(format!("%{model}%"));
        }
        rows_qb
            .push(" ORDER BY started_at DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows: Vec<AgentSessionRow> = rows_qb.build_query_as().fetch_all(&self.pool).await?;

        Ok(hyperinfer_core::AgentSessionsPage {
            sessions: rows
                .into_iter()
                .map(hyperinfer_core::AgentSession::from)
                .collect(),
            total,
            limit,
            offset,
        })
    }

    async fn get_agent_session(
        &self,
        id: &str,
    ) -> Result<Option<hyperinfer_core::AgentSession>, DbError> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| DbError::InvalidUuid)?;
        let row: Option<AgentSessionRow> = sqlx::query_as(
            "SELECT id, session_key, team_id, user_id, api_key_id, agent_name, model, status, title,
                    input_tokens, cached_input_tokens, output_tokens, reasoning_tokens,
                    cost_cents, cache_savings_cents, cache_hits, total_latency_ms,
                    mcp_tool_calls, trajectory, started_at, ended_at
             FROM agent_sessions WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(hyperinfer_core::AgentSession::from))
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AgentSessionRow {
    id: uuid::Uuid,
    session_key: String,
    team_id: uuid::Uuid,
    user_id: Option<uuid::Uuid>,
    api_key_id: Option<uuid::Uuid>,
    agent_name: String,
    model: String,
    status: String,
    title: Option<String>,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cost_cents: f64,
    cache_savings_cents: f64,
    cache_hits: i32,
    total_latency_ms: i64,
    mcp_tool_calls: serde_json::Value,
    trajectory: serde_json::Value,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

impl From<AgentSessionRow> for hyperinfer_core::AgentSession {
    fn from(row: AgentSessionRow) -> Self {
        Self {
            id: row.id.to_string(),
            session_key: row.session_key,
            team_id: row.team_id.to_string(),
            user_id: row.user_id.map(|u| u.to_string()),
            api_key_id: row.api_key_id.map(|u| u.to_string()),
            agent_name: row.agent_name,
            model: row.model,
            status: row.status,
            title: row.title,
            input_tokens: row.input_tokens,
            cached_input_tokens: row.cached_input_tokens,
            output_tokens: row.output_tokens,
            reasoning_tokens: row.reasoning_tokens,
            cost_cents: row.cost_cents,
            cache_savings_cents: row.cache_savings_cents,
            cache_hits: row.cache_hits,
            total_latency_ms: row.total_latency_ms,
            mcp_tool_calls: serde_json::from_value(row.mcp_tool_calls).unwrap_or_default(),
            trajectory: serde_json::from_value(row.trajectory).unwrap_or_default(),
            started_at: row.started_at,
            ended_at: row.ended_at,
        }
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
    password_hash: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User {
            id: row.id.to_string(),
            team_id: row.team_id.to_string(),
            email: row.email,
            role: row.role,
            password_hash: row.password_hash,
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
    budget_cents: Option<i64>,
    spent_cents: i64,
    allowed_models: Option<Vec<String>>,
    rpm_limit: Option<i32>,
    tpm_limit: Option<i32>,
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
            budget_cents: row.budget_cents,
            spent_cents: row.spent_cents,
            allowed_models: row.allowed_models,
            rpm_limit: row.rpm_limit,
            tpm_limit: row.tpm_limit,
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
    status_code: i16,
    cached_input_tokens: i32,
    reasoning_tokens: i32,
    cost_cents: f64,
    cache_hit: bool,
    failover_used: bool,
    deployment_name: Option<String>,
    stop_reason: Option<String>,
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
            status_code: row.status_code,
            cached_input_tokens: row.cached_input_tokens,
            reasoning_tokens: row.reasoning_tokens,
            cost_cents: row.cost_cents,
            cache_hit: row.cache_hit,
            failover_used: row.failover_used,
            deployment_name: row.deployment_name,
            stop_reason: row.stop_reason,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct DeploymentRow {
    id: uuid::Uuid,
    name: String,
    provider: String,
    model: String,
    api_key_ref: String,
    base_url: String,
    is_active: bool,
    weight: i32,
    priority: i32,
    max_tpm: Option<i32>,
    max_rpm: Option<i32>,
    cost_per_1k_input_tokens: Option<f64>,
    cost_per_1k_output_tokens: Option<f64>,
    metadata: serde_json::Value,
    sort_order: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DeploymentRow> for Deployment {
    fn from(row: DeploymentRow) -> Self {
        Deployment {
            id: row.id.to_string(),
            name: row.name,
            provider: row.provider,
            model: row.model,
            api_key_ref: row.api_key_ref,
            base_url: row.base_url,
            is_active: row.is_active,
            weight: row.weight as u32,
            priority: row.priority as u32,
            max_tpm: row.max_tpm.map(|v| v as u32),
            max_rpm: row.max_rpm.map(|v| v as u32),
            cost_per_1k_input_tokens: row.cost_per_1k_input_tokens,
            cost_per_1k_output_tokens: row.cost_per_1k_output_tokens,
            metadata: row.metadata,
            sort_order: row.sort_order as u32,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct RoutingConfigRow {
    id: i32,
    strategy: String,
    strategy_params: serde_json::Value,
    fallback_config: serde_json::Value,
    routing_groups: serde_json::Value,
    updated_at: DateTime<Utc>,
}

impl From<RoutingConfigRow> for RoutingConfig {
    fn from(row: RoutingConfigRow) -> Self {
        RoutingConfig {
            strategy: row.strategy,
            strategy_params: row.strategy_params,
            fallback_config: row.fallback_config,
            routing_groups: row.routing_groups,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct MemoryRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    user_id: uuid::Uuid,
    repo_id: Option<String>,
    scope: String,
    content: String,
    tags: Vec<String>,
    status: String,
    valid_from: DateTime<Utc>,
    valid_to: Option<DateTime<Utc>>,
    superseded_by: Option<uuid::Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<MemoryRow> for Memory {
    fn from(row: MemoryRow) -> Self {
        Memory {
            id: row.id.to_string(),
            team_id: row.team_id.to_string(),
            user_id: row.user_id.to_string(),
            repo_id: row.repo_id,
            scope: row.scope,
            content: row.content,
            tags: row.tags,
            status: row.status,
            valid_from: row.valid_from,
            valid_to: row.valid_to,
            superseded_by: row.superseded_by.map(|u| u.to_string()),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct MemorySearchRow {
    id: uuid::Uuid,
    team_id: uuid::Uuid,
    user_id: uuid::Uuid,
    repo_id: Option<String>,
    scope: String,
    content: String,
    tags: Vec<String>,
    status: String,
    valid_from: DateTime<Utc>,
    valid_to: Option<DateTime<Utc>>,
    superseded_by: Option<uuid::Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    score: f64,
}

impl From<MemorySearchRow> for MemorySearchResult {
    fn from(row: MemorySearchRow) -> Self {
        MemorySearchResult {
            memory: Memory {
                id: row.id.to_string(),
                team_id: row.team_id.to_string(),
                user_id: row.user_id.to_string(),
                repo_id: row.repo_id,
                scope: row.scope,
                content: row.content,
                tags: row.tags,
                status: row.status,
                valid_from: row.valid_from,
                valid_to: row.valid_to,
                superseded_by: row.superseded_by.map(|u| u.to_string()),
                created_at: row.created_at,
                updated_at: row.updated_at,
            },
            score: row.score,
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

    pub async fn ping(&self) -> Result<(), hyperinfer_core::ConfigError> {
        self.manager.ping().await
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

    async fn publish_memory_event(
        &self,
        event: &MemoryUpdateEvent,
    ) -> Result<(), hyperinfer_core::ConfigError> {
        self.manager.publish_memory_event(event).await
    }

    async fn ping(&self) -> Result<(), hyperinfer_core::ConfigError> {
        self.manager.ping().await
    }
}
