//! HyperInfer Client Library - Data Plane

pub mod cache;
pub mod http_client;
pub mod mirroring;
pub mod router;
pub mod telemetry;
pub mod telemetry_otlp;

pub use cache::ExactMatchCache;
pub use http_client::HttpCaller;
pub use mirroring::{MirrorConfig, MirrorHandle};
pub use router::Router;
pub use telemetry::Telemetry;
pub use telemetry_otlp::{
    init_langfuse_telemetry, init_telemetry, init_telemetry_with_headers, set_gen_ai_attributes,
    set_gen_ai_response, set_gen_ai_usage, shutdown_telemetry,
};

use futures::Stream;
use hyperinfer_core::{
    rate_limiting::RateLimiter, types::Provider, ChatChunk, ChatRequest, ChatResponse, Config,
    HyperInferError,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::Instrument as _;

pub struct HyperInferClient {
    config: Arc<RwLock<Config>>,
    http_caller: Arc<HttpCaller>,
    router: Arc<Router>,
    rate_limiter: RateLimiter,
    telemetry: Telemetry,
    cache: ExactMatchCache,
    mirror: MirrorHandle,
}

impl HyperInferClient {
    pub async fn new(redis_url: &str, config: Config) -> Result<Self, HyperInferError> {
        let http_caller = Arc::new(HttpCaller::new().map_err(HyperInferError::Http)?);
        let router = Arc::new(
            Router::new(config.routing_rules.clone())
                .with_aliases(config.model_aliases.clone())
                .with_default_provider(config.default_provider.clone()),
        );
        let rate_limiter = RateLimiter::new(Some(redis_url))
            .await
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string())))?;
        let telemetry = Telemetry::new(redis_url)
            .await
            .map_err(|e| HyperInferError::Config(std::io::Error::other(e.to_string())))?;
        let cache = ExactMatchCache::new(redis_url).await;
        let mirror: MirrorHandle = Arc::new(RwLock::new(None));
        let config = Arc::new(RwLock::new(config));

        Ok(Self {
            config,
            http_caller,
            router,
            rate_limiter,
            telemetry,
            cache,
            mirror,
        })
    }

    /// Configure traffic mirroring.  Pass `None` to disable.
    pub async fn set_mirror(&self, cfg: Option<MirrorConfig>) {
        let mut guard = self.mirror.write().await;
        *guard = cfg;
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

            // Record async Redis telemetry (fire-and-forget).
            let _ = self
                .telemetry
                .record_with_tokens(key, &model, input_tokens, output_tokens, elapsed)
                .await;

            // Record usage for rate-limiter token bucket.
            let total_tokens = response.usage.input_tokens + response.usage.output_tokens;
            let _ = self
                .rate_limiter
                .record_usage(key, total_tokens as u64)
                .await;

            // 5. Fire-and-forget traffic mirror (if configured).
            mirroring::maybe_mirror(
                self.mirror.clone(),
                self.http_caller.clone(),
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

        // 2. Resolve model / provider / api key.
        let (model, provider, api_key) = {
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

            (model, provider, api_key)
        };

        // TODO(phase4): Add OTel span (using .instrument()) and record rate-limiter
        // token usage once the stream is exhausted (requires wrapping the stream).

        // 3. Dispatch to the correct SSE stream.
        let stream: Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send>> =
            match provider {
                Provider::OpenAI => self.http_caller.stream_openai(&model, &api_key, &request),
                Provider::Anthropic => self
                    .http_caller
                    .stream_anthropic(&model, &api_key, &request),
                _ => {
                    return Err(HyperInferError::Config(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "Unsupported provider for streaming",
                    )));
                }
            };
        // Note: streaming responses are not cached — the stream is consumed
        // incrementally by the caller so we cannot inspect it here.

        Ok(stream)
    }
}
