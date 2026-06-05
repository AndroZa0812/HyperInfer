use hyperinfer_core::{ChatRequest, Database, Provider};
use hyperinfer_router::{
    deployment::Deployment as RouterDeployment,
    engine::{GlobalLimits, RouterEngine},
    error::RoutingError,
    strategy::{
        cost_based::CostBased, latency_based::LatencyBased, least_busy::LeastBusy,
        usage_based::UsageBased, weighted_shuffle::WeightedShuffle, DeploymentMetrics,
        RecordFailureResult, RoutingContext, RoutingState,
    },
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::LazyLock;

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

const BLOCKED_IP_PREFIXES: &[&str] = &[
    "169.254.", "10.", "172.16.", "172.17.", "172.18.", "172.19.", "172.20.", "172.21.", "172.22.",
    "172.23.", "172.24.", "172.25.", "172.26.", "172.27.", "172.28.", "172.29.", "172.30.",
    "172.31.", "192.168.", "127.", "0.",
];

pub struct ProxyAuth {
    pub team_id: String,
    pub api_key_id: String,
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
            Ok(ProxyAuth {
                team_id: key.team_id,
                api_key_id: key.id,
            })
        }
        Ok(None) => Err(401),
        Err(_) => Err(500),
    }
}

fn validate_base_url(url: &str) -> Result<(), u16> {
    let parsed = url::Url::parse(url).map_err(|_| 400u16)?;
    let host = parsed.host_str().ok_or(400u16)?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        let ip_str = ip.to_string();
        for prefix in BLOCKED_IP_PREFIXES {
            if ip_str.starts_with(prefix) {
                return Err(400);
            }
        }
    }

    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(400);
    }

    Ok(())
}

fn default_base_url_for_provider(provider: &Provider) -> String {
    match provider {
        Provider::Anthropic => "https://api.anthropic.com/v1".to_string(),
        Provider::OpenAI => "https://api.openai.com/v1".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
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
        .unwrap_or_else(|| default_base_url_for_provider(&selected.provider));

    Ok(SelectedDeployment {
        deployment: RouterDeployment::new(
            selected.model_name.clone(),
            selected.provider.clone(),
            selected.model.clone(),
            selected.api_key_ref.clone(),
        ),
        base_url,
        api_key: selected.api_key_ref.clone(),
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
