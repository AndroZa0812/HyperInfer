use hyperinfer_core::{ChatRequest, Database, Provider};
use hyperinfer_router::{
    deployment::Deployment as RouterDeployment,
    engine::{GlobalLimits, RouterEngine},
    error::RoutingError,
    strategy::{
        weighted_shuffle::WeightedShuffle, DeploymentMetrics, RecordFailureResult, RoutingContext,
        RoutingState,
    },
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Auth context extracted from API key
pub struct ProxyAuth {
    pub team_id: String,
    pub api_key_id: String,
}

/// Noop routing state for server-side proxy (no metrics tracking)
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

/// Validate API key and extract team info
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
                return Err(403);
            }
            Ok(ProxyAuth {
                team_id: key.team_id,
                api_key_id: key.id,
            })
        }
        Ok(None) => Err(401),
        Err(_) => Err(500),
    }
}

/// Selected deployment result from routing
pub struct SelectedDeployment {
    pub deployment: RouterDeployment,
    pub base_url: String,
    pub api_key: String,
}

/// Select a deployment for the given request using routing strategies
pub async fn select_deployment(
    request: &ChatRequest,
    deployments: &[hyperinfer_core::Deployment],
    _auth: Option<&ProxyAuth>,
) -> Result<SelectedDeployment, RoutingError> {
    if deployments.is_empty() {
        return Err(RoutingError::NoDeployments(request.model.clone()));
    }

    let engine = RouterEngine::new(GlobalLimits::default());
    engine
        .register_strategy(Box::new(WeightedShuffle::new()))
        .await;

    for d in deployments {
        let provider = match d.provider.as_str() {
            "openai" => Provider::OpenAI,
            "anthropic" => Provider::Anthropic,
            _ => Provider::Other,
        };
        let mut router_deployment = RouterDeployment::new(
            d.name.clone(),
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
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    Ok(SelectedDeployment {
        deployment: RouterDeployment::new(
            selected.model_name.clone(),
            selected.provider.clone(),
            selected.model.clone(),
            selected.api_key_ref.clone(),
        ),
        base_url,
        api_key: selected.api_key_ref.clone(),
    })
}

/// Forward a chat request to a specific deployment URL
pub async fn forward_request(
    request: &ChatRequest,
    base_url: &str,
    api_key: &str,
) -> Result<serde_json::Value, u16> {
    let client = reqwest::Client::new();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "content-type",
        "application/json".parse().map_err(|_| 500u16)?,
    );
    if !api_key.is_empty() {
        headers.insert(
            "authorization",
            format!("Bearer {}", api_key).parse().map_err(|_| 500u16)?,
        );
    }

    let response = match client
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
