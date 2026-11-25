use futures::Stream;
use hyperinfer_core::{ChatChunk, ChatRequest, ChatResponse, Database, HyperInferError, Provider};
use hyperinfer_providers::{
    anthropic::AnthropicProvider, gemini::GeminiProvider, openai::OpenAiProvider, LlmProvider,
    ProviderRegistry,
};
use hyperinfer_router::{
    deployment::Deployment as RouterDeployment,
    engine::{GlobalLimits, RouterEngine},
    error::RoutingError,
    state::redis_state::{RedisConfig, RedisRoutingState},
    strategy::{
        cost_based::CostBased, latency_based::LatencyBased, least_busy::LeastBusy,
        usage_based::UsageBased, weighted_shuffle::WeightedShuffle, DeploymentMetrics,
        RecordFailureResult, RoutingContext, RoutingState,
    },
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use tracing::warn;

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .dns_resolver(Arc::new(crate::safe_resolver::SafeResolver))
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("Failed to build secure reqwest HTTP client")
});

pub struct ProxyAuth {
    pub team_id: String,
    pub api_key_id: String,
    pub budget_cents: Option<i64>,
    pub spent_cents: i64,
    pub allowed_models: Option<Vec<String>>,
    pub rpm_limit: Option<i32>,
    pub tpm_limit: Option<i32>,
}

pub struct NoopState;

#[async_trait::async_trait]
impl RoutingState for NoopState {
    async fn get_metrics(&self, _deployment_id: &str) -> Result<DeploymentMetrics, RoutingError> {
        Ok(DeploymentMetrics::default())
    }

    async fn get_all_metrics(
        &self,
        _ids: &[&str],
    ) -> Result<HashMap<String, DeploymentMetrics>, RoutingError> {
        Ok(HashMap::new())
    }

    async fn is_cooled_down(&self, _deployment_id: &str) -> Result<bool, RoutingError> {
        Ok(false)
    }

    async fn record_request_start(&self, _deployment_id: &str) -> Result<(), RoutingError> {
        Ok(())
    }

    async fn record_request_success(
        &self,
        _deployment_id: &str,
        _latency_ms: f64,
        _tokens: u64,
    ) -> Result<(), RoutingError> {
        Ok(())
    }

    async fn record_request_failure(
        &self,
        _deployment_id: &str,
    ) -> Result<RecordFailureResult, RoutingError> {
        Ok(RecordFailureResult {
            failure_count: 0,
            cooldown_triggered: false,
        })
    }
}

pub async fn validate_api_key<D: Database>(db: &D, api_key: &str) -> Result<ProxyAuth, u16> {
    if api_key.is_empty() {
        return Err(401);
    }

    let key_hash = {
        let mut hasher = Sha256::new();
        hasher.update(api_key.as_bytes());
        hex::encode(hasher.finalize())
    };

    match db.get_api_key_by_hash(&key_hash).await {
        Ok(Some(key)) => {
            if !key.is_active {
                return Err(401);
            }
            if let Some(exp) = key.expires_at {
                if exp <= chrono::Utc::now() {
                    return Err(401);
                }
            }
            Ok(ProxyAuth {
                team_id: key.team_id,
                api_key_id: key.id,
                budget_cents: key.budget_cents,
                spent_cents: key.spent_cents,
                allowed_models: key.allowed_models,
                rpm_limit: key.rpm_limit,
                tpm_limit: key.tpm_limit,
            })
        }
        Ok(None) => Err(401),
        Err(_) => Err(500),
    }
}

impl ProxyAuth {
    pub fn check_request_allowed(&self, model: &str) -> Result<(), (u16, String)> {
        if let Some(ref models) = self.allowed_models {
            if !models.is_empty() && !models.iter().any(|m| m == model) {
                return Err((
                    403,
                    format!("Model '{}' is not allowed for this API key", model),
                ));
            }
        }
        if let Some(cap) = self.budget_cents {
            if self.spent_cents >= cap {
                return Err((402, "API key budget exceeded".to_string()));
            }
        }
        Ok(())
    }
}

fn validate_base_url(url: &str) -> Result<(), u16> {
    let parsed = url::Url::parse(url).map_err(|_| 400u16)?;
    let host = parsed.host_str().ok_or(400u16)?;

    // Reject localhost (case-insensitive)
    let host_lower = host.to_lowercase();
    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        return Err(400);
    }

    // Strip IPv6 brackets before parsing
    let host_clean = host
        .strip_prefix('[')
        .unwrap_or(host)
        .strip_suffix(']')
        .unwrap_or(host);

    if let Ok(ip) = host_clean.parse::<IpAddr>() {
        if crate::safe_resolver::is_blocked(ip) {
            return Err(400);
        }
    }

    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(400);
    }

    Ok(())
}

fn resolve_api_key_ref(raw: &str) -> String {
    if let Some(var_name) = raw.strip_prefix('$') {
        std::env::var(var_name).unwrap_or_default()
    } else if let Some(var_name) = raw.strip_prefix("env:") {
        std::env::var(var_name).unwrap_or_default()
    } else {
        raw.to_string()
    }
}

fn default_base_url_for_provider(provider: &Provider) -> String {
    match provider {
        Provider::Anthropic => "https://api.anthropic.com".to_string(),
        Provider::Gemini => "https://generativelanguage.googleapis.com".to_string(),
        Provider::Antigravity => "https://cloudcode-pa.googleapis.com".to_string(),
        Provider::OpenAI | Provider::Other => "https://api.openai.com".to_string(),
    }
}

/// Normalize a deployment's `base_url` for `hyperinfer-providers` (`OpenAiProvider` and
/// `AnthropicProvider` append `/v1/chat/completions` and `/v1/messages` respectively).
fn normalize_provider_base_url(base_url: &str, provider: &Provider) -> String {
    let trimmed = base_url.trim_end_matches('/');
    match provider {
        Provider::OpenAI | Provider::Anthropic | Provider::Other => {
            trimmed.trim_end_matches("/v1").to_string()
        }
        Provider::Gemini | Provider::Antigravity => trimmed.to_string(),
    }
}

/// Shared proxy router holding a persistent `RouterEngine`, `RoutingState` (`RedisRoutingState`
/// with graceful `NoopState` fallback), and `ProviderRegistry`.
pub struct ProxyRouter {
    engine: Arc<RouterEngine>,
    state: Arc<dyn RoutingState>,
    providers: Arc<ProviderRegistry>,
}

impl ProxyRouter {
    /// Create an in-memory `ProxyRouter` (for tests or Redis-less environments).
    pub async fn new_in_memory() -> Self {
        let engine = RouterEngine::new(GlobalLimits::default());
        Self::register_all_strategies(&engine).await;
        let providers = Arc::new(ProviderRegistry::new());
        hyperinfer_providers::init_default_registry(&providers);
        Self {
            engine: Arc::new(engine),
            state: Arc::new(NoopState),
            providers,
        }
    }

    /// Create a `ProxyRouter` backed by `RedisRoutingState`, falling back to `NoopState`
    /// if Redis is unavailable.
    pub async fn with_redis(redis_url: &str) -> Self {
        let engine = RouterEngine::new(GlobalLimits::default());
        Self::register_all_strategies(&engine).await;
        let state: Arc<dyn RoutingState> =
            match RedisRoutingState::new(redis_url, RedisConfig::default()).await {
                Ok(redis_state) => Arc::new(redis_state),
                Err(e) => {
                    warn!(
                        error = %e,
                        "Redis unavailable for server ProxyRouter; falling back to NoopState"
                    );
                    Arc::new(NoopState)
                }
            };
        let providers = Arc::new(ProviderRegistry::new());
        hyperinfer_providers::init_default_registry(&providers);
        Self {
            engine: Arc::new(engine),
            state,
            providers,
        }
    }

    async fn register_all_strategies(engine: &RouterEngine) {
        engine
            .register_strategy(Box::new(WeightedShuffle::new()))
            .await;
        engine
            .register_strategy(Box::new(LatencyBased::new()))
            .await;
        engine.register_strategy(Box::new(LeastBusy::new())).await;
        engine.register_strategy(Box::new(UsageBased::new())).await;
        engine.register_strategy(Box::new(CostBased::new())).await;
    }

    pub fn providers(&self) -> &Arc<ProviderRegistry> {
        &self.providers
    }

    /// Select a deployment using a per-request `RouterEngine` (to avoid pool races)
    /// while still passing the shared `RoutingState` for metrics/latency/cooldowns.
    pub async fn select_deployment<D: Database>(
        &self,
        db: &D,
        request: &ChatRequest,
        deployments: &[hyperinfer_core::Deployment],
        model_aliases: &HashMap<String, String>,
        auth: Option<&ProxyAuth>,
    ) -> Result<SelectedDeployment, RoutingError> {
        if deployments.is_empty() {
            return Err(RoutingError::NoDeployments(request.model.clone()));
        }

        // Build a per-request engine to avoid mutating the shared pool.
        let engine = RouterEngine::new(GlobalLimits::default());
        Self::register_all_strategies(&engine).await;

        for (alias, target) in model_aliases {
            engine.set_alias(alias, target).await;
        }

        if let Ok(Some(config)) = db.get_routing_config().await {
            engine.set_default_strategy(&config.strategy).await;
            if let Ok(groups) =
                serde_json::from_value::<HashMap<String, String>>(config.routing_groups)
            {
                for (model, strategy_name) in groups {
                    engine.set_routing_group(&model, &strategy_name).await;
                }
            }
        }

        let router_deployments: Vec<RouterDeployment> =
            deployments.iter().map(core_to_router_deployment).collect();
        engine.rebuild_pool(router_deployments).await;

        let ctx = RoutingContext {
            team_id: auth.map(|a| a.team_id.clone()),
            ..Default::default()
        };
        // Pass the shared state so metrics/latency/cooldowns are still shared.
        let result = engine
            .select_deployment(&request.model, self.state.as_ref(), &ctx)
            .await?;

        let selected = &result.deployment;
        let base_url = selected
            .base_url
            .clone()
            .unwrap_or_else(|| default_base_url_for_provider(&selected.provider));

        Ok(SelectedDeployment {
            deployment: selected.as_ref().clone(),
            base_url,
            api_key: resolve_api_key_ref(&selected.api_key_ref),
            provider: selected.provider.clone(),
        })
    }

    pub async fn record_success(&self, deployment_id: &str, latency_ms: f64, tokens: u64) {
        self.engine
            .record_success(deployment_id, latency_ms, tokens, self.state.as_ref())
            .await;
    }

    pub async fn record_failure(&self, deployment_id: &str) {
        self.engine
            .record_failure(deployment_id, self.state.as_ref())
            .await;
    }

    pub async fn record_quota_exhausted(&self, deployment_id: &str) {
        self.engine
            .record_quota_exhausted(deployment_id, self.state.as_ref())
            .await;
    }
}

fn core_to_router_deployment(d: &hyperinfer_core::Deployment) -> RouterDeployment {
    let provider = match d.provider.as_str() {
        "openai" => Provider::OpenAI,
        "anthropic" => Provider::Anthropic,
        "gemini" => Provider::Gemini,
        "antigravity" => Provider::Antigravity,
        _ => Provider::Other,
    };
    let mut router_deployment = RouterDeployment::new(
        d.model.clone(),
        provider,
        d.model.clone(),
        d.api_key_ref.clone(),
    );
    router_deployment.id = d.id.clone();
    if !d.base_url.is_empty() {
        router_deployment = router_deployment.with_base_url(d.base_url.clone());
    }
    router_deployment = router_deployment.with_weight(d.weight);
    if let Some(max_tpm) = d.max_tpm {
        router_deployment = router_deployment.with_tpm_limit(max_tpm as u64);
    }
    if let Some(max_rpm) = d.max_rpm {
        router_deployment = router_deployment.with_rpm_limit(max_rpm as u64);
    }
    if let Some(cost) = d.cost_per_1k_input_tokens {
        router_deployment = router_deployment.with_input_cost(cost);
    }
    if let Some(cost) = d.cost_per_1k_output_tokens {
        router_deployment = router_deployment.with_output_cost(cost);
    }
    router_deployment
}

fn resolve_provider(
    registry: &ProviderRegistry,
    provider: &Provider,
    base_url: &str,
) -> Result<Arc<dyn LlmProvider>, u16> {
    let default_url = default_base_url_for_provider(provider);
    let normalized = normalize_provider_base_url(base_url, provider);

    if normalized == default_url {
        if let Some(p) = registry.get(&provider.to_string()) {
            return Ok(p);
        }
    }

    match provider {
        Provider::Anthropic => AnthropicProvider::with_base_url(normalized)
            .map(|p| Arc::new(p) as Arc<dyn LlmProvider>)
            .map_err(|_| 500u16),
        Provider::Gemini => GeminiProvider::with_base_url(normalized)
            .map(|p| Arc::new(p) as Arc<dyn LlmProvider>)
            .map_err(|_| 500u16),
        Provider::Antigravity => GeminiProvider::antigravity_with_base_url(normalized)
            .map(|p| Arc::new(p) as Arc<dyn LlmProvider>)
            .map_err(|_| 500u16),
        Provider::OpenAI | Provider::Other => OpenAiProvider::with_base_url(normalized)
            .map(|p| Arc::new(p) as Arc<dyn LlmProvider>)
            .map_err(|_| 500u16),
    }
}

fn map_provider_error(err: HyperInferError) -> u16 {
    match err {
        HyperInferError::ApiError { status, .. } => status,
        HyperInferError::RateLimit(_) => 429,
        HyperInferError::UnsupportedStreaming(_) => 400,
        _ => 502,
    }
}

/// Forward a non-streaming `ChatRequest` through `hyperinfer-providers`.
pub async fn forward_chat(
    registry: &ProviderRegistry,
    request: &ChatRequest,
    base_url: &str,
    api_key: &str,
    provider: &Provider,
) -> Result<ChatResponse, u16> {
    validate_base_url(base_url)?;
    let llm_provider = resolve_provider(registry, provider, base_url)?;
    llm_provider
        .chat(request, api_key)
        .await
        .map_err(map_provider_error)
}

/// Forward a streaming `ChatRequest` (`stream: true`) through `hyperinfer-providers`.
pub async fn forward_chat_stream(
    registry: &ProviderRegistry,
    request: &ChatRequest,
    base_url: &str,
    api_key: &str,
    provider: &Provider,
) -> Result<Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send>>, u16> {
    validate_base_url(base_url)?;
    let llm_provider = resolve_provider(registry, provider, base_url)?;
    Ok(llm_provider.stream(request, api_key))
}

fn set_auth_header(
    headers: &mut reqwest::header::HeaderMap,
    provider: &Provider,
    api_key: &str,
) -> Result<(), u16> {
    if api_key.is_empty() {
        return Ok(());
    }
    match provider {
        Provider::Anthropic => {
            headers.insert("x-api-key", api_key.parse().map_err(|_| 500u16)?);
            headers.insert(
                "anthropic-version",
                "2023-06-01".parse().map_err(|_| 500u16)?,
            );
        }
        _ => {
            headers.insert(
                "authorization",
                format!("Bearer {}", api_key).parse().map_err(|_| 500u16)?,
            );
        }
    }
    Ok(())
}

pub struct SelectedDeployment {
    pub deployment: RouterDeployment,
    pub base_url: String,
    pub api_key: String,
    pub provider: Provider,
}

pub async fn select_deployment<D: Database>(
    db: &D,
    request: &ChatRequest,
    deployments: &[hyperinfer_core::Deployment],
    model_aliases: &std::collections::HashMap<String, String>,
    _auth: Option<&ProxyAuth>,
) -> Result<SelectedDeployment, RoutingError> {
    if deployments.is_empty() {
        return Err(RoutingError::NoDeployments(request.model.clone()));
    }

    let engine = RouterEngine::new(GlobalLimits::default());
    engine
        .register_strategy(Box::new(WeightedShuffle::new()))
        .await;
    engine
        .register_strategy(Box::new(LatencyBased::new()))
        .await;
    engine.register_strategy(Box::new(LeastBusy::new())).await;
    engine.register_strategy(Box::new(UsageBased::new())).await;
    engine.register_strategy(Box::new(CostBased::new())).await;

    for (alias, target) in model_aliases {
        engine.set_alias(alias, target).await;
    }

    if let Ok(Some(config)) = db.get_routing_config().await {
        engine.set_default_strategy(&config.strategy).await;

        if let Ok(groups) = serde_json::from_value::<std::collections::HashMap<String, String>>(
            config.routing_groups,
        ) {
            for (model, strategy_name) in groups {
                engine.set_routing_group(&model, &strategy_name).await;
            }
        }
    }

    for d in deployments {
        let provider = match d.provider.as_str() {
            "openai" => Provider::OpenAI,
            "anthropic" => Provider::Anthropic,
            "gemini" => Provider::Gemini,
            "antigravity" => Provider::Antigravity,
            _ => Provider::Other,
        };
        let mut router_deployment = RouterDeployment::new(
            d.model.clone(),
            provider,
            d.model.clone(),
            d.api_key_ref.clone(),
        );
        router_deployment.id = d.id.clone();
        if !d.base_url.is_empty() {
            router_deployment = router_deployment.with_base_url(d.base_url.clone());
        }
        router_deployment = router_deployment.with_weight(d.weight);
        if let Some(max_tpm) = d.max_tpm {
            router_deployment = router_deployment.with_tpm_limit(max_tpm as u64);
        }
        if let Some(max_rpm) = d.max_rpm {
            router_deployment = router_deployment.with_rpm_limit(max_rpm as u64);
        }
        if let Some(cost) = d.cost_per_1k_input_tokens {
            router_deployment = router_deployment.with_input_cost(cost);
        }
        if let Some(cost) = d.cost_per_1k_output_tokens {
            router_deployment = router_deployment.with_output_cost(cost);
        }
        engine.add_deployment(router_deployment).await;
    }

    let state_ref = &NoopState;
    let ctx = RoutingContext::default();
    let result = engine
        .select_deployment(&request.model, state_ref, &ctx)
        .await?;

    let selected = &result.deployment;
    let base_url = selected
        .base_url
        .clone()
        .unwrap_or_else(|| default_base_url_for_provider(&selected.provider));

    Ok(SelectedDeployment {
        deployment: RouterDeployment::new(
            selected.model_name.clone(),
            selected.provider.clone(),
            selected.model.clone(),
            selected.api_key_ref.clone(),
        ),
        base_url,
        api_key: resolve_api_key_ref(&selected.api_key_ref),
        provider: selected.provider.clone(),
    })
}

pub async fn forward_request(
    request: &ChatRequest,
    base_url: &str,
    api_key: &str,
    provider: &Provider,
) -> Result<serde_json::Value, u16> {
    validate_base_url(base_url)?;

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "content-type",
        "application/json".parse().map_err(|_| 500u16)?,
    );
    set_auth_header(&mut headers, provider, api_key)?;

    let response = match HTTP_CLIENT
        .post(&url)
        .headers(headers)
        .json(request)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Err(502),
    };

    let status = response.status();
    let body: serde_json::Value = match response.json().await {
        Ok(b) => b,
        Err(_) => return Err(502),
    };

    if status.is_success() {
        Ok(body)
    } else {
        Err(status.as_u16())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_provider_base_url() {
        assert_eq!(
            normalize_provider_base_url("https://api.openai.com/v1/", &Provider::OpenAI),
            "https://api.openai.com"
        );
        assert_eq!(
            normalize_provider_base_url("https://api.anthropic.com/v1", &Provider::Anthropic),
            "https://api.anthropic.com"
        );
        assert_eq!(
            normalize_provider_base_url(
                "https://generativelanguage.googleapis.com/",
                &Provider::Gemini
            ),
            "https://generativelanguage.googleapis.com"
        );
    }

    #[test]
    fn test_validate_base_url_blocks_ssrf_targets() {
        assert!(validate_base_url("https://api.openai.com/v1").is_ok());
        assert!(validate_base_url("http://localhost:8080").is_err());
        assert!(validate_base_url("http://sub.localhost:8080").is_err());
        assert!(validate_base_url("http://[::1]:8080").is_err());
        assert!(validate_base_url("http://127.0.0.1:8080").is_err());
        assert!(validate_base_url("ftp://example.com").is_err());
    }

    #[test]
    fn test_resolve_provider_uses_registry_and_custom_urls() {
        let registry = ProviderRegistry::new();
        hyperinfer_providers::init_default_registry(&registry);

        let openai = resolve_provider(&registry, &Provider::OpenAI, "https://api.openai.com/v1")
            .expect("openai");
        assert_eq!(openai.name(), "openai");

        let anthropic = resolve_provider(
            &registry,
            &Provider::Anthropic,
            "https://api.anthropic.com/v1",
        )
        .expect("anthropic");
        assert_eq!(anthropic.name(), "anthropic");

        let custom = resolve_provider(
            &registry,
            &Provider::Anthropic,
            "https://proxy.example.com/v1",
        )
        .expect("custom anthropic");
        assert_eq!(custom.name(), "anthropic");
    }
}
