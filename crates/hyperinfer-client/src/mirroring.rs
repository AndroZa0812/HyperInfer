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

use crate::HttpCaller;
use crate::Router;
use hyperinfer_core::{types::Provider, ChatRequest, Config};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

/// Configuration for the traffic mirror.
#[derive(Debug, Clone)]
pub struct MirrorConfig {
    /// Target model for the shadow request (e.g. `"claude-3-5-sonnet-20241022"`).
    pub model: String,
    /// Fraction of requests to mirror in `[0.0, 1.0]`.
    /// `1.0` means mirror every request; `0.0` disables mirroring.
    pub sample_rate: f64,
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
    tokio::spawn(async move {
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
                let content = resp
                    .choices
                    .first()
                    .map(|c| c.message.content.as_str())
                    .unwrap_or("<empty>");
                tracing::debug!(
                    mirror_model = %model,
                    input_tokens = resp.usage.input_tokens,
                    output_tokens = resp.usage.output_tokens,
                    "Mirror response (first 120 chars): {}",
                    &content[..content.len().min(120)]
                );
            }
            Err(e) => {
                warn!("Mirror request failed: {:?}", e);
            }
        }
    });
}

/// Cheap pseudo-random float in `[0.0, 1.0)` without an external RNG dep.
/// Uses the low bits of the current monotonic instant.
fn rand_f64() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // Mix the bits a little.
    let mixed = nanos.wrapping_mul(2654435761);
    (mixed as f64) / (u32::MAX as f64 + 1.0)
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

    #[test]
    fn test_rand_f64_in_range() {
        for _ in 0..100 {
            let v = rand_f64();
            assert!((0.0..1.0).contains(&v));
        }
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
        };

        maybe_mirror(handle, http, router, config, "key".to_string(), request);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
