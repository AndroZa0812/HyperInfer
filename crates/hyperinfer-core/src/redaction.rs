//! PII and secret redaction plugin for gateway telemetry and logs.
//!
//! Scrubs API keys, bearer tokens, emails, SSNs, and custom patterns
//! from request/response content before logging or telemetry emission.
//! Supports opt-in reversible tokenization (`RedactionVault` + `StreamRehydrator`)
//! and smart semantic redaction via Microsoft Presidio or a local/trusted LLM.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::plugin::{GatewayPlugin, PluginAction, PluginContext};
use crate::redaction_smart::{redact_with_llm, redact_with_presidio};
use crate::types::{ChatMessage, ChatRequest, ChatResponse, ContentPart};

pub use crate::redaction_smart::{
    PlaceholderFormat, RedactionVault, SmartRedactionBackend, SmartRedactionConfig,
    SmartRedactionFailMode, StreamRehydrator, REDACTION_VAULT_METADATA_KEY, SKIP_REDACTION_HEADER,
    SKIP_REDACTION_METADATA_KEY,
};

/// Configuration for the redaction plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RedactionConfig {
    pub redact_api_keys: bool,
    pub redact_bearer_tokens: bool,
    pub redact_emails: bool,
    pub redact_ssns: bool,
    pub custom_patterns: Vec<(String, String)>,
    pub redact_requests: bool,
    pub redact_responses: bool,
    /// Opt-in reversible redaction: tokenize secrets per request and restore them
    /// in the response. Defaults to `false` (one-way redaction).
    pub reversible: bool,
    /// Placeholder format used when `reversible` is enabled.
    pub placeholder_format: PlaceholderFormat,
    /// Optional smart redaction (Microsoft Presidio sidecar or local/trusted LLM).
    pub smart: SmartRedactionConfig,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            redact_api_keys: true,
            redact_bearer_tokens: true,
            redact_emails: true,
            redact_ssns: true,
            custom_patterns: Vec::new(),
            redact_requests: true,
            redact_responses: true,
            reversible: false,
            placeholder_format: PlaceholderFormat::Indexed,
            smart: SmartRedactionConfig::default(),
        }
    }
}

struct CompiledPattern {
    regex: Regex,
    static_replacement: String,
    category: String,
    preserve_bearer_prefix: bool,
}

struct CompiledRedaction {
    config: RedactionConfig,
    patterns: Vec<CompiledPattern>,
}

impl CompiledRedaction {
    fn compile(config: RedactionConfig) -> Self {
        let mut patterns = Vec::new();

        if config.redact_api_keys {
            let re = Regex::new(
                r"\b(?:sk-ant-|sk-proj-|sk-|hi-local-|hi-admin-|hi-)[A-Za-z0-9_-]{12,}\b",
            )
            .expect("api key regex is valid");
            patterns.push(CompiledPattern {
                regex: re,
                static_replacement: "[REDACTED_API_KEY]".to_string(),
                category: "API_KEY".to_string(),
                preserve_bearer_prefix: false,
            });
        }

        if config.redact_bearer_tokens {
            let re = Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{8,}")
                .expect("bearer token regex is valid");
            patterns.push(CompiledPattern {
                regex: re,
                static_replacement: "Bearer [REDACTED_TOKEN]".to_string(),
                category: "TOKEN".to_string(),
                preserve_bearer_prefix: true,
            });
        }

        if config.redact_emails {
            let re = Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b")
                .expect("email regex is valid");
            patterns.push(CompiledPattern {
                regex: re,
                static_replacement: "[REDACTED_EMAIL]".to_string(),
                category: "EMAIL".to_string(),
                preserve_bearer_prefix: false,
            });
        }

        if config.redact_ssns {
            let re = Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("SSN regex is valid");
            patterns.push(CompiledPattern {
                regex: re,
                static_replacement: "[REDACTED_SSN]".to_string(),
                category: "SSN".to_string(),
                preserve_bearer_prefix: false,
            });
        }

        for (idx, (pattern, replacement)) in config.custom_patterns.iter().enumerate() {
            match Regex::new(pattern) {
                Ok(re) => {
                    let cat = derive_custom_category(replacement, idx + 1);
                    patterns.push(CompiledPattern {
                        regex: re,
                        static_replacement: replacement.to_string(),
                        category: cat,
                        preserve_bearer_prefix: false,
                    });
                }
                Err(e) => warn!("invalid custom redaction pattern '{}': {}", pattern, e),
            }
        }

        Self { config, patterns }
    }
}

fn derive_custom_category(replacement: &str, idx: usize) -> String {
    let trimmed = replacement.trim().trim_matches(|c| c == '[' || c == ']');
    let stripped = trimmed
        .strip_prefix("REDACTED_")
        .or_else(|| trimmed.strip_prefix("REDACTED"))
        .unwrap_or(trimmed);
    let normalized: String = stripped
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = normalized.trim_matches('_');
    if cleaned.is_empty() {
        format!("CUSTOM_{}", idx)
    } else {
        cleaned.to_string()
    }
}

/// Redaction plugin that scrubs sensitive data from requests and responses.
/// Supports hot-reloading its `RedactionConfig` at runtime.
#[derive(Clone)]
pub struct RedactionPlugin {
    inner: Arc<RwLock<CompiledRedaction>>,
    http_client: reqwest::Client,
}

impl RedactionPlugin {
    pub fn new(config: RedactionConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            inner: Arc::new(RwLock::new(CompiledRedaction::compile(config))),
            http_client,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(RedactionConfig::default())
    }

    /// Hot-reload the active redaction configuration.
    pub fn update_config(&self, config: RedactionConfig) {
        let compiled = CompiledRedaction::compile(config);
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *guard = compiled;
    }

    /// Return a snapshot of the active redaction configuration.
    pub fn config(&self) -> RedactionConfig {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .config
            .clone()
    }

    /// Synchronous one-way regex redaction (backward compatible).
    pub fn redact_str(&self, input: &str) -> String {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let mut result = input.to_string();
        for pat in &guard.patterns {
            result = pat
                .regex
                .replace_all(&result, pat.static_replacement.as_str())
                .to_string();
        }
        result
    }

    /// Synchronous reversible regex redaction storing original values in `vault`.
    pub fn redact_str_with_vault(&self, input: &str, vault: &mut RedactionVault) -> String {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let format = guard.config.placeholder_format;
        let mut result = input.to_string();

        for pat in &guard.patterns {
            let matches: Vec<(usize, usize)> = pat
                .regex
                .find_iter(&result)
                .map(|m| (m.start(), m.end()))
                .collect();
            if matches.is_empty() {
                continue;
            }
            let mut next = String::with_capacity(result.len());
            let mut cursor = 0usize;
            for (start, end) in matches {
                next.push_str(&result[cursor..start]);
                let matched = &result[start..end];
                if pat.preserve_bearer_prefix {
                    if let Some(ws_idx) = matched.find(char::is_whitespace) {
                        let prefix_end = matched[ws_idx..]
                            .find(|c: char| !c.is_whitespace())
                            .map(|off| ws_idx + off)
                            .unwrap_or(ws_idx);
                        let prefix = &matched[..prefix_end];
                        let token = &matched[prefix_end..];
                        let placeholder = vault.tokenize(&pat.category, token, format);
                        next.push_str(prefix);
                        next.push_str(&placeholder);
                    } else {
                        let placeholder = vault.tokenize(&pat.category, matched, format);
                        next.push_str(&placeholder);
                    }
                } else {
                    let placeholder = vault.tokenize(&pat.category, matched, format);
                    next.push_str(&placeholder);
                }
                cursor = end;
            }
            next.push_str(&result[cursor..]);
            result = next;
        }
        result
    }

    /// Async redaction combining deterministic regex rules and optional smart redaction
    /// (Microsoft Presidio sidecar or local/trusted LLM pre-pass).
    pub async fn redact_str_async(
        &self,
        input: &str,
        mut vault: Option<&mut RedactionVault>,
    ) -> Result<String, String> {
        let regex_redacted = if let Some(v) = vault.as_deref_mut() {
            self.redact_str_with_vault(input, v)
        } else {
            self.redact_str(input)
        };

        // Snapshot config and drop the RwLock read guard before any `.await`.
        let (smart, format) = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            (guard.config.smart.clone(), guard.config.placeholder_format)
        };

        if !smart.enabled || regex_redacted.trim().is_empty() {
            return Ok(regex_redacted);
        }

        let smart_result = match smart.backend {
            SmartRedactionBackend::Presidio => {
                redact_with_presidio(&self.http_client, &regex_redacted, vault, format, &smart)
                    .await
            }
            SmartRedactionBackend::Llm => {
                redact_with_llm(&self.http_client, &regex_redacted, vault, format, &smart).await
            }
        };

        match smart_result {
            Ok(redacted) => Ok(redacted),
            Err(err) => match smart.fail_mode {
                SmartRedactionFailMode::FailClosed => Err(err),
                SmartRedactionFailMode::FallbackRegex => {
                    warn!(
                        error = %err,
                        backend = ?smart.backend,
                        "Smart redaction failed; falling back to regex redaction"
                    );
                    Ok(regex_redacted)
                }
            },
        }
    }

    async fn redact_message_async(
        &self,
        msg: &mut ChatMessage,
        mut vault: Option<&mut RedactionVault>,
    ) -> Result<(), String> {
        msg.content = self
            .redact_str_async(&msg.content, vault.as_deref_mut())
            .await?;
        if let Some(parts) = &mut msg.content_parts {
            for part in parts {
                if let ContentPart::Text { text } = part {
                    *text = self.redact_str_async(text, vault.as_deref_mut()).await?;
                }
            }
        }
        if let Some(calls) = &mut msg.tool_calls {
            for call in calls {
                call.arguments = self
                    .redact_str_async(&call.arguments, vault.as_deref_mut())
                    .await?;
            }
        }
        Ok(())
    }

    fn redact_message_sync(&self, msg: &mut ChatMessage) {
        msg.content = self.redact_str(&msg.content);
        if let Some(parts) = &mut msg.content_parts {
            for part in parts {
                if let ContentPart::Text { text } = part {
                    *text = self.redact_str(text);
                }
            }
        }
        if let Some(calls) = &mut msg.tool_calls {
            for call in calls {
                call.arguments = self.redact_str(&call.arguments);
            }
        }
    }

    fn redact_response(&self, resp: &mut ChatResponse) {
        for choice in &mut resp.choices {
            self.redact_message_sync(&mut choice.message);
        }
    }
}

#[async_trait]
impl GatewayPlugin for RedactionPlugin {
    fn name(&self) -> &str {
        "redaction"
    }

    async fn pre_request(&self, ctx: &mut PluginContext, req: &mut ChatRequest) -> PluginAction {
        if ctx
            .metadata
            .get(SKIP_REDACTION_METADATA_KEY)
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            return PluginAction::Continue;
        }

        let (redact_requests, reversible) = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            (guard.config.redact_requests, guard.config.reversible)
        };
        if !redact_requests {
            return PluginAction::Continue;
        }

        let mut vault = if reversible {
            Some(RedactionVault::new())
        } else {
            None
        };

        for msg in &mut req.messages {
            if let Err(err) = self.redact_message_async(msg, vault.as_mut()).await {
                return PluginAction::ShortCircuit {
                    status_code: 503,
                    message: format!("Redaction service unavailable: {}", err),
                };
            }
        }

        if let Some(v) = vault {
            if !v.is_empty() {
                if let Ok(serialized) = serde_json::to_value(&v) {
                    ctx.metadata
                        .insert(REDACTION_VAULT_METADATA_KEY.to_string(), serialized);
                }
            }
        }

        PluginAction::Continue
    }

    async fn post_response(&self, ctx: &mut PluginContext, resp: &mut ChatResponse) {
        if ctx
            .metadata
            .get(SKIP_REDACTION_METADATA_KEY)
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            return;
        }

        let (redact_responses, reversible) = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            (guard.config.redact_responses, guard.config.reversible)
        };

        if reversible {
            if let Some(raw_vault) = ctx.metadata.remove(REDACTION_VAULT_METADATA_KEY) {
                if let Ok(vault) = serde_json::from_value::<RedactionVault>(raw_vault) {
                    for choice in &mut resp.choices {
                        vault.restore_message(&mut choice.message);
                    }
                }
            }
        } else if redact_responses {
            self.redact_response(resp);
        }
    }
}

#[cfg(test)]
include!("redaction_tests.rs");
