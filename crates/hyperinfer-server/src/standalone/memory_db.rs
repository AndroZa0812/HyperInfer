use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::Utc;
use hyperinfer_core::{
    ApiKey, CreateDeploymentRequest, CreateMemoryRequest, Database, DbError, Deployment,
    ListMemoriesFilter, Memory, MemorySearchResult, ModelAlias, Quota, RoutingConfig,
    SearchMemoriesRequest, Team, UpdateMemoryRequest, UpdateRoutingConfigRequest, UsageLog,
    UsageSummary, User,
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
    memories: Vec<Memory>,
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
            status_code: 200,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            cost_cents: 0.0,
            cache_hit: false,
            failover_used: false,
            deployment_name: None,
            stop_reason: None,
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

    async fn create_memory(&self, req: CreateMemoryRequest) -> Result<Memory, DbError> {
        let mut store = self.store.write().unwrap();
        let now = Utc::now();
        let new_id = Uuid::new_v4().to_string();

        if let Some(ref old_id) = req.supersedes_id {
            let old = store
                .memories
                .iter_mut()
                .find(|m| m.id == *old_id && m.team_id == req.team_id && m.valid_to.is_none())
                .ok_or(DbError::NotFound)?;
            old.valid_to = Some(now);
            old.status = "expired".to_string();
            old.superseded_by = Some(new_id.clone());
            old.updated_at = now;
        }

        let memory = Memory {
            id: new_id,
            team_id: req.team_id,
            user_id: req.user_id,
            repo_id: req.repo_id,
            scope: req.scope,
            content: req.content,
            tags: req.tags,
            status: req.status,
            valid_from: now,
            valid_to: None,
            superseded_by: None,
            created_at: now,
            updated_at: now,
        };
        store.memories.push(memory.clone());
        Ok(memory)
    }

    async fn get_memory(&self, id: &str) -> Result<Option<Memory>, DbError> {
        let store = self.store.read().unwrap();
        Ok(store.memories.iter().find(|m| m.id == id).cloned())
    }

    async fn list_memories(&self, filter: ListMemoriesFilter) -> Result<Vec<Memory>, DbError> {
        let store = self.store.read().unwrap();
        let mut results: Vec<Memory> = store
            .memories
            .iter()
            .filter(|m| m.team_id == filter.team_id)
            .filter(|m| {
                filter
                    .user_id
                    .as_deref()
                    .is_none_or(|uid| m.scope != "user" || m.user_id == uid)
            })
            .filter(|m| filter.scope.as_deref().is_none_or(|s| m.scope == s))
            .filter(|m| {
                filter
                    .repo_id
                    .as_deref()
                    .is_none_or(|r| m.repo_id.as_deref() == Some(r))
            })
            .filter(|m| {
                filter
                    .tag
                    .as_deref()
                    .is_none_or(|t| m.tags.iter().any(|mt| mt == t))
            })
            .filter(|m| filter.status.as_deref().is_none_or(|s| m.status == s))
            .filter(|m| filter.include_expired || (m.valid_to.is_none() && m.status != "expired"))
            .cloned()
            .collect();

        results.sort_by_key(|m| std::cmp::Reverse(m.updated_at));
        let offset = filter.offset.max(0) as usize;
        let limit = filter.limit.max(0) as usize;
        Ok(results.into_iter().skip(offset).take(limit).collect())
    }

    async fn search_memories(
        &self,
        req: SearchMemoriesRequest,
    ) -> Result<Vec<MemorySearchResult>, DbError> {
        let store = self.store.read().unwrap();
        let query_terms: Vec<String> = req
            .query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_lowercase())
            .collect();

        if query_terms.is_empty() {
            return Ok(vec![]);
        }

        let mut scored: Vec<MemorySearchResult> = store
            .memories
            .iter()
            .filter(|m| m.team_id == req.team_id)
            .filter(|m| m.valid_to.is_none() && m.status == "active")
            .filter(|m| {
                req.user_id
                    .as_deref()
                    .is_none_or(|uid| m.scope != "user" || m.user_id == uid)
            })
            .filter(|m| req.scope.as_deref().is_none_or(|s| m.scope == s))
            .filter(|m| {
                req.repo_id
                    .as_deref()
                    .is_none_or(|r| m.repo_id.as_deref() == Some(r))
            })
            .filter(|m| {
                req.tags.is_empty()
                    || req
                        .tags
                        .iter()
                        .all(|t| m.tags.iter().any(|mt| mt.eq_ignore_ascii_case(t)))
            })
            .filter_map(|m| {
                let haystack = format!("{} {}", m.content, m.tags.join(" ")).to_lowercase();
                let mut matches = 0usize;
                let mut total_hits = 0usize;
                for term in &query_terms {
                    let count = haystack.matches(term.as_str()).count();
                    if count > 0 {
                        matches += 1;
                        total_hits += count;
                    }
                }
                if matches == 0 {
                    return None;
                }
                let coverage = (matches as f64) / (query_terms.len() as f64);
                let tf_bonus = (total_hits as f64) * 0.1;
                Some(MemorySearchResult {
                    memory: m.clone(),
                    score: coverage + tf_bonus,
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory.updated_at.cmp(&a.memory.updated_at))
        });
        scored.truncate(req.limit.max(0) as usize);
        Ok(scored)
    }

    async fn update_memory(
        &self,
        id: &str,
        team_id: &str,
        req: UpdateMemoryRequest,
    ) -> Result<Memory, DbError> {
        let mut store = self.store.write().unwrap();
        let now = Utc::now();
        let mem = store
            .memories
            .iter_mut()
            .find(|m| m.id == id && m.team_id == team_id)
            .ok_or(DbError::NotFound)?;

        if let Some(content) = req.content {
            mem.content = content;
        }
        if let Some(tags) = req.tags {
            mem.tags = tags;
        }
        if let Some(status) = req.status {
            if status == "expired" && mem.valid_to.is_none() {
                mem.valid_to = Some(now);
            } else if status == "active" {
                mem.valid_to = None;
            }
            mem.status = status;
        }
        mem.updated_at = now;
        Ok(mem.clone())
    }

    async fn expire_memory(&self, id: &str, team_id: &str) -> Result<Memory, DbError> {
        let mut store = self.store.write().unwrap();
        let now = Utc::now();
        let mem = store
            .memories
            .iter_mut()
            .find(|m| m.id == id && m.team_id == team_id)
            .ok_or(DbError::NotFound)?;

        mem.status = "expired".to_string();
        if mem.valid_to.is_none() {
            mem.valid_to = Some(now);
        }
        mem.updated_at = now;
        Ok(mem.clone())
    }

    async fn list_usage_logs(
        &self,
        filter: hyperinfer_core::ListUsageLogsFilter,
    ) -> Result<hyperinfer_core::UsageLogsPage, DbError> {
        let store = self.store.read().unwrap();
        let limit = filter.limit.clamp(1, 200);
        let offset = filter.offset.max(0);

        let mut matched: Vec<UsageLog> = store
            .usage_logs
            .iter()
            .filter(|l| {
                if let Some(ref tid) = filter.team_id {
                    if &l.team_id != tid {
                        return false;
                    }
                }
                if let Some(ref kid) = filter.api_key_id {
                    if &l.api_key_id != kid {
                        return false;
                    }
                }
                if let Some(ref model) = filter.model {
                    if !l.model.to_lowercase().contains(&model.to_lowercase()) {
                        return false;
                    }
                }
                if let Some(code) = filter.status_code {
                    if l.status_code != code {
                        return false;
                    }
                }
                if let Some(hit) = filter.cache_hit {
                    if l.cache_hit != hit {
                        return false;
                    }
                }
                if let Some(from) = filter.from {
                    if l.recorded_at < from {
                        return false;
                    }
                }
                if let Some(to) = filter.to {
                    if l.recorded_at > to {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        matched.sort_by_key(|b| std::cmp::Reverse(b.recorded_at));
        let total = matched.len() as i64;
        let logs = matched
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        Ok(hyperinfer_core::UsageLogsPage {
            logs,
            total,
            limit,
            offset,
        })
    }
}
