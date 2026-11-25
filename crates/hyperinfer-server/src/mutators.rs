//! Deployment mutators: model virtualization and request transformation.
//!
//! Mutators are configured per-deployment and applied to every request
//! before it is forwarded to the upstream provider.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use hyperinfer_core::ChatRequest;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DeploymentMutators {
    #[serde(default, alias = "modelNameOverride")]
    pub model_name_override: Option<String>,
    #[serde(default)]
    pub max_tokens_cap: Option<u32>,
    #[serde(default)]
    pub temperature_override: Option<f64>,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default)]
    pub body_overrides: HashMap<String, serde_json::Value>,
}

impl DeploymentMutators {
    /// Parse mutators from deployment metadata.
    ///
    /// Supports both nested `metadata.mutators` and flat `metadata` fields.
    pub fn from_metadata(metadata: &serde_json::Value) -> Self {
        if let Some(nested) = metadata.get("mutators") {
            if let Ok(m) = serde_json::from_value(nested.clone()) {
                return m;
            }
        }
        if let Ok(m) = serde_json::from_value(metadata.clone()) {
            return m;
        }
        Default::default()
    }

    /// Apply mutators to a chat request in-place.
    pub fn apply_to_request(&self, req: &mut ChatRequest) {
        if let Some(ref m) = self.model_name_override {
            if !m.is_empty() {
                req.model = m.clone();
            }
        }
        if let Some(cap) = self.max_tokens_cap {
            req.max_tokens = Some(req.max_tokens.map_or(cap, |cur| cur.min(cap)));
        }
        if let Some(t) = self.temperature_override {
            req.temperature = Some(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_metadata_nested() {
        let meta = serde_json::json!({
            "mutators": {
                "model_name_override": "gpt-4o",
                "max_tokens_cap": 1000
            }
        });
        let m = DeploymentMutators::from_metadata(&meta);
        assert_eq!(m.model_name_override, Some("gpt-4o".to_string()));
        assert_eq!(m.max_tokens_cap, Some(1000));
    }

    #[test]
    fn from_metadata_flat_alias() {
        let meta = serde_json::json!({
            "modelNameOverride": "claude-sonnet-4-20250514"
        });
        let m = DeploymentMutators::from_metadata(&meta);
        assert_eq!(
            m.model_name_override,
            Some("claude-sonnet-4-20250514".to_string())
        );
    }

    #[test]
    fn apply_model_override() {
        let mut req = ChatRequest {
            model: "gpt-4".to_string(),
            ..Default::default()
        };
        let m = DeploymentMutators {
            model_name_override: Some("gpt-4o".to_string()),
            ..Default::default()
        };
        m.apply_to_request(&mut req);
        assert_eq!(req.model, "gpt-4o");
    }

    #[test]
    fn apply_max_tokens_cap() {
        let mut req = ChatRequest {
            max_tokens: Some(2000),
            ..Default::default()
        };
        let m = DeploymentMutators {
            max_tokens_cap: Some(1000),
            ..Default::default()
        };
        m.apply_to_request(&mut req);
        assert_eq!(req.max_tokens, Some(1000));
    }

    #[test]
    fn apply_temperature_override() {
        let mut req = ChatRequest {
            temperature: Some(0.5),
            ..Default::default()
        };
        let m = DeploymentMutators {
            temperature_override: Some(0.0),
            ..Default::default()
        };
        m.apply_to_request(&mut req);
        assert_eq!(req.temperature, Some(0.0));
    }
}
