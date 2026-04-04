//! Redis-backed exact-match response cache.
//!
//! The cache key is a SHA-256 hash of the canonical JSON serialization of the
//! [`ChatRequest`].  Because the request struct derives [`Serialize`], the
//! hash is deterministic for identical requests regardless of field insertion
//! order (serde always serialises struct fields in declaration order).
//!
//! Cache entries expire after [`DEFAULT_TTL_SECS`] seconds; callers can
//! override this via [`ExactMatchCache::with_ttl`].
//!
//! The cache gracefully degrades: if Redis is unavailable all `get`/`set`
//! calls return `None`/`Ok(())` without surfacing errors to the caller.

use hyperinfer_core::{ChatRequest, ChatResponse};
use redis::{aio::ConnectionManager, AsyncCommands};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Default TTL for cached responses (5 minutes).
pub const DEFAULT_TTL_SECS: u64 = 300;

/// Exact-match Redis cache for [`ChatResponse`] values.
#[derive(Clone)]
pub struct ExactMatchCache {
    conn: Option<Arc<Mutex<ConnectionManager>>>,
    ttl_secs: u64,
    /// Namespace for cache keys to avoid cross-client collisions.
    namespace: String,
}

impl ExactMatchCache {
    /// Connect to Redis at `redis_url`.  On failure the cache is disabled and
    /// all operations become no-ops.
    pub async fn new(redis_url: &str, namespace: &str) -> Self {
        match redis::Client::open(redis_url) {
            Ok(client) => match ConnectionManager::new(client).await {
                Ok(mgr) => {
                    debug!("ExactMatchCache: connected to Redis");
                    Self {
                        conn: Some(Arc::new(Mutex::new(mgr))),
                        ttl_secs: DEFAULT_TTL_SECS,
                        namespace: namespace.to_string(),
                    }
                }
                Err(e) => {
                    warn!(
                        "ExactMatchCache: Redis connection failed: {}; cache disabled",
                        e
                    );
                    Self {
                        conn: None,
                        ttl_secs: DEFAULT_TTL_SECS,
                        namespace: namespace.to_string(),
                    }
                }
            },
            Err(e) => {
                warn!("ExactMatchCache: invalid Redis URL: {}; cache disabled", e);
                Self {
                    conn: None,
                    ttl_secs: DEFAULT_TTL_SECS,
                    namespace: namespace.to_string(),
                }
            }
        }
    }

    /// Override the cache TTL.  Returns `self` for chaining.
    pub fn with_ttl(mut self, secs: u64) -> Self {
        self.ttl_secs = secs;
        self
    }

    /// Compute the cache key for `request`.
    pub fn cache_key(&self, request: &ChatRequest) -> Option<String> {
        // Clone and normalize to ignore streaming preference
        let mut normalized_request = request.clone();
        normalized_request.stream = None;

        match serde_json::to_string(&normalized_request) {
            Ok(json) => {
                let mut hasher = Sha256::new();
                hasher.update(json.as_bytes());
                let hash = format!("{:x}", hasher.finalize());
                Some(format!("hyperinfer:cache:{}:{}", self.namespace, hash))
            }
            Err(e) => {
                warn!("Cache key serialisation error: {}", e);
                None
            }
        }
    }

    /// Attempt to retrieve a cached [`ChatResponse`] for `request`.
    ///
    /// Returns `None` on cache miss, Redis error, or deserialisation failure.
    pub async fn get(&self, request: &ChatRequest) -> Option<ChatResponse> {
        let conn = self.conn.as_ref()?;
        let key = self.cache_key(request)?;

        let mut guard = conn.lock().await;
        let raw: Option<String> = guard.get(&key).await.ok()?;
        drop(guard);

        let raw = raw?;
        match serde_json::from_str::<ChatResponse>(&raw) {
            Ok(resp) => {
                debug!("Cache HIT for key {}", key);
                Some(resp)
            }
            Err(e) => {
                warn!("Cache deserialisation error: {}", e);
                None
            }
        }
    }

    /// Store `response` in the cache under the key derived from `request`.
    ///
    /// Silently ignores serialisation and Redis errors.
    pub async fn set(&self, request: &ChatRequest, response: &ChatResponse) {
        let conn = match self.conn.as_ref() {
            Some(c) => c,
            None => return,
        };

        let key = match self.cache_key(request) {
            Some(k) => k,
            None => return,
        };
        let raw = match serde_json::to_string(response) {
            Ok(s) => s,
            Err(e) => {
                warn!("Cache serialisation error: {}", e);
                return;
            }
        };

        let mut guard = conn.lock().await;
        let result: redis::RedisResult<()> = guard.set_ex(&key, &raw, self.ttl_secs).await;
        drop(guard);

        if let Err(e) = result {
            warn!("Cache write error: {}", e);
        } else {
            debug!("Cache SET key {} ttl={}s", key, self.ttl_secs);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hyperinfer_core::{
        types::{ChatMessage, Choice, MessageRole, Usage},
        ChatRequest, ChatResponse,
    };

    fn sample_request(model: &str) -> ChatRequest {
        ChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
            }],
            max_tokens: Some(100),
            temperature: None,
            stream: None,
            stop: None,
        }
    }

    fn sample_response() -> ChatResponse {
        ChatResponse {
            id: "resp-test".to_string(),
            model: model_unused(),
            choices: vec![Choice {
                message: ChatMessage {
                    role: MessageRole::Assistant,
                    content: "Hi there!".to_string(),
                },
                finish_reason: Some("stop".to_string()),
                index: 0,
            }],
            usage: Usage {
                input_tokens: 5,
                output_tokens: 10,
            },
        }
    }

    fn model_unused() -> String {
        "gpt-4".to_string()
    }

    #[test]
    fn test_cache_key_deterministic() {
        let req = sample_request("gpt-4");
        let cache = ExactMatchCache {
            conn: None,
            ttl_secs: DEFAULT_TTL_SECS,
            namespace: "test-ns".to_string(),
        };
        let k1 = cache.cache_key(&req);
        let k2 = cache.cache_key(&req);
        assert_eq!(k1, k2);
        assert!(k1.unwrap().starts_with("hyperinfer:cache:test-ns:"));
    }

    #[test]
    fn test_cache_key_different_models() {
        let cache = ExactMatchCache {
            conn: None,
            ttl_secs: DEFAULT_TTL_SECS,
            namespace: "test-ns".to_string(),
        };
        let k1 = cache.cache_key(&sample_request("gpt-4"));
        let k2 = cache.cache_key(&sample_request("claude-3"));
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_cache_key_different_messages() {
        let cache = ExactMatchCache {
            conn: None,
            ttl_secs: DEFAULT_TTL_SECS,
            namespace: "test-ns".to_string(),
        };
        let mut r1 = sample_request("gpt-4");
        let mut r2 = sample_request("gpt-4");
        r1.messages[0].content = "hello".to_string();
        r2.messages[0].content = "goodbye".to_string();
        assert_ne!(cache.cache_key(&r1), cache.cache_key(&r2));
    }

    #[test]
    fn test_cache_key_ignores_stream() {
        let cache = ExactMatchCache {
            conn: None,
            ttl_secs: DEFAULT_TTL_SECS,
            namespace: "test-ns".to_string(),
        };
        let mut r1 = sample_request("gpt-4");
        r1.stream = Some(true);

        let mut r2 = sample_request("gpt-4");
        r2.stream = Some(false);

        let mut r3 = sample_request("gpt-4");
        r3.stream = None;

        let k1 = cache.cache_key(&r1);
        let k2 = cache.cache_key(&r2);
        let k3 = cache.cache_key(&r3);

        assert_eq!(k1, k2);
        assert_eq!(k2, k3);
    }

    #[tokio::test]
    async fn test_cache_disabled_get_returns_none() {
        // Build a cache with an invalid URL → disabled.
        let cache = ExactMatchCache::new("redis://invalid-host:1", "test-ns").await;
        let req = sample_request("gpt-4");
        let result = cache.get(&req).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_disabled_set_no_panic() {
        let cache = ExactMatchCache::new("redis://invalid-host:1", "test-ns").await;
        let req = sample_request("gpt-4");
        let resp = sample_response();
        // Should not panic.
        cache.set(&req, &resp).await;
    }

    #[test]
    fn test_with_ttl() {
        // Verify the builder stores the custom TTL.
        // We can't easily call async new in a sync test, so test the field
        // directly by constructing a disabled cache inline.
        let cache = ExactMatchCache {
            conn: None,
            ttl_secs: DEFAULT_TTL_SECS,
            namespace: "test-ns".to_string(),
        };
        let cache = cache.with_ttl(60);
        assert_eq!(cache.ttl_secs, 60);
    }

    // Note: this test simulates the Redis deserialisation error.
    // However, owing to integration test environment constraints (Docker is failing to mount overlayfs),
    // we use a more lightweight unit test approach: test the exact component.
    // Wait, since we are doing `cache.get()`, it relies on redis to return raw string.
    // What if we test an invalid URL but use redis mock? There is no redis mock.
    // Let's actually test it by asserting the match behavior directly, or if we can't,
    // we use the local running redis if REDIS_URL is present, otherwise we skip.
    #[tokio::test]
    async fn test_cache_deserialisation_error() {
        // Fallback to local redis if available, or skip if no Redis server
        let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

        let cache = ExactMatchCache::new(&redis_url, "test-ns-malformed").await;

        if cache.conn.is_none() {
            println!("Skipping test: Redis not available at {}", redis_url);
            return;
        }

        let req = sample_request("gpt-4");
        let key = cache.cache_key(&req).unwrap();

        // Directly insert malformed JSON into Redis
        let client = redis::Client::open(redis_url.as_str()).unwrap();
        let mut conn = client.get_multiplexed_async_connection().await.unwrap();
        let _: () = redis::cmd("SET")
            .arg(&key)
            .arg("not valid json")
            .query_async(&mut conn)
            .await
            .unwrap();

        // The get should return None and not panic
        let result = cache.get(&req).await;
        assert!(result.is_none(), "Deserialization error should result in a cache miss (None)");
    }
}
