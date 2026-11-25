use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::Utc;
use hyperinfer_core::{
    ApiKey, CreateDeploymentRequest, Database, DbError, Deployment, ModelAlias, Quota,
    RoutingConfig, Team, UpdateRoutingConfigRequest, UsageLog, UsageSummary, User,
};
use uuid::Uuid;

#[derive(Default)]
struct MemoryStore {
    teams: Vec<Team>,
    users: Vec<User>,
    api_keys: Vec<ApiKey>,
    model_aliases: Vec<ModelAlias>,
    quotas: Vec<Quota>,
    usage_logs: Vec<UsageLog>,
    deployments: Vec<Deployment>,
    routing_config: Option<RoutingConfig>,
}

#[derive(Clone, Default)]
pub struct InMemoryDb {
    store: Arc<RwLock<MemoryStore>>,
}

impl InMemoryDb {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Database for InMemoryDb {
    async fn get_team(&self, id: &str) -> Result<Option<Team>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.teams.iter().find(|t| t.id == id).cloned())
    }

    async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, DbError> {
        let mut store = self.store.write().unwrap();
        if store.teams.iter().any(|t| t.name == name) {
            return Err(DbError::UniqueViolation(format!("team name: {name}")));
        }
        let now = Utc::now();
        let team = Team {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            budget_cents,
            created_at: now,
            updated_at: now,
        };
        store.teams.push(team.clone());
        Ok(team)
    }

    async fn get_user(&self, id: &str) -> Result<Option<User>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.users.iter().find(|u| u.id == id).cloned())
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.users.iter().find(|u| u.email == email).cloned())
    }

    async fn create_user(
        &self,
        team_id: &str,
        email: &str,
        role: &str,
        password_hash: Option<String>,
    ) -> Result<User, DbError> {
        let mut store = self.store.write().unwrap();
        if store.users.iter().any(|u| u.email == email) {
            return Err(DbError::UniqueViolation(format!("email: {email}")));
        }
        let user = User {
            id: Uuid::new_v4().to_string(),
            team_id: team_id.to_string(),
            email: email.to_string(),
            role: role.to_string(),
            password_hash,
            created_at: Utc::now(),
        };
        store.users.push(user.clone());
        Ok(user)
    }

    async fn update_password_hash(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), DbError> {
        let mut store = self.store.write().unwrap();
        let user = store
            .users
            .iter_mut()
            .find(|u| u.id == user_id)
            .ok_or(DbError::NotFound)?;
        user.password_hash = Some(password_hash.to_string());
        Ok(())
    }

    async fn count_users_by_role(&self, role: &str) -> Result<i64, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.users.iter().filter(|u| u.role == role).count() as i64)
    }

    async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.api_keys.iter().find(|k| k.id == id).cloned())
    }

    async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store
            .api_keys
            .iter()
            .find(|k| k.key_hash == key_hash)
            .cloned())
    }

    async fn create_api_key(
        &self,
        key_hash: &str,
        user_id: &str,
        team_id: &str,
        name: Option<String>,
    ) -> Result<ApiKey, DbError> {
        let mut store = self.store.write().unwrap();
        let key = ApiKey {
            id: Uuid::new_v4().to_string(),
            key_hash: key_hash.to_string(),
            user_id: user_id.to_string(),
            team_id: team_id.to_string(),
            name,
            is_active: true,
            created_at: Utc::now(),
            expires_at: None,
            budget_cents: None,
            spent_cents: 0,
            allowed_models: None,
            rpm_limit: None,
            tpm_limit: None,
        };
        store.api_keys.push(key.clone());
        Ok(key)
    }

    async fn deactivate_api_key(&self, id: &str) -> Result<ApiKey, DbError> {
        let mut store = self.store.write().unwrap();
        let key = store
            .api_keys
            .iter_mut()
            .find(|k| k.id == id)
            .ok_or(DbError::NotFound)?;
        key.is_active = false;
        Ok(key.clone())
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_api_key_limits(
        &self,
        id: &str,
        budget_cents: Option<i64>,
        allowed_models: Option<Vec<String>>,
        rpm_limit: Option<i32>,
        tpm_limit: Option<i32>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<ApiKey, DbError> {
        let mut store = self.store.write().unwrap();
        let key = store
            .api_keys
            .iter_mut()
            .find(|k| k.id == id)
            .ok_or(DbError::NotFound)?;
        key.budget_cents = budget_cents;
        key.allowed_models = allowed_models;
        key.rpm_limit = rpm_limit;
        key.tpm_limit = tpm_limit;
        key.expires_at = expires_at;
        Ok(key.clone())
    }

    async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.model_aliases.iter().find(|a| a.id == id).cloned())
    }

    async fn create_model_alias(
        &self,
        team_id: &str,
        alias: &str,
        target_model: &str,
        provider: &str,
    ) -> Result<ModelAlias, DbError> {
        let mut store = self.store.write().unwrap();
        let ma = ModelAlias {
            id: Uuid::new_v4().to_string(),
            team_id: team_id.to_string(),
            alias: alias.to_string(),
            target_model: target_model.to_string(),
            provider: provider.to_string(),
            created_at: Utc::now(),
        };
        store.model_aliases.push(ma.clone());
        Ok(ma)
    }

    async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.quotas.iter().find(|q| q.team_id == team_id).cloned())
    }

    async fn create_quota(
        &self,
        team_id: &str,
        rpm_limit: i32,
        tpm_limit: i32,
    ) -> Result<Quota, DbError> {
        let mut store = self.store.write().unwrap();
        if let Some(pos) = store.quotas.iter().position(|q| q.team_id == team_id) {
            store.quotas[pos].rpm_limit = rpm_limit;
            store.quotas[pos].tpm_limit = tpm_limit;
            store.quotas[pos].updated_at = Utc::now();
            return Ok(store.quotas[pos].clone());
        }
        let quota = Quota {
            id: Uuid::new_v4().to_string(),
            team_id: team_id.to_string(),
            rpm_limit,
            tpm_limit,
            updated_at: Utc::now(),
        };
        store.quotas.push(quota.clone());
        Ok(quota)
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
        let mut store = self.store.write().unwrap();
        let log = UsageLog {
            id: Uuid::new_v4().to_string(),
            team_id: team_id.to_string(),
            api_key_id: api_key_id.to_string(),
            model: model.to_string(),
            input_tokens,
            output_tokens,
            response_time_ms,
            recorded_at: Utc::now(),
        };
        store.usage_logs.push(log.clone());
        Ok(log)
    }

    async fn list_deployments(
        &self,
        model: &str,
        is_active: Option<bool>,
    ) -> Result<Vec<Deployment>, DbError> {
        let store = self.store.read().unwrap();
        let mut result = store.deployments.clone();
        if !model.is_empty() {
            result.retain(|d| d.model == model);
        }
        if let Some(flag) = is_active {
            result.retain(|d| d.is_active == flag);
        }
        Ok(result)
    }

    async fn get_deployment(&self, id: &str) -> Result<Option<Deployment>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.deployments.iter().find(|d| d.id == id).cloned())
    }

    async fn create_deployment(&self, req: CreateDeploymentRequest) -> Result<Deployment, DbError> {
        let mut store = self.store.write().unwrap();
        let now = Utc::now();
        let deployment = Deployment {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            provider: req.provider,
            model: req.model,
            api_key_ref: req.api_key_ref.unwrap_or_default(),
            base_url: req.base_url,
            is_active: req.is_active,
            weight: req.weight as u32,
            priority: req.priority as u32,
            max_tpm: req.max_tpm.map(|v| v as u32),
            max_rpm: req.max_rpm.map(|v| v as u32),
            cost_per_1k_input_tokens: req.cost_per_1k_input_tokens,
            cost_per_1k_output_tokens: req.cost_per_1k_output_tokens,
            metadata: req.metadata.unwrap_or(serde_json::Value::Null),
            sort_order: req.sort_order.unwrap_or(0) as u32,
            created_at: now,
            updated_at: now,
        };
        store.deployments.push(deployment.clone());
        Ok(deployment)
    }

    async fn update_deployment(
        &self,
        id: &str,
        req: CreateDeploymentRequest,
    ) -> Result<Deployment, DbError> {
        let mut store = self.store.write().unwrap();
        let dep = store
            .deployments
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or(DbError::NotFound)?;
        dep.name = req.name;
        dep.provider = req.provider;
        dep.model = req.model;
        dep.api_key_ref = req.api_key_ref.unwrap_or_default();
        dep.base_url = req.base_url;
        dep.is_active = req.is_active;
        dep.weight = req.weight as u32;
        dep.priority = req.priority as u32;
        dep.max_tpm = req.max_tpm.map(|v| v as u32);
        dep.max_rpm = req.max_rpm.map(|v| v as u32);
        dep.cost_per_1k_input_tokens = req.cost_per_1k_input_tokens;
        dep.cost_per_1k_output_tokens = req.cost_per_1k_output_tokens;
        dep.metadata = req.metadata.unwrap_or(serde_json::Value::Null);
        dep.sort_order = req.sort_order.unwrap_or(0) as u32;
        dep.updated_at = Utc::now();
        Ok(dep.clone())
    }

    async fn delete_deployment(&self, id: &str) -> Result<(), DbError> {
        let mut store = self.store.write().unwrap();
        let pos = store
            .deployments
            .iter()
            .position(|d| d.id == id)
            .ok_or(DbError::NotFound)?;
        store.deployments.remove(pos);
        Ok(())
    }

    async fn get_routing_config(&self) -> Result<Option<RoutingConfig>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.routing_config.clone())
    }

    async fn update_routing_config(
        &self,
        req: UpdateRoutingConfigRequest,
    ) -> Result<RoutingConfig, DbError> {
        let mut store = self.store.write().unwrap();
        let existing = store.routing_config.as_ref();
        let config = RoutingConfig {
            strategy: req
                .strategy
                .or_else(|| existing.map(|c| c.strategy.clone()))
                .unwrap_or_default(),
            strategy_params: req
                .strategy_params
                .or_else(|| existing.map(|c| c.strategy_params.clone()))
                .unwrap_or(serde_json::Value::Null),
            fallback_config: req
                .fallback_config
                .or_else(|| existing.map(|c| c.fallback_config.clone()))
                .unwrap_or(serde_json::Value::Null),
            routing_groups: req
                .routing_groups
                .or_else(|| existing.map(|c| c.routing_groups.clone()))
                .unwrap_or(serde_json::Value::Null),
            updated_at: Utc::now(),
        };
        store.routing_config = Some(config.clone());
        Ok(config)
    }

    async fn ping(&self) -> Result<(), DbError> {
        Ok(())
    }

    async fn list_teams(&self) -> Result<Vec<Team>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.teams.clone())
    }

    async fn list_api_keys(&self, team_id: Option<String>) -> Result<Vec<ApiKey>, DbError> {
        let store = self.store.read().unwrap();
        match team_id {
            Some(tid) => Ok(store
                .api_keys
                .iter()
                .filter(|k| k.team_id == tid)
                .cloned()
                .collect()),
            None => Ok(store.api_keys.clone()),
        }
    }

    async fn get_usage_summary(&self, team_id: Option<String>) -> Result<UsageSummary, DbError> {
        let store = self.store.read().unwrap();
        let logs: Vec<&UsageLog> = match team_id {
            Some(tid) => store
                .usage_logs
                .iter()
                .filter(|l| l.team_id == tid)
                .collect(),
            None => store.usage_logs.iter().collect(),
        };
        let total_requests = logs.len() as i64;
        let total_input_tokens: i64 = logs.iter().map(|l| l.input_tokens as i64).sum();
        let total_output_tokens: i64 = logs.iter().map(|l| l.output_tokens as i64).sum();
        let avg_response_time_ms = if logs.is_empty() {
            0.0
        } else {
            let sum: i64 = logs.iter().map(|l| l.response_time_ms).sum();
            (sum as f64) / (logs.len() as f64)
        };
        Ok(UsageSummary {
            total_requests,
            total_input_tokens,
            total_output_tokens,
            avg_response_time_ms,
        })
    }
}
