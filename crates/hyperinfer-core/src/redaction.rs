//! PII and secret redaction plugin for gateway telemetry and logs.
//!
//! Scrubs API keys, bearer tokens, emails, SSNs, and custom patterns
//! from request/response content before logging or telemetry emission.

use async_trait::async_trait;
use regex::Regex;
use tracing::warn;

use crate::plugin::{GatewayPlugin, PluginAction, PluginContext};
use crate::types::{ChatRequest, ChatResponse, ContentPart};

/// Configuration for the redaction plugin.
#[derive(Debug, Clone)]
pub struct RedactionConfig {
    pub redact_api_keys: bool,
    pub redact_bearer_tokens: bool,
    pub redact_emails: bool,
    pub redact_ssns: bool,
    pub custom_patterns: Vec<(String, String)>,
    pub redact_requests: bool,
    pub redact_responses: bool,
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
        }
    }
}

/// Redaction plugin that scrubs sensitive data from requests and responses.
#[derive(Clone)]
pub struct RedactionPlugin {
    config: RedactionConfig,
    patterns: Vec<(Regex, String)>,
}

impl RedactionPlugin {
    pub fn new(config: RedactionConfig) -> Self {
        let mut patterns = Vec::new();

        if config.redact_api_keys {
            let re = Regex::new(
                r"\b(?:sk-ant-|sk-proj-|sk-|hi-local-|hi-admin-|hi-)[A-Za-z0-9_-]{12,}\b",
            )
            .expect("api key regex is valid");
            patterns.push((re, "[REDACTED_API_KEY]".to_string()));
        }

        if config.redact_bearer_tokens {
            let re = Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{8,}")
                .expect("bearer token regex is valid");
            patterns.push((re, "Bearer [REDACTED_TOKEN]".to_string()));
        }

        if config.redact_emails {
            let re = Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b")
                .expect("email regex is valid");
            patterns.push((re, "[REDACTED_EMAIL]".to_string()));
        }

        if config.redact_ssns {
            let re = Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("SSN regex is valid");
            patterns.push((re, "[REDACTED_SSN]".to_string()));
        }

        for (pattern, replacement) in &config.custom_patterns {
            match Regex::new(pattern) {
                Ok(re) => patterns.push((re, replacement.to_string())),
                Err(e) => warn!("invalid custom redaction pattern '{}': {}", pattern, e),
            }
        }

        Self { config, patterns }
    }

    pub fn with_defaults() -> Self {
        Self::new(RedactionConfig::default())
    }

    pub fn redact_str(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (re, replacement) in &self.patterns {
            result = re.replace_all(&result, replacement.as_str()).to_string();
        }
        result
    }

    fn redact_message(&self, msg: &mut crate::types::ChatMessage) {
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

    fn redact_request(&self, req: &mut ChatRequest) {
        for msg in &mut req.messages {
            self.redact_message(msg);
        }
    }

    fn redact_response(&self, resp: &mut ChatResponse) {
        for choice in &mut resp.choices {
            self.redact_message(&mut choice.message);
        }
    }
}

#[async_trait]
impl GatewayPlugin for RedactionPlugin {
    fn name(&self) -> &str {
        "redaction"
    }

    async fn pre_request(&self, _ctx: &mut PluginContext, req: &mut ChatRequest) -> PluginAction {
        if self.config.redact_requests {
            self.redact_request(req);
        }
        PluginAction::Continue
    }

    async fn post_response(&self, _ctx: &mut PluginContext, resp: &mut ChatResponse) {
        if self.config.redact_responses {
            self.redact_response(resp);
        }
    }
}

#[cfg(test)]
include!("redaction_tests.rs");
