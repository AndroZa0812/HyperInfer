//! HyperInfer Client Library - Data Plane

pub mod http_client;
pub mod router;
pub mod telemetry;

pub use http_client::HttpCaller;
pub use router::Router;
pub use telemetry::Telemetry;

use hyperinfer_core::{
    rate_limiting::RateLimiter, types::Provider, ChatRequest, ChatResponse, Config, HyperInferError,
};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct HyperInferClient {
    config: Arc<RwLock<Config>>,
    http_caller: HttpCaller,
    router: Router,
    rate_limiter: RateLimiter,
    telemetry: Telemetry,
}

impl HyperInferClient {
    pub async fn new(redis_url: &str, config: Config) -> Result<Self, HyperInferError> {
        let http_caller = HttpCaller::new().map_err(HyperInferError::Http)?;
        let router = Router::new(config.routing_rules.clone())
            .with_aliases(config.model_aliases.clone())
            .with_default_provider(config.default_provider.clone());
        let rate_limiter = RateLimiter::new(Some(redis_url)).await.map_err(|e| {
            HyperInferError::Config(std::io::Error::other(e.to_string()))
        })?;
        let telemetry = Telemetry::new(redis_url).await.map_err(|e| {
            HyperInferError::Config(std::io::Error::other(e.to_string()))
        })?;
        let config = Arc::new(RwLock::new(config));

        Ok(Self {
            config,
            http_caller,
            router,
            rate_limiter,
            telemetry,
        })
    }

    pub async fn chat(
        &self,
        key: &str,
        request: ChatRequest,
    ) -> Result<ChatResponse, HyperInferError> {
        request.validate()?;

        let start = std::time::Instant::now();

        // 1. Check rate limit
        let allowed = self.rate_limiter.is_allowed(key, 1).await;
        if let Err(e) = allowed {
            return Err(HyperInferError::RateLimit(e.to_string()));
        }
        if !allowed.unwrap() {
            return Err(HyperInferError::RateLimit(
                "Rate limit exceeded".to_string(),
            ));
        }

        // 2. Resolve model alias
        let (model, provider, api_key) = {
            let config = self.config.read().await;
            let resolved = self.router.resolve(&request.model, &config);

            let (model, provider) = resolved.ok_or_else(|| {
                HyperInferError::Config(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Unknown model: '{}'. No routing rule or alias found.", request.model),
                ))
            })?;

            let api_key = config
                .api_keys
                .get(&provider.to_string())
                .cloned()
                .ok_or_else(|| {
                    HyperInferError::Config(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("API key not found for provider: {:?}", provider),
                    ))
                })?;

            (model, provider, api_key)
        };

        // 3. Execute HTTP call
        let response = match provider {
            Provider::OpenAI => {
                self.http_caller
                    .call_openai(&model, &api_key, &request)
                    .await?
            }
            Provider::Anthropic => {
                self.http_caller
                    .call_anthropic(&model, &api_key, &request)
                    .await?
            }
            _ => {
                return Err(HyperInferError::Config(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "Unsupported provider",
                )));
            }
        };

        // 4. Record telemetry
        let elapsed = start.elapsed().as_millis() as u64;
        let _ = self.telemetry.record(key, &model, elapsed).await;

        // Record usage
        let total_tokens = response.usage.input_tokens + response.usage.output_tokens;
        let _ = self
            .rate_limiter
            .record_usage(key, total_tokens as u64)
            .await;

        // 5. Return response
        Ok(response)
    }
}
