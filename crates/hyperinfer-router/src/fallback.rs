use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorKind {
    RateLimit,
    ServerError,
    Timeout,
    ContentPolicy,
    ContextWindow,
    AuthError,
    Other,
}

impl ErrorKind {
    pub fn from_status(status: u16, body: &str) -> Self {
        match status {
            429 => Self::RateLimit,
            401 | 403 => Self::AuthError,
            400 => {
                let lower = body.to_lowercase();
                if lower.contains("content_policy") || lower.contains("content_filter") {
                    Self::ContentPolicy
                } else if lower.contains("context_length")
                    || lower.contains("context_window")
                    || lower.contains("maximum context")
                {
                    Self::ContextWindow
                } else {
                    Self::Other
                }
            }
            500 | 502 | 503 | 504 => Self::ServerError,
            _ => Self::Other,
        }
    }

    pub fn is_timeout(err: &hyperinfer_core::HyperInferError) -> bool {
        match err {
            hyperinfer_core::HyperInferError::Http(e) => e.is_timeout(),
            _ => false,
        }
    }

    pub fn classify(err: &hyperinfer_core::HyperInferError) -> Self {
        match err {
            hyperinfer_core::HyperInferError::ApiError { status, message } => {
                Self::from_status(*status, message)
            }
            hyperinfer_core::HyperInferError::Http(e) if e.is_timeout() => Self::Timeout,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    pub fallbacks: HashMap<String, Vec<String>>,
    pub default_fallbacks: Vec<String>,
    pub content_policy_fallbacks: HashMap<String, Vec<String>>,
    pub context_window_fallbacks: HashMap<String, Vec<String>>,
    pub max_fallbacks: usize,
    pub num_retries: u32,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl FallbackConfig {
    pub fn new() -> Self {
        Self {
            fallbacks: HashMap::new(),
            default_fallbacks: Vec::new(),
            content_policy_fallbacks: HashMap::new(),
            context_window_fallbacks: HashMap::new(),
            max_fallbacks: 5,
            num_retries: 3,
        }
    }

    pub fn with_fallback(mut self, model: impl Into<String>, targets: Vec<String>) -> Self {
        self.fallbacks.insert(model.into(), targets);
        self
    }

    pub fn with_default_fallbacks(mut self, targets: Vec<String>) -> Self {
        self.default_fallbacks = targets;
        self
    }

    pub fn with_content_policy_fallback(
        mut self,
        model: impl Into<String>,
        targets: Vec<String>,
    ) -> Self {
        self.content_policy_fallbacks.insert(model.into(), targets);
        self
    }

    pub fn with_context_window_fallback(
        mut self,
        model: impl Into<String>,
        targets: Vec<String>,
    ) -> Self {
        self.context_window_fallbacks.insert(model.into(), targets);
        self
    }

    pub fn get_fallbacks(&self, model: &str, error_kind: &ErrorKind) -> Vec<String> {
        let map = match error_kind {
            ErrorKind::ContentPolicy => Some(&self.content_policy_fallbacks),
            ErrorKind::ContextWindow => Some(&self.context_window_fallbacks),
            _ => Some(&self.fallbacks),
        };

        if let Some(map) = map {
            if let Some(targets) = map.get(model) {
                return targets.clone();
            }
        }

        self.default_fallbacks.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_429() {
        let err = hyperinfer_core::HyperInferError::ApiError {
            status: 429,
            message: "rate limited".into(),
        };
        assert_eq!(ErrorKind::classify(&err), ErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_500() {
        let err = hyperinfer_core::HyperInferError::ApiError {
            status: 500,
            message: "internal error".into(),
        };
        assert_eq!(ErrorKind::classify(&err), ErrorKind::ServerError);
    }

    #[test]
    fn test_classify_502() {
        let err = hyperinfer_core::HyperInferError::ApiError {
            status: 502,
            message: "bad gateway".into(),
        };
        assert_eq!(ErrorKind::classify(&err), ErrorKind::ServerError);
    }

    #[test]
    fn test_classify_401() {
        let err = hyperinfer_core::HyperInferError::ApiError {
            status: 401,
            message: "unauthorized".into(),
        };
        assert_eq!(ErrorKind::classify(&err), ErrorKind::AuthError);
    }

    #[test]
    fn test_classify_content_policy() {
        let err = hyperinfer_core::HyperInferError::ApiError {
            status: 400,
            message: "violated content_policy rules".into(),
        };
        assert_eq!(ErrorKind::classify(&err), ErrorKind::ContentPolicy);
    }

    #[test]
    fn test_classify_context_window() {
        let err = hyperinfer_core::HyperInferError::ApiError {
            status: 400,
            message: "exceeds context_length limit".into(),
        };
        assert_eq!(ErrorKind::classify(&err), ErrorKind::ContextWindow);
    }

    #[test]
    fn test_classify_unknown_400() {
        let err = hyperinfer_core::HyperInferError::ApiError {
            status: 400,
            message: "bad request".into(),
        };
        assert_eq!(ErrorKind::classify(&err), ErrorKind::Other);
    }

    #[test]
    fn test_fallback_lookup_specific() {
        let config = FallbackConfig::new()
            .with_fallback("gpt-4", vec!["claude-3".into(), "gemini-pro".into()]);
        let result = config.get_fallbacks("gpt-4", &ErrorKind::ServerError);
        assert_eq!(result, vec!["claude-3", "gemini-pro"]);
    }

    #[test]
    fn test_fallback_lookup_default() {
        let config = FallbackConfig::new().with_default_fallbacks(vec!["default-model".into()]);
        let result = config.get_fallbacks("unknown-model", &ErrorKind::ServerError);
        assert_eq!(result, vec!["default-model"]);
    }

    #[test]
    fn test_fallback_content_policy_specific() {
        let config = FallbackConfig::new()
            .with_content_policy_fallback("gpt-4", vec!["claude-3-opus".into()]);
        let result = config.get_fallbacks("gpt-4", &ErrorKind::ContentPolicy);
        assert_eq!(result, vec!["claude-3-opus"]);
    }

    #[test]
    fn test_fallback_context_window_specific() {
        let config = FallbackConfig::new()
            .with_context_window_fallback("gpt-4", vec!["gemini-pro-1m".into()]);
        let result = config.get_fallbacks("gpt-4", &ErrorKind::ContextWindow);
        assert_eq!(result, vec!["gemini-pro-1m"]);
    }

    #[test]
    fn test_fallback_no_match_returns_empty() {
        let config = FallbackConfig::new();
        let result = config.get_fallbacks("gpt-4", &ErrorKind::ServerError);
        assert!(result.is_empty());
    }
}
