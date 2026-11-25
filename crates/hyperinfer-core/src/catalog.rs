//! Dynamic LLM Model Catalog & Live Pricing Registry.
//!
//! Fetches, parses, and caches up-to-date model metadata (context windows,
//! max output tokens, token pricing, and capabilities) from open registries
//! such as `https://models.dev/api.json` or OpenRouter's `/api/v1/models`,
//! eliminating hardcoded legacy model lists and stale pricing tables.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::cost::{CostCalculator, TokenCostWeights, WeightedUsage};

/// Default community-maintained model & pricing catalog URL.
pub const DEFAULT_CATALOG_URL: &str = "https://models.dev/api.json";

/// Default on-disk cache TTL (12 hours).
pub const DEFAULT_CACHE_TTL_SECS: u64 = 12 * 60 * 60;

/// Per-model pricing expressed in USD per 1 million tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPricing {
    pub input_per_1m_usd: f64,
    pub output_per_1m_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_1m_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_1m_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_per_1m_usd: Option<f64>,
}

impl ModelPricing {
    /// Convert USD-per-1M-tokens into `TokenCostWeights` (cents per 1M tokens).
    pub fn to_token_cost_weights(&self) -> TokenCostWeights {
        let input_cents = self.input_per_1m_usd * 100.0;
        let output_cents = self.output_per_1m_usd * 100.0;
        TokenCostWeights {
            input_cents_per_1m: input_cents,
            cached_input_cents_per_1m: self
                .cache_read_per_1m_usd
                .map(|v| v * 100.0)
                .unwrap_or(input_cents * 0.5),
            output_cents_per_1m: output_cents,
            reasoning_cents_per_1m: self
                .reasoning_per_1m_usd
                .map(|v| v * 100.0)
                .unwrap_or(output_cents),
        }
    }
}

/// Normalized metadata for a single model from a catalog or provider API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelSpec {
    pub id: String,
    pub provider: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<ModelPricing>,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tool_call: bool,
    #[serde(default)]
    pub source: String,
}

/// Model returned by a live provider's `list_models()` API endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredModel {
    pub id: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<ModelPricing>,
}

/// In-memory catalog of models indexed by `(provider, id)` and `id`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCatalog {
    /// Keyed by `"{provider}:{id}"` (lowercased).
    by_provider_model: HashMap<String, ModelSpec>,
    /// Keyed by lowercased `id` (canonical / first-seen provider).
    by_model: HashMap<String, ModelSpec>,
    /// Source URL or description.
    pub source: String,
    /// Unix timestamp (seconds) when this catalog was loaded/fetched.
    pub updated_at_secs: u64,
}

fn normalize_provider(provider: &str) -> String {
    match provider.trim().to_lowercase().as_str() {
        "google" | "google-ai" | "google-vertex" => "gemini".to_string(),
        other => other.to_string(),
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Default cache file path (`$XDG_CACHE_HOME/hyperinfer/model_catalog.json` or `~/.cache/hyperinfer/model_catalog.json`).
pub fn default_cache_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.trim().is_empty() {
            return Some(PathBuf::from(xdg).join("hyperinfer/model_catalog.json"));
        }
    }
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.trim().is_empty())
        .map(|home| PathBuf::from(home).join(".cache/hyperinfer/model_catalog.json"))
}

impl ModelCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.by_provider_model.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_provider_model.len()
    }

    pub fn insert(&mut self, spec: ModelSpec) {
        let norm_prov = normalize_provider(&spec.provider);
        let lower_id = spec.id.to_lowercase();
        let key = format!("{}:{}", norm_prov, lower_id);
        self.by_provider_model.insert(key, spec.clone());
        // Prefer canonical first-party providers for unscoped lookups
        let is_canonical = matches!(norm_prov.as_str(), "openai" | "anthropic" | "gemini");
        if is_canonical || !self.by_model.contains_key(&lower_id) {
            self.by_model.insert(lower_id, spec);
        }
    }

    /// Parse `https://models.dev/api.json` format.
    pub fn from_models_dev_json(raw: &str) -> Result<Self, String> {
        let root: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| format!("Invalid models.dev JSON: {e}"))?;
        let providers_obj = root
            .as_object()
            .ok_or_else(|| "Expected top-level JSON object for models.dev catalog".to_string())?;

        let mut catalog = Self {
            source: "models.dev".to_string(),
            updated_at_secs: now_unix_secs(),
            ..Default::default()
        };

        for (provider_key, provider_val) in providers_obj {
            let Some(models_obj) = provider_val.get("models").and_then(|m| m.as_object()) else {
                continue;
            };
            let norm_provider = normalize_provider(provider_key);

            for (model_key, model_val) in models_obj {
                // Skip non-chat/embedding-only entries if explicitly typed as embedding/image/audio
                if let Some(kind) = model_val.get("type").and_then(|t| t.as_str()) {
                    if matches!(kind, "embedding" | "image" | "audio" | "tts" | "stt") {
                        continue;
                    }
                }

                let id = model_val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(model_key)
                    .to_string();
                let name = model_val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&id)
                    .to_string();

                let limit = model_val.get("limit");
                let context_window = limit
                    .and_then(|l| l.get("context"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let max_output_tokens = limit
                    .and_then(|l| l.get("output"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);

                let pricing = model_val.get("cost").and_then(|c| {
                    let input = c.get("input").and_then(|v| v.as_f64())?;
                    let output = c.get("output").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    Some(ModelPricing {
                        input_per_1m_usd: input,
                        output_per_1m_usd: output,
                        cache_read_per_1m_usd: c.get("cache_read").and_then(|v| v.as_f64()),
                        cache_write_per_1m_usd: c.get("cache_write").and_then(|v| v.as_f64()),
                        reasoning_per_1m_usd: c.get("reasoning").and_then(|v| v.as_f64()),
                    })
                });

                let reasoning = model_val
                    .get("reasoning")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let tool_call = model_val
                    .get("tool_call")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                catalog.insert(ModelSpec {
                    id,
                    provider: norm_provider.clone(),
                    name,
                    context_window,
                    max_output_tokens,
                    pricing,
                    reasoning,
                    tool_call,
                    source: "models.dev".to_string(),
                });
            }
        }

        Ok(catalog)
    }

    /// Parse OpenRouter `GET /api/v1/models` JSON format (`{ "data": [ ... ] }`).
    pub fn from_openrouter_json(raw: &str) -> Result<Self, String> {
        let root: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| format!("Invalid OpenRouter JSON: {e}"))?;
        let data = root
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| "Expected top-level 'data' array for OpenRouter catalog".to_string())?;

        let mut catalog = Self {
            source: "openrouter".to_string(),
            updated_at_secs: now_unix_secs(),
            ..Default::default()
        };

        for item in data {
            let Some(full_id) = item.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let (provider, short_id) = match full_id.split_once('/') {
                Some((p, m)) => (normalize_provider(p), m.to_string()),
                None => ("openrouter".to_string(), full_id.to_string()),
            };
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(full_id)
                .to_string();
            let context_window = item
                .get("context_length")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let max_output_tokens = item
                .get("top_provider")
                .and_then(|tp| tp.get("max_completion_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let parse_per_token = |v: Option<&serde_json::Value>| -> Option<f64> {
                let raw = v?;
                let per_tok = raw
                    .as_f64()
                    .or_else(|| raw.as_str().and_then(|s| s.parse::<f64>().ok()))?;
                if per_tok >= 0.0 {
                    Some(per_tok * 1_000_000.0)
                } else {
                    None
                }
            };

            let pricing = item.get("pricing").and_then(|p| {
                let input = parse_per_token(p.get("prompt"))?;
                let output = parse_per_token(p.get("completion")).unwrap_or(0.0);
                Some(ModelPricing {
                    input_per_1m_usd: input,
                    output_per_1m_usd: output,
                    cache_read_per_1m_usd: parse_per_token(p.get("input_cache_read")),
                    cache_write_per_1m_usd: parse_per_token(p.get("input_cache_write")),
                    reasoning_per_1m_usd: parse_per_token(p.get("internal_reasoning")),
                })
            });

            catalog.insert(ModelSpec {
                id: short_id,
                provider,
                name,
                context_window,
                max_output_tokens,
                pricing,
                reasoning: false,
                tool_call: true,
                source: "openrouter".to_string(),
            });
        }

        Ok(catalog)
    }

    /// Auto-detect and parse either OpenRouter `{ "data": [...] }` or `models.dev` JSON.
    pub fn from_json_auto(raw: &str) -> Result<Self, String> {
        let peek: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| format!("Invalid catalog JSON: {e}"))?;
        if peek.get("data").and_then(|d| d.as_array()).is_some() {
            Self::from_openrouter_json(raw)
        } else if peek.get("by_provider_model").is_some() {
            serde_json::from_str(raw).map_err(|e| format!("Invalid cached catalog JSON: {e}"))
        } else {
            Self::from_models_dev_json(raw)
        }
    }

    /// Fetch and parse a live catalog from an HTTP(S) URL (10s timeout).
    pub async fn fetch_from_url(url: &str) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let resp = client
            .get(url)
            .header("User-Agent", "hyperinfer-catalog/0.2")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch catalog from {url}: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Catalog URL {url} returned HTTP {}", resp.status()));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read catalog response body: {e}"))?;

        let mut catalog = Self::from_json_auto(&body)?;
        catalog.source = url.to_string();
        Ok(catalog)
    }

    /// Load catalog from disk cache if fresh, otherwise fetch from `HYPERINFER_CATALOG_URL`
    /// (or `DEFAULT_CATALOG_URL`) and update the disk cache. Never fails startup: falls back
    /// to stale cache or an empty catalog if offline.
    pub async fn load_or_fetch(cache_path: Option<&Path>, ttl: Duration) -> Self {
        let env_url = std::env::var("HYPERINFER_CATALOG_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());

        // Support local file path via HYPERINFER_CATALOG_URL=/path/to/file.json
        if let Some(ref custom) = env_url {
            if !custom.starts_with("http://") && !custom.starts_with("https://") {
                if let Ok(raw) = tokio::fs::read_to_string(custom).await {
                    match Self::from_json_auto(&raw) {
                        Ok(cat) => return cat,
                        Err(e) => warn!("Failed to parse local catalog '{}': {}", custom, e),
                    }
                }
            }
        }

        let mut stale_cached: Option<Self> = None;
        if let Some(path) = cache_path {
            if let Ok(raw) = tokio::fs::read_to_string(path).await {
                if let Ok(cat) = Self::from_json_auto(&raw) {
                    let age = now_unix_secs().saturating_sub(cat.updated_at_secs);
                    if age < ttl.as_secs() && !cat.is_empty() {
                        debug!(
                            models = cat.len(),
                            age_secs = age,
                            "Loaded fresh model catalog from cache"
                        );
                        return cat;
                    }
                    stale_cached = Some(cat);
                }
            }
        }

        let url = env_url.as_deref().unwrap_or(DEFAULT_CATALOG_URL);
        match Self::fetch_from_url(url).await {
            Ok(cat) => {
                if let Some(path) = cache_path {
                    if let Some(parent) = path.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    if let Ok(serialized) = serde_json::to_string(&cat) {
                        let _ = tokio::fs::write(path, serialized).await;
                    }
                }
                cat
            }
            Err(e) => {
                warn!("Failed to refresh live model catalog ({}): {}", url, e);
                stale_cached.unwrap_or_default()
            }
        }
    }

    /// Look up a model spec by optional provider and model ID (exact, stripped prefix, or longest prefix).
    pub fn get(&self, provider: Option<&str>, model_id: &str) -> Option<&ModelSpec> {
        let lower_id = model_id.trim().to_lowercase();
        let stripped_id = lower_id
            .split_once('/')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_else(|| lower_id.clone());

        if let Some(prov) = provider {
            let norm_prov = normalize_provider(prov);
            if let Some(spec) = self
                .by_provider_model
                .get(&format!("{}:{}", norm_prov, lower_id))
            {
                return Some(spec);
            }
            if let Some(spec) = self
                .by_provider_model
                .get(&format!("{}:{}", norm_prov, stripped_id))
            {
                return Some(spec);
            }
        }

        if let Some(spec) = self.by_model.get(&lower_id) {
            return Some(spec);
        }
        if let Some(spec) = self.by_model.get(&stripped_id) {
            return Some(spec);
        }

        // Longest prefix match (handles dated snapshots like `claude-sonnet-4-20250514` -> `claude-sonnet-4`)
        let mut best: Option<(&str, &ModelSpec)> = None;
        for (id, spec) in &self.by_model {
            if (stripped_id.starts_with(id.as_str()) || id.starts_with(stripped_id.as_str()))
                && best.map(|(b, _)| id.len() > b.len()).unwrap_or(true)
            {
                best = Some((id.as_str(), spec));
            }
        }
        best.map(|(_, spec)| spec)
    }

    /// List all models belonging to a specific provider (handles `gemini` / `google` normalization).
    pub fn list_by_provider(&self, provider: &str) -> Vec<&ModelSpec> {
        let norm = normalize_provider(provider);
        let mut list: Vec<&ModelSpec> = self
            .by_provider_model
            .values()
            .filter(|m| m.provider == norm)
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Return all unique models in the catalog, sorted by `(provider, id)`.
    pub fn all_models(&self) -> Vec<&ModelSpec> {
        let mut list: Vec<&ModelSpec> = self.by_provider_model.values().collect();
        list.sort_by(|a, b| (&a.provider, &a.id).cmp(&(&b.provider, &b.id)));
        list
    }

    /// Build a `CostCalculator` populated from all models in this catalog that carry pricing.
    pub fn to_cost_calculator(&self) -> CostCalculator {
        let mut calc = CostCalculator::with_defaults();
        for (model_id, spec) in &self.by_model {
            if let Some(ref pricing) = spec.pricing {
                calc.set_rule(model_id.clone(), pricing.to_token_cost_weights());
            }
        }
        calc
    }

    /// Estimate USD cost for a weighted token usage record using live catalog pricing.
    pub fn estimate_cost_usd(
        &self,
        provider: Option<&str>,
        model_id: &str,
        usage: &WeightedUsage,
    ) -> Option<f64> {
        let spec = self.get(provider, model_id)?;
        let pricing = spec.pricing.as_ref()?;
        let non_cached_input = usage.input_tokens.saturating_sub(usage.cached_input_tokens);
        let cache_read_rate = pricing
            .cache_read_per_1m_usd
            .unwrap_or(pricing.input_per_1m_usd * 0.5);
        let reasoning_rate = pricing
            .reasoning_per_1m_usd
            .unwrap_or(pricing.output_per_1m_usd);

        let total = (non_cached_input as f64) * pricing.input_per_1m_usd / 1_000_000.0
            + (usage.cached_input_tokens as f64) * cache_read_rate / 1_000_000.0
            + (usage.output_tokens as f64) * pricing.output_per_1m_usd / 1_000_000.0
            + (usage.reasoning_tokens as f64) * reasoning_rate / 1_000_000.0;
        Some(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MODELS_DEV: &str = r#"{
        "openai": {
            "models": {
                "gpt-6.2": {
                    "id": "gpt-6.2",
                    "name": "GPT 6.2",
                    "reasoning": true,
                    "tool_call": true,
                    "limit": { "context": 1000000, "output": 128000 },
                    "cost": { "input": 8.0, "output": 40.0, "cache_read": 0.4, "cache_write": 10.0 }
                },
                "text-embedding-ada-002": {
                    "id": "text-embedding-ada-002",
                    "type": "embedding",
                    "name": "Ada Embedding",
                    "limit": { "context": 8192, "output": 1536 },
                    "cost": { "input": 0.1, "output": 0.0 }
                }
            }
        },
        "google": {
            "models": {
                "gemini-3.5-flash-lite": {
                    "id": "gemini-3.5-flash-lite",
                    "name": "Gemini 3.5 Flash Lite",
                    "reasoning": true,
                    "tool_call": true,
                    "limit": { "context": 1048576, "output": 65536 },
                    "cost": { "input": 0.3, "output": 2.5, "cache_read": 0.03 }
                }
            }
        }
    }"#;

    const SAMPLE_OPENROUTER: &str = r#"{
        "data": [
            {
                "id": "anthropic/claude-fable-5",
                "name": "Claude Fable 5",
                "context_length": 1000000,
                "pricing": {
                    "prompt": "0.00001",
                    "completion": "0.00005",
                    "input_cache_read": "0.000001"
                }
            }
        ]
    }"#;

    #[test]
    fn test_parse_models_dev_and_skip_embeddings() {
        let cat = ModelCatalog::from_models_dev_json(SAMPLE_MODELS_DEV).unwrap();
        assert_eq!(cat.len(), 2);
        assert!(cat.get(Some("openai"), "text-embedding-ada-002").is_none());

        let gpt = cat.get(Some("openai"), "gpt-6.2").unwrap();
        assert_eq!(gpt.context_window, Some(1_000_000));
        assert_eq!(gpt.pricing.as_ref().unwrap().input_per_1m_usd, 8.0);
        assert_eq!(
            gpt.pricing.as_ref().unwrap().cache_read_per_1m_usd,
            Some(0.4)
        );

        // Google normalizes to gemini
        let gem = cat.get(Some("gemini"), "gemini-3.5-flash-lite").unwrap();
        assert_eq!(gem.provider, "gemini");
        assert_eq!(gem.context_window, Some(1_048_576));
    }

    #[test]
    fn test_parse_openrouter_and_cost_calculation() {
        let cat = ModelCatalog::from_json_auto(SAMPLE_OPENROUTER).unwrap();
        let spec = cat.get(Some("anthropic"), "claude-fable-5").unwrap();
        assert_eq!(spec.context_window, Some(1_000_000));
        let pricing = spec.pricing.as_ref().unwrap();
        assert!((pricing.input_per_1m_usd - 10.0).abs() < 1e-6);
        assert!((pricing.output_per_1m_usd - 50.0).abs() < 1e-6);
        assert!((pricing.cache_read_per_1m_usd.unwrap() - 1.0).abs() < 1e-6);

        let usage = WeightedUsage {
            input_tokens: 1_000_000,
            cached_input_tokens: 500_000,
            output_tokens: 100_000,
            reasoning_tokens: 0,
        };
        // 0.5M * $10 + 0.5M * $1 + 0.1M * $50 = $5 + $0.5 + $5 = $10.50
        let usd = cat
            .estimate_cost_usd(Some("anthropic"), "claude-fable-5", &usage)
            .unwrap();
        assert!((usd - 10.5).abs() < 1e-6);

        let calc = cat.to_cost_calculator();
        let cents = calc.calculate_cost_cents("claude-fable-5", &usage);
        assert!((cents - 1050.0).abs() < 1e-4);
    }
}
