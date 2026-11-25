//! HyperInfer Client Library - Data Plane

#![allow(clippy::double_must_use)]

pub mod cache;
pub mod mirroring;
pub mod router;
pub mod router_engine;
pub mod safe_resolver;
pub mod telemetry;
pub mod telemetry_otlp;
mod util;

pub use cache::ExactMatchCache;
pub use mirroring::{MirrorConfig, MirrorHandle};
pub use router::Router;
pub use router_engine::RouterEngine;
pub use telemetry::Telemetry;
pub use telemetry_otlp::{
    init_langfuse_telemetry, init_telemetry, init_telemetry_with_headers, set_gen_ai_attributes,
    set_gen_ai_response, set_gen_ai_usage, shutdown_telemetry,
};

use futures::{Stream, StreamExt};
use hyperinfer_core::{
    rate_limiting::RateLimiter, ChatChunk, ChatRequest, ChatResponse, Config, HyperInferError,
    PluginAction, PluginContext, PluginPipeline, Provider,
};
use hyperinfer_providers::{
    anthropic::AnthropicProvider, gemini::GeminiProvider, openai::OpenAiProvider, LlmProvider,
    ProviderRegistry,
};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::RwLock;
use tracing::Instrument as _;

/// Wraps a provider `ChatChunk` stream and performs the same accounting as
/// `chat()` once the stream terminates (naturally or via an error):
///
/// - Fires Redis telemetry off the critical path via `tokio::spawn`.
/// - Records output-token usage in the rate-limiter bucket.
/// - Sets OTel span usage / response attributes.
///
/// The accounting is triggered exactly once, either when a `[DONE]`-equivalent
/// chunk with a `finish_reason` is seen **or** when the stream signals
/// `Poll::Ready(None)`.
struct AccountedStream {
    inner: Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send>>,
    telemetry: Telemetry,
    rate_limiter: RateLimiter,
    key: String,
    model: String,
    start: std::time::Instant,
    /// Accumulated token counts from the stream's usage chunk (if any).
    input_tokens: u32,
    output_tokens: u32,
    /// Guards against running the accounting block more than once.
    accounted: bool,
    /// OTel span that lives for the full stream lifetime.
    span: tracing::Span,
}

impl AccountedStream {
    /// Run the accounting side-effects exactly once.
    fn account(&mut self) {
        if self.accounted {
            return;
        }
        self.accounted = true;

        let elapsed = self.start.elapsed().as_millis() as u64;
        let input_tokens = self.input_tokens;
        let output_tokens = self.output_tokens;

        let _enter = self.span.clone().entered();
        crate::telemetry_otlp::set_gen_ai_usage(&self.span, input_tokens, output_tokens);

        // Telemetry write is off the critical path (record_with_tokens spawns internally).
        let telemetry = self.telemetry.clone();
        let key = self.key.clone();
        let model = self.model.clone();
        tokio::spawn(async move {
            if let Err(e) = telemetry
                .record_with_tokens(&key, &model, input_tokens, output_tokens, elapsed)
                .await
            {
                tracing::warn!(error = %e, "stream telemetry record failed");
            }
        });

        // Rate-limiter token-bucket update is lightweight and synchronous-ish;
        // run it in a spawn to avoid blocking the poll path.
        let rate_limiter = self.rate_limiter.clone();
        let key2 = self.key.clone();
        let total = (input_tokens + output_tokens) as u64;
        tokio::spawn(async move {
            let _ = rate_limiter.record_usage(&key2, total).await;
        });
    }
}

impl Drop for AccountedStream {
    fn drop(&mut self) {
        self.account();
    }
}

impl Stream for AccountedStream {
    type Item = Result<ChatChunk, HyperInferError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Clone the span so the Entered guard holds no borrow into `self`,
        // allowing subsequent mutable borrows of other fields.
        let _enter = self.span.clone().entered();
        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                // Capture usage from whatever chunk carries it (typically the last).
                if let Some(ref u) = chunk.usage {
                    self.input_tokens = u.input_tokens;
                    self.output_tokens = u.output_tokens;
                }
                // If this chunk has a finish_reason the stream is done; account now
                // so the span attributes are set while the span is still open.
                if chunk.finish_reason.is_some() {
                    self.account();
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(e))) => {
                self.account();
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                self.account();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
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

fn normalize_provider_base_url(base_url: &str, provider: &Provider) -> String {
    let trimmed = base_url.trim_end_matches('/');
    match provider {
        Provider::OpenAI | Provider::Anthropic | Provider::Other => {
            trimmed.trim_end_matches("/v1").to_string()
        }
        Provider::Gemini | Provider::Antigravity => trimmed.to_string(),
    }
}

fn resolve_provider(
    registry: &ProviderRegistry,
    provider: &Provider,
    base_url: &str,
) -> Result<Arc<dyn LlmProvider>, HyperInferError> {
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
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string()))),
        Provider::Gemini => GeminiProvider::with_base_url(normalized)
            .map(|p| Arc::new(p) as Arc<dyn LlmProvider>)
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string()))),
        Provider::Antigravity => GeminiProvider::antigravity_with_base_url(normalized)
            .map(|p| Arc::new(p) as Arc<dyn LlmProvider>)
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string()))),
        Provider::OpenAI | Provider::Other => OpenAiProvider::with_base_url(normalized)
            .map(|p| Arc::new(p) as Arc<dyn LlmProvider>)
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string()))),
    }
}

pub struct HyperInferClient {
    config: Arc<RwLock<Config>>,
    router: Arc<Router>,
    router_engine: Arc<RouterEngine>,
    rate_limiter: RateLimiter,
    telemetry: Telemetry,
    cache: ExactMatchCache,
    mirror: MirrorHandle,
    provider_registry: Arc<ProviderRegistry>,
    plugins: PluginPipeline,
}

impl HyperInferClient {
    pub async fn new(redis_url: &str, config: Config) -> Result<Self, HyperInferError> {
        let router = Arc::new(
            Router::new(config.routing_rules.clone())
                .with_aliases(config.model_aliases.clone())
                .with_default_provider(config.default_provider.clone()),
        );

        // Create a single Redis client and connection manager shared across subsystems
        let redis_client = redis::Client::open(redis_url)
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string())))?;
        let manager = redis::aio::ConnectionManager::new(redis_client.clone())
            .await
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string())))?;

        // Share the connection manager across subsystems
        let rate_limiter = RateLimiter::from_connection_manager(manager.clone());
        let telemetry = Telemetry::from_connection_manager(manager.clone());
        let cache = ExactMatchCache::from_connection_manager(manager, "default");

        let mirror: MirrorHandle = Arc::new(RwLock::new(None));
        let config = Arc::new(RwLock::new(config));

        let provider_registry = Arc::new(ProviderRegistry::new());
        hyperinfer_providers::init_default_registry(&provider_registry);

        Ok(Self {
            config,
            router,
            router_engine: Arc::new(RouterEngine::with_redis(redis_url).await),
            rate_limiter,
            telemetry,
            cache,
            mirror,
            provider_registry,
            plugins: PluginPipeline::new(),
        })
    }

    /// Create a standalone in-memory `HyperInferClient` that requires no Redis or external server.
    pub async fn new_local(config: Config) -> Result<Self, HyperInferError> {
        let router = Arc::new(
            Router::new(config.routing_rules.clone())
                .with_aliases(config.model_aliases.clone())
                .with_default_provider(config.default_provider.clone()),
        );
        let rate_limiter = RateLimiter::new(None)
            .await
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string())))?;
        let telemetry = Telemetry::disabled();
        let cache = ExactMatchCache::disabled("local");
        let mirror: MirrorHandle = Arc::new(RwLock::new(None));
        let config = Arc::new(RwLock::new(config));

        let provider_registry = Arc::new(ProviderRegistry::new());
        hyperinfer_providers::init_default_registry(&provider_registry);

        Ok(Self {
            config,
            router,
            router_engine: Arc::new(RouterEngine::new().await),
            rate_limiter,
            telemetry,
            cache,
            mirror,
            provider_registry,
            plugins: PluginPipeline::new(),
        })
    }

    /// Register or replace a custom `LlmProvider` in this client's registry
    /// (for example, a local Ollama/vLLM endpoint or mock provider).
    pub async fn register_provider<P: hyperinfer_providers::LlmProvider + 'static>(
        &self,
        provider: P,
    ) {
        let registry = self.provider_registry.clone();
        let _ = registry.unregister(provider.name());
        registry.register(provider);
    }

    /// Configure traffic mirroring.  Pass `None` to disable.
    pub async fn set_mirror(&self, cfg: Option<MirrorConfig>) {
        let mut guard = self.mirror.write().await;
        *guard = cfg;
    }

    pub async fn inject_provider_registry(&mut self, external_registry: Arc<ProviderRegistry>) {
        let _old = std::mem::replace(&mut self.provider_registry, external_registry);
    }

    /// Add a gateway plugin to this client's pipeline (builder-style).
    pub fn with_plugin(mut self, plugin: Arc<dyn hyperinfer_core::GatewayPlugin>) -> Self {
        self.plugins.add(plugin);
        self
    }

    /// Add a gateway plugin to this client's pipeline (mutating).
    pub fn add_plugin(&mut self, plugin: Arc<dyn hyperinfer_core::GatewayPlugin>) {
        self.plugins.add(plugin);
    }

    /// Load deployments into the router engine for deployment-based routing
    pub async fn load_deployments(&self, deployments: Vec<hyperinfer_core::Deployment>) {
        self.router_engine.load_deployments(deployments).await;
    }

    /// Subscribe to Redis Pub/Sub for live deployment config changes.
    /// When a message arrives on "hyperinfer:config_updates", calls the provided
    /// fetcher function to re-fetch deployments and rebuilds the routing pool.
    pub async fn subscribe_config_updates<F, Fut>(
        &self,
        redis_url: &str,
        fetcher: F,
    ) -> Result<(), HyperInferError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Vec<hyperinfer_core::Deployment>, String>> + Send,
    {
        let client = redis::Client::open(redis_url)
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string())))?;
        let mut pubsub = client
            .get_async_pubsub()
            .await
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string())))?;

        pubsub
            .subscribe("hyperinfer:config_updates")
            .await
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string())))?;

        let engine = self.router_engine.clone();
        let _handle = tokio::spawn(async move {
            let mut stream = pubsub.on_message();
            loop {
                match stream.next().await {
                    Some(_msg) => {
                        tracing::info!(
                            "Received config update notification, re-fetching deployments"
                        );
                        match fetcher().await {
                            Ok(deployments) => {
                                engine.rebuild_pool(deployments).await;
                                tracing::info!("Rebuilt deployment pool after config update");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to re-fetch deployments after config update");
                            }
                        }
                    }
                    None => {
                        tracing::info!("Pub/Sub stream ended");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Get a reference to the router engine
    pub fn router_engine(&self) -> &Arc<RouterEngine> {
        &self.router_engine
    }

    pub async fn chat(
        &self,
        key: &str,
        request: ChatRequest,
    ) -> Result<ChatResponse, HyperInferError> {
        request.validate()?;

        // 0. Exact-match cache lookup (before rate-limiting to avoid wasting quota).
        if let Some(cached) = self.cache.get(&request).await {
            return Ok(cached);
        }

        // Create a root OTel span following the GenAI Semantic Conventions.
        // We use `.instrument(span)` on the inner async block so the span is
        // properly propagated across every `.await` point (using `span.enter()`
        // in an async function is unsafe — the guard can survive suspension).
        let span = tracing::info_span!(
            "gen_ai.chat",
            gen_ai.operation.name = "chat",
            gen_ai.request.model = %request.model,
        );

        async move {
            let start = std::time::Instant::now();

            // 1. Estimate prompt tokens and check rate limit using configured per-key quota
            let estimated_tokens = ((request
                .messages
                .iter()
                .map(|m| m.content.len())
                .sum::<usize>()
                / 4)
            .max(1)) as u64;
            let key_quota = self.config.read().await.quotas.get(key).cloned();
            let allowed = self
                .rate_limiter
                .is_allowed_with_quota(key, estimated_tokens, key_quota.as_ref())
                .await;
            if let Err(e) = allowed {
                return Err(HyperInferError::RateLimit(e.to_string()));
            }
            if !allowed.unwrap() {
                return Err(HyperInferError::RateLimit(
                    "Rate limit exceeded".to_string(),
                ));
            }

            // 1.5 Run plugin pre-request hooks
            let mut plugin_ctx = PluginContext {
                request_id: uuid::Uuid::new_v4().to_string(),
                model: request.model.clone(),
                ..Default::default()
            };
            let mut request = request;
            match self
                .plugins
                .run_pre_request(&mut plugin_ctx, &mut request)
                .await
            {
                PluginAction::ShortCircuit {
                    status_code,
                    message,
                } => {
                    return Err(HyperInferError::ApiError {
                        status: status_code,
                        message,
                    });
                }
                PluginAction::Respond(mut resp) => {
                    self.plugins
                        .run_post_response(&mut plugin_ctx, &mut resp)
                        .await;
                    return Ok(resp);
                }
                PluginAction::Continue => {}
            }

            // 2. Try deployment-based routing first (if deployments are loaded)
            let deployment_result = self.router_engine.select_deployment(&request).await;
            let (model, config_snapshot, response) = if let Ok(routing_result) = deployment_result {
                let deployment = &routing_result.deployment;
                let default_url = default_base_url_for_provider(&deployment.provider);
                let base_url = deployment.base_url.as_deref().unwrap_or(&default_url);
                let api_key = &deployment.api_key_ref;

                let provider_name = deployment.provider.to_string();
                crate::telemetry_otlp::set_gen_ai_attributes(
                    &tracing::Span::current(),
                    &provider_name,
                    &deployment.model,
                    "chat",
                );

                let llm_provider = {
                    let registry = self.provider_registry.clone();
                    match resolve_provider(&registry, &deployment.provider, base_url) {
                        Ok(p) => p,
                        Err(e) => {
                            self.router_engine.record_failure(&deployment.id).await;
                            return Err(e);
                        }
                    }
                };

                let mut resolved_request = request.clone();
                resolved_request.model = deployment.model.clone();
                match llm_provider.chat(&resolved_request, api_key).await {
                    Ok(body) => {
                        let latency = start.elapsed().as_secs_f64() * 1000.0;
                        let actual_tokens =
                            (body.usage.input_tokens + body.usage.output_tokens) as u64;
                        self.router_engine
                            .record_success(&deployment.id, latency, actual_tokens)
                            .await;

                        let config_snapshot = Arc::new(self.config.read().await.clone());
                        (deployment.model.clone(), config_snapshot, body)
                    }
                    Err(e) => {
                        self.router_engine.record_failure(&deployment.id).await;
                        self.plugins
                            .run_on_error(&mut plugin_ctx, &e.to_string())
                            .await;
                        return Err(e);
                    }
                }
            } else {
                // 3. Fallback to existing Router-based flow
                let (model, provider, api_key, config_snapshot) = {
                    let config = self.config.read().await;
                    let resolved = self.router.resolve(&request.model, &config);

                    let (model, provider) = resolved.ok_or_else(|| {
                        HyperInferError::Config(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!(
                                "Unknown model: '{}'. No routing rule or alias found.",
                                request.model
                            ),
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

                    (model, provider, api_key, Arc::new(config.clone()))
                };

                // Enrich span with the resolved provider and final model name.
                let provider_name = provider.to_string();
                crate::telemetry_otlp::set_gen_ai_attributes(
                    &tracing::Span::current(),
                    &provider_name,
                    &model,
                    "chat",
                );

                // Execute HTTP call via provider registry
                let llm_provider = {
                    let registry = self.provider_registry.clone();
                    registry.get(&provider_name).ok_or_else(|| {
                        HyperInferError::Config(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Provider '{}' not found in registry", provider_name),
                        ))
                    })?
                };

                let mut resolved_request = request.clone();
                resolved_request.model = model.clone();
                match llm_provider.chat(&resolved_request, &api_key).await {
                    Ok(resp) => (model, config_snapshot, resp),
                    Err(e) => {
                        self.plugins
                            .run_on_error(&mut plugin_ctx, &e.to_string())
                            .await;
                        return Err(e);
                    }
                }
            };

            // 3.5 Run plugin post-response hooks before telemetry and caching
            let mut response = response;
            self.plugins
                .run_post_response(&mut plugin_ctx, &mut response)
                .await;

            // 4. Record OTel usage and response attributes on the span.
            let elapsed = start.elapsed().as_millis() as u64;
            let input_tokens = response.usage.input_tokens;
            let output_tokens = response.usage.output_tokens;

            crate::telemetry_otlp::set_gen_ai_usage(
                &tracing::Span::current(),
                input_tokens,
                output_tokens,
            );

            let finish_reason = response
                .choices
                .first()
                .and_then(|c| c.finish_reason.as_deref())
                .unwrap_or("unknown");
            crate::telemetry_otlp::set_gen_ai_response(
                &tracing::Span::current(),
                &response.id,
                finish_reason,
            );

            // Store successful response in exact-match cache.
            self.cache.set(&request, &response).await;

            // Record async Redis telemetry off the critical path.
            // record_with_tokens spawns internally, so call directly.
            let telemetry = self.telemetry.clone();
            let key_owned = key.to_string();
            let model_owned = model.clone();
            if let Err(e) = telemetry
                .record_with_tokens(
                    &key_owned,
                    &model_owned,
                    input_tokens,
                    output_tokens,
                    elapsed,
                )
                .await
            {
                tracing::warn!(error = %e, "telemetry record failed");
            }

            // Record usage for rate-limiter token bucket.
            let total_tokens = response.usage.input_tokens + response.usage.output_tokens;
            let _ = self
                .rate_limiter
                .record_usage(key, total_tokens as u64)
                .await;

            // Charge delta if actual usage exceeded our estimate
            if total_tokens as u64 > estimated_tokens {
                let delta = total_tokens as u64 - estimated_tokens;
                let quota_clone = key_quota.clone();
                let rl = self.rate_limiter.clone();
                let key_owned = key.to_string();
                tokio::spawn(async move {
                    if let Err(e) = rl
                        .charge_tokens(&key_owned, delta, quota_clone.as_ref())
                        .await
                    {
                        tracing::warn!(
                            error = %e,
                            key = %key_owned,
                            "Failed to charge token delta"
                        );
                    }
                });
            }

            // 5. Fire-and-forget traffic mirror (if configured).
            let registry_for_mirror = self.provider_registry.clone();
            mirroring::maybe_mirror(
                self.mirror.clone(),
                registry_for_mirror,
                self.router.clone(),
                config_snapshot,
                key.to_string(),
                request,
            );

            // 6. Return response
            Ok(response)
        }
        .instrument(span)
        .await
    }

    /// Stream token chunks for a chat request.
    ///
    /// Returns a `Stream` of `ChatChunk` items.  The caller is responsible for
    /// collecting `delta` fields and assembling the final text.  The last chunk
    /// in the stream has a non-`None` `finish_reason` and may carry `usage`.
    ///
    /// Rate-limiting and routing follow the same logic as `chat()`.
    pub async fn chat_stream(
        &self,
        key: &str,
        request: ChatRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send>>,
        HyperInferError,
    > {
        request.validate()?;

        // 1. Rate limit check (same as non-streaming path).
        let allowed = self.rate_limiter.is_allowed(key, 1).await;
        if let Err(e) = allowed {
            return Err(HyperInferError::RateLimit(e.to_string()));
        }
        if !allowed.unwrap() {
            return Err(HyperInferError::RateLimit(
                "Rate limit exceeded".to_string(),
            ));
        }

        // 2. Try deployment-based routing first (if deployments are loaded)
        let deployment_result = self.router_engine.select_deployment(&request).await;
        let (model, provider_name, provider_stream) = if let Ok(routing_result) = deployment_result
        {
            let deployment = &routing_result.deployment;
            let default_url = default_base_url_for_provider(&deployment.provider);
            let base_url = deployment.base_url.as_deref().unwrap_or(&default_url);
            let api_key = deployment.api_key_ref.clone();

            let llm_provider = {
                let registry = self.provider_registry.clone();
                match resolve_provider(&registry, &deployment.provider, base_url) {
                    Ok(p) => p,
                    Err(e) => {
                        self.router_engine.record_failure(&deployment.id).await;
                        return Err(e);
                    }
                }
            };

            let mut resolved_request = request.clone();
            resolved_request.model = deployment.model.clone();
            let provider_stream = llm_provider.stream(&resolved_request, &api_key);

            // Record success at stream start (streaming makes per-request latency tracking harder)
            self.router_engine
                .record_success(&deployment.id, 0.0, 0)
                .await;

            (
                deployment.model.clone(),
                deployment.provider.to_string(),
                provider_stream,
            )
        } else {
            // Fallback to existing Router-based flow
            let config = self.config.read().await;
            let resolved = self.router.resolve(&request.model, &config);

            let (model, provider) = resolved.ok_or_else(|| {
                HyperInferError::Config(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "Unknown model: '{}'. No routing rule or alias found.",
                        request.model
                    ),
                ))
            })?;

            let provider_name = provider.to_string();
            let api_key = config
                .api_keys
                .get(&provider_name)
                .cloned()
                .ok_or_else(|| {
                    HyperInferError::Config(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("API key not found for provider: {:?}", provider),
                    ))
                })?;

            // Get streaming provider from registry (already checks supports_streaming)
            let streaming_provider = {
                let registry = self.provider_registry.clone();
                registry.get_streaming(&provider_name).ok_or_else(|| {
                    HyperInferError::Config(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "Provider '{}' not found in registry or does not support streaming",
                            provider_name
                        ),
                    ))
                })?
            };

            let mut resolved_request = request.clone();
            resolved_request.model = model.clone();
            let provider_stream = streaming_provider.into_stream(&resolved_request, &api_key);
            (model, provider_name, provider_stream)
        };
        // Note: streaming responses are not cached — the stream is consumed
        // incrementally by the caller so we cannot inspect it here.

        // 4. Create an OTel span for the stream lifetime and enrich it with
        //    resolved provider / model information (mirrors chat()).
        let span = tracing::info_span!(
            "gen_ai.chat_stream",
            gen_ai.operation.name = "chat_stream",
            gen_ai.request.model = %request.model,
        );
        crate::telemetry_otlp::set_gen_ai_attributes(&span, &provider_name, &model, "chat_stream");

        // 5. Wrap the provider stream so usage/telemetry are recorded on
        //    termination — the same accounting chat() performs, but deferred
        //    to when the last chunk (or an error) is polled.
        //    The span is stored inside the wrapper; poll_next enters it on
        //    every poll so it covers the full stream lifetime.
        let stream = AccountedStream {
            inner: provider_stream,
            telemetry: self.telemetry.clone(),
            rate_limiter: self.rate_limiter.clone(),
            key: key.to_string(),
            model,
            start: std::time::Instant::now(),
            input_tokens: 0,
            output_tokens: 0,
            accounted: false,
            span,
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_base_url_for_provider() {
        assert_eq!(
            default_base_url_for_provider(&Provider::OpenAI),
            "https://api.openai.com"
        );
        assert_eq!(
            default_base_url_for_provider(&Provider::Anthropic),
            "https://api.anthropic.com"
        );
        assert_eq!(
            default_base_url_for_provider(&Provider::Gemini),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            default_base_url_for_provider(&Provider::Antigravity),
            "https://cloudcode-pa.googleapis.com"
        );
        assert_eq!(
            default_base_url_for_provider(&Provider::Other),
            "https://api.openai.com"
        );
    }

    #[test]
    fn test_normalize_provider_base_url() {
        // OpenAI: strip /v1
        assert_eq!(
            normalize_provider_base_url("https://api.openai.com/v1", &Provider::OpenAI),
            "https://api.openai.com"
        );
        assert_eq!(
            normalize_provider_base_url("https://api.openai.com/v1/", &Provider::OpenAI),
            "https://api.openai.com"
        );
        assert_eq!(
            normalize_provider_base_url("https://api.openai.com", &Provider::OpenAI),
            "https://api.openai.com"
        );

        // Anthropic: strip /v1
        assert_eq!(
            normalize_provider_base_url("https://api.anthropic.com/v1", &Provider::Anthropic),
            "https://api.anthropic.com"
        );

        // Gemini: keep as-is
        assert_eq!(
            normalize_provider_base_url(
                "https://generativelanguage.googleapis.com",
                &Provider::Gemini
            ),
            "https://generativelanguage.googleapis.com"
        );

        // Custom base URL
        assert_eq!(
            normalize_provider_base_url("https://proxy.example.com/v1", &Provider::OpenAI),
            "https://proxy.example.com"
        );
    }

    #[test]
    fn test_resolve_provider_default_registry() {
        let registry = ProviderRegistry::new();
        registry.register(OpenAiProvider::new().unwrap());
        registry.register(AnthropicProvider::new().unwrap());
        registry.register(GeminiProvider::new().unwrap());

        // Default URLs should use registry providers
        let openai = resolve_provider(&registry, &Provider::OpenAI, "https://api.openai.com/v1")
            .expect("openai default");
        assert_eq!(openai.name(), "openai");

        let anthropic = resolve_provider(
            &registry,
            &Provider::Anthropic,
            "https://api.anthropic.com/v1",
        )
        .expect("anthropic default");
        assert_eq!(anthropic.name(), "anthropic");

        let gemini = resolve_provider(
            &registry,
            &Provider::Gemini,
            "https://generativelanguage.googleapis.com",
        )
        .expect("gemini default");
        assert_eq!(gemini.name(), "gemini");
    }

    #[test]
    fn test_resolve_provider_custom_base_url() {
        let registry = ProviderRegistry::new();
        registry.register(OpenAiProvider::new().unwrap());

        // Custom base URL should create a new provider instance
        let custom = resolve_provider(&registry, &Provider::OpenAI, "https://proxy.example.com/v1")
            .expect("custom openai");
        assert_eq!(custom.name(), "openai");

        let custom_anthropic = resolve_provider(
            &registry,
            &Provider::Anthropic,
            "https://proxy.example.com/v1",
        )
        .expect("custom anthropic");
        assert_eq!(custom_anthropic.name(), "anthropic");
    }

    #[tokio::test]
    async fn test_plugin_short_circuit() {
        use hyperinfer_core::{GatewayPlugin, PluginAction, PluginContext};
        use std::sync::Arc;

        #[derive(Clone)]
        struct ShortCircuitPlugin;

        #[async_trait::async_trait]
        impl GatewayPlugin for ShortCircuitPlugin {
            fn name(&self) -> &str {
                "short_circuit"
            }
            async fn pre_request(
                &self,
                _ctx: &mut PluginContext,
                _req: &mut ChatRequest,
            ) -> PluginAction {
                PluginAction::ShortCircuit {
                    status_code: 400,
                    message: "Blocked by plugin".to_string(),
                }
            }
        }

        let client = HyperInferClient::new_local(Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        })
        .await
        .unwrap();

        let client = client.with_plugin(Arc::new(ShortCircuitPlugin));

        let request = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![hyperinfer_core::ChatMessage::user("Hello")],
            ..Default::default()
        };

        let result = client.chat("test-key", request).await;
        match result {
            Err(HyperInferError::ApiError { status, message }) => {
                assert_eq!(status, 400);
                assert_eq!(message, "Blocked by plugin");
            }
            _ => panic!("Expected ApiError with status 400"),
        }
    }
}
