//! Traffic mirroring: fire-and-forget shadow requests to a secondary model.
//!
//! When enabled, every successful primary call also spawns a background
//! [`tokio::task`] that repeats the same request against a configured
//! "mirror" model.  The mirror response is **not** returned to the caller;
//! it is simply logged so operators can compare primary vs. mirror results
//! (e.g. when evaluating a model switch).
//!
//! # Usage
//!
//! ```rust,ignore
//! let mirror = MirrorConfig {
//!     model: "claude-3-5-sonnet-20241022".to_string(),
//!     sample_rate: 0.5,   // mirror 50 % of requests
//! };
//! client.set_mirror(Some(mirror));
//! ```
//!
//! Thread-safety: the config is stored behind an `Arc<RwLock<…>>` so it can
//! be hot-swapped at runtime without restarting the client.

use crate::util::rand_f64;
use crate::HttpCaller;
use crate::Router;
use hyperinfer_core::{types::Provider, ChatRequest, Config};
use std::sync::{Arc, OnceLock};
use tokio::sync::{RwLock, Semaphore};
use tracing::warn;

/// Maximum number of concurrent background mirror tasks.
const MIRROR_CONCURRENCY_LIMIT: usize = 100;

/// Module-level semaphore — shared across all `maybe_mirror` calls so we never
/// have more than `MIRROR_CONCURRENCY_LIMIT` in-flight mirror tasks at once.
fn mirror_semaphore() -> &'static Arc<Semaphore> {
    static SEM: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SEM.get_or_init(|| Arc::new(Semaphore::new(MIRROR_CONCURRENCY_LIMIT)))
}

/// Configuration for the traffic mirror.
#[derive(Debug, Clone)]
pub struct MirrorConfig {
    /// Target model for the shadow request (e.g. `"claude-3-5-sonnet-20241022"`).
    pub model: String,
    /// Fraction of requests to mirror in `[0.0, 1.0]`.
    /// `1.0` means mirror every request; `0.0` disables mirroring.
    pub sample_rate: f64,
}

impl MirrorConfig {
    pub fn new(model: String, sample_rate: f64) -> Self {
        Self {
            model,
            sample_rate: sample_rate.clamp(0.0, 1.0),
        }
    }
}

/// Shared, hot-swappable mirror configuration.
pub type MirrorHandle = Arc<RwLock<Option<MirrorConfig>>>;

/// Spawn a fire-and-forget background task that sends `request` to the mirror
/// model.  Errors are logged but never propagated.
///
/// The function returns immediately; the mirror call runs concurrently.
pub fn maybe_mirror(
    mirror_handle: MirrorHandle,
    http_caller: Arc<HttpCaller>,
    router: Arc<Router>,
    config_snapshot: Arc<Config>,
    _key: String,
    mut request: ChatRequest,
) {
    // Acquire a permit before spawning; if already at capacity, skip this mirror
    // request to avoid unbounded task growth.
    let permit = match mirror_semaphore().clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            tracing::debug!("Mirror skipped: concurrency limit reached");
            return;
        }
    };

    tokio::spawn(async move {
        // Hold the permit for the lifetime of this task.
        let _permit = permit;

        // Sample check — read the mirror config, bail out quickly if disabled.
        let mirror_cfg = {
            let guard = mirror_handle.read().await;
            match guard.as_ref() {
                Some(cfg) if cfg.sample_rate > 0.0 => cfg.clone(),
                _ => return,
            }
        };

        // Probabilistic sampling.
        if mirror_cfg.sample_rate < 1.0 {
            let roll: f64 = rand_f64();
            if roll > mirror_cfg.sample_rate {
                tracing::debug!(
                    "Mirror skipped (sample_rate={:.2}, roll={:.2})",
                    mirror_cfg.sample_rate,
                    roll
                );
                return;
            }
        }

        // Rewrite the model to the mirror target.
        request.model = mirror_cfg.model.clone();

        // Resolve provider for the mirror model.
        let resolved = router.resolve(&request.model, &config_snapshot);
        let (model, provider) = match resolved {
            Some(r) => r,
            None => {
                warn!(
                    "Mirror: could not resolve model '{}', skipping",
                    request.model
                );
                return;
            }
        };

        let api_key = match config_snapshot.api_keys.get(&provider.to_string()) {
            Some(k) => k.clone(),
            None => {
                warn!("Mirror: no API key for provider {:?}", provider);
                return;
            }
        };

        let result = match provider {
            Provider::OpenAI => http_caller.call_openai(&model, &api_key, &request).await,
            Provider::Anthropic => http_caller.call_anthropic(&model, &api_key, &request).await,
            _ => {
                warn!("Mirror: unsupported provider {:?}", provider);
                return;
            }
        };

        match result {
            Ok(resp) => {
                let content_len = resp
                    .choices
                    .first()
                    .map(|c| c.message.content.len())
                    .unwrap_or(0);
                tracing::debug!(
                    mirror_model = %model,
                    input_tokens = resp.usage.input_tokens,
                    output_tokens = resp.usage.output_tokens,
                    content_len,
                    "Mirror response received",
                );
            }
            Err(e) => {
                warn!("Mirror request failed: {:?}", e);
            }
        }
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hyperinfer_core::types::Config;
    use std::collections::HashMap;

    fn empty_config() -> Config {
        Config {
            api_keys: HashMap::new(),
            routing_rules: vec![],
            quotas: HashMap::new(),
            model_aliases: HashMap::new(),
            default_provider: None,
        }
    }

    #[test]
    fn test_mirror_config_clone() {
        let cfg = MirrorConfig {
            model: "gpt-4o".to_string(),
            sample_rate: 0.5,
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.model, "gpt-4o");
        assert!((cloned.sample_rate - 0.5).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_maybe_mirror_disabled_no_panic() {
        // With sample_rate = 0.0 the spawn fires but exits immediately.
        let handle: MirrorHandle = Arc::new(RwLock::new(Some(MirrorConfig {
            model: "gpt-4o".to_string(),
            sample_rate: 0.0,
        })));
        let http = Arc::new(HttpCaller::new().unwrap());
        let router = Arc::new(Router::new(vec![]));
        let config = Arc::new(empty_config());

        let request = hyperinfer_core::ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![hyperinfer_core::types::ChatMessage {
                role: hyperinfer_core::types::MessageRole::User,
                content: "hello".to_string(),
            }],
            max_tokens: Some(10),
            temperature: None,
            stream: None,
            stop: None,
        };

        maybe_mirror(handle, http, router, config, "key".to_string(), request);
        // Allow the task to run and exit.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn test_maybe_mirror_none_config_no_panic() {
        // With None config nothing should happen.
        let handle: MirrorHandle = Arc::new(RwLock::new(None));
        let http = Arc::new(HttpCaller::new().unwrap());
        let router = Arc::new(Router::new(vec![]));
        let config = Arc::new(empty_config());

        let request = hyperinfer_core::ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![hyperinfer_core::types::ChatMessage {
                role: hyperinfer_core::types::MessageRole::User,
                content: "hello".to_string(),
            }],
            max_tokens: Some(10),
            temperature: None,
            stream: None,
            stop: None,
        };

        maybe_mirror(handle, http, router, config, "key".to_string(), request);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn test_maybe_mirror_unresolvable_model_no_panic() {
        // Mirror model that cannot be resolved → logs warning, no panic.
        let handle: MirrorHandle = Arc::new(RwLock::new(Some(MirrorConfig {
            model: "unknown-llm-xyz".to_string(),
            sample_rate: 1.0,
        })));
        let http = Arc::new(HttpCaller::new().unwrap());
        let router = Arc::new(Router::new(vec![]));
        let config = Arc::new(empty_config());

        let request = hyperinfer_core::ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![hyperinfer_core::types::ChatMessage {
                role: hyperinfer_core::types::MessageRole::User,
                content: "hello".to_string(),
            }],
            max_tokens: Some(10),
            temperature: None,
            stream: None,
            stop: None,
        };

        maybe_mirror(handle, http, router, config, "key".to_string(), request);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    static SERIALIZE_TESTS: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

    fn serializer() -> &'static tokio::sync::Mutex<()> {
        SERIALIZE_TESTS.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    #[tokio::test]
    async fn test_maybe_mirror_concurrency_limit_no_panic() {
        let _guard = serializer().lock().await;

        let sem = mirror_semaphore();

        let mut acquired = Vec::new();
        for _ in 0..MIRROR_CONCURRENCY_LIMIT {
            let permit = sem
                .clone()
                .try_acquire_owned()
                .expect("should acquire permit");
            acquired.push(permit);
        }

        assert_eq!(sem.available_permits(), 0);

        let handle: MirrorHandle = Arc::new(RwLock::new(Some(MirrorConfig {
            model: "gpt-4o".to_string(),
            sample_rate: 1.0,
        })));
        let http = Arc::new(HttpCaller::new().unwrap());
        let router = Arc::new(Router::new(vec![]));
        let config = Arc::new(empty_config());

        let request = hyperinfer_core::ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![hyperinfer_core::types::ChatMessage {
                role: hyperinfer_core::types::MessageRole::User,
                content: "hello".to_string(),
            }],
            max_tokens: Some(10),
            temperature: None,
            stream: None,
            stop: None,
        };

        maybe_mirror(handle, http, router, config, "key".to_string(), request);

        let acquired_after = sem.clone().try_acquire_owned();
        assert!(
            acquired_after.is_err(),
            "maybe_mirror should not have acquired a permit when at capacity"
        );

        drop(acquired);

        assert_eq!(
            sem.available_permits(),
            MIRROR_CONCURRENCY_LIMIT,
            "all permits should be released"
        );
    }
}
