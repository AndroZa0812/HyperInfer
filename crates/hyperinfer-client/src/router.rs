use hyperinfer_core::types::{Config, Provider};
use tracing::warn;

pub struct Router {
    #[allow(dead_code)]
    rules: Vec<hyperinfer_core::types::RoutingRule>,
    model_aliases: std::collections::HashMap<String, (String, Option<Provider>)>,
    default_provider: Option<Provider>,
}

impl Router {
    pub fn new(rules: Vec<hyperinfer_core::types::RoutingRule>) -> Self {
        Self {
            rules,
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        }
    }

    pub fn with_aliases(mut self, aliases: std::collections::HashMap<String, String>) -> Self {
        self.model_aliases = aliases
            .into_iter()
            .filter_map(|(alias, target)| match Self::parse_target_model(&target) {
                Ok((model, provider)) => Some((alias, (model, provider))),
                Err(err) => {
                    warn!("Invalid alias '{}': {}", alias, err);
                    None
                }
            })
            .collect();
        self
    }

    pub fn with_default_provider(mut self, provider: Option<Provider>) -> Self {
        self.default_provider = provider;
        self
    }

    fn parse_target_model(target: &str) -> Result<(String, Option<Provider>), String> {
        if let Some(slash_pos) = target.find('/') {
            let provider_str = &target[..slash_pos];
            let model = target[slash_pos + 1..].to_string();
            let provider = match provider_str.to_lowercase().as_str() {
                "openai" => Some(Provider::OpenAI),
                "anthropic" => Some(Provider::Anthropic),
                unknown => return Err(format!("Unknown provider: '{}'", unknown)),
            };
            Ok((model, provider))
        } else {
            Ok((target.to_string(), None))
        }
    }

    fn infer_provider(model: &str) -> Option<Provider> {
        if model.starts_with("gpt-") || model.starts_with("o1-") || model.starts_with("o3-") {
            Some(Provider::OpenAI)
        } else if model.starts_with("claude-") {
            Some(Provider::Anthropic)
        } else {
            None
        }
    }

    fn resolve_provider(&self, explicit: Option<Provider>, model: &str) -> Option<Provider> {
        if let Some(provider) = explicit {
            return Some(provider);
        }
        Self::infer_provider(model).or(self.default_provider.clone())
    }

    pub fn resolve(&self, model: &str, _config: &Config) -> Option<(String, Provider)> {
        if let Some((target_model, explicit_provider)) = self.model_aliases.get(model) {
            let provider = self.resolve_provider(explicit_provider.clone(), target_model)?;
            return Some((target_model.clone(), provider));
        }

        let provider = self.resolve_provider(None, model)?;
        Some((model.to_string(), provider))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_config() -> Config {
        Config {
            api_keys: HashMap::new(),
            routing_rules: vec![],
            quotas: HashMap::new(),
            model_aliases: HashMap::new(),
            default_provider: None,
        }
    }

    #[test]
    fn test_router_new() {
        let router = Router::new(vec![]);
        assert_eq!(router.model_aliases.len(), 0);
        assert_eq!(router.default_provider, None);
    }

    #[test]
    fn test_router_with_default_provider() {
        let router = Router::new(vec![]).with_default_provider(Some(Provider::OpenAI));
        assert_eq!(router.default_provider, Some(Provider::OpenAI));
    }

    #[test]
    fn test_parse_target_model_with_provider() {
        let result = Router::parse_target_model("openai/gpt-4").unwrap();
        assert_eq!(result.0, "gpt-4");
        assert_eq!(result.1, Some(Provider::OpenAI));

        let result = Router::parse_target_model("anthropic/claude-3").unwrap();
        assert_eq!(result.0, "claude-3");
        assert_eq!(result.1, Some(Provider::Anthropic));
    }

    #[test]
    fn test_parse_target_model_without_provider() {
        let result = Router::parse_target_model("gpt-4").unwrap();
        assert_eq!(result.0, "gpt-4");
        assert_eq!(result.1, None);
    }

    #[test]
    fn test_parse_target_model_unknown_provider() {
        let result = Router::parse_target_model("unknown/model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown provider"));
    }

    #[test]
    fn test_infer_provider_gpt() {
        assert_eq!(Router::infer_provider("gpt-4"), Some(Provider::OpenAI));
        assert_eq!(
            Router::infer_provider("gpt-3.5-turbo"),
            Some(Provider::OpenAI)
        );
    }

    #[test]
    fn test_infer_provider_o1() {
        assert_eq!(Router::infer_provider("o1-preview"), Some(Provider::OpenAI));
        assert_eq!(Router::infer_provider("o1-mini"), Some(Provider::OpenAI));
    }

    #[test]
    fn test_infer_provider_o3() {
        assert_eq!(Router::infer_provider("o3-mini"), Some(Provider::OpenAI));
    }

    #[test]
    fn test_infer_provider_claude() {
        assert_eq!(
            Router::infer_provider("claude-3-opus"),
            Some(Provider::Anthropic)
        );
        assert_eq!(
            Router::infer_provider("claude-2"),
            Some(Provider::Anthropic)
        );
    }

    #[test]
    fn test_infer_provider_unknown() {
        assert_eq!(Router::infer_provider("unknown-model"), None);
        assert_eq!(Router::infer_provider("llama-2"), None);
    }

    #[test]
    fn test_with_aliases_valid() {
        let mut aliases = HashMap::new();
        aliases.insert("my-gpt".to_string(), "openai/gpt-4".to_string());
        aliases.insert("my-claude".to_string(), "anthropic/claude-3".to_string());

        let router = Router::new(vec![]).with_aliases(aliases);
        assert_eq!(router.model_aliases.len(), 2);
    }

    #[test]
    fn test_with_aliases_invalid_skipped() {
        let mut aliases = HashMap::new();
        aliases.insert("valid".to_string(), "openai/gpt-4".to_string());
        aliases.insert("invalid".to_string(), "unknown/model".to_string());

        let router = Router::new(vec![]).with_aliases(aliases);
        assert_eq!(router.model_aliases.len(), 1);
        assert!(router.model_aliases.contains_key("valid"));
        assert!(!router.model_aliases.contains_key("invalid"));
    }

    #[test]
    fn test_resolve_with_alias() {
        let mut aliases = HashMap::new();
        aliases.insert("my-model".to_string(), "openai/gpt-4".to_string());

        let router = Router::new(vec![]).with_aliases(aliases);
        let config = create_test_config();

        let result = router.resolve("my-model", &config);
        assert!(result.is_some());
        let (model, provider) = result.unwrap();
        assert_eq!(model, "gpt-4");
        assert_eq!(provider, Provider::OpenAI);
    }

    #[test]
    fn test_resolve_with_inference() {
        let router = Router::new(vec![]);
        let config = create_test_config();

        let result = router.resolve("gpt-4", &config);
        assert!(result.is_some());
        let (model, provider) = result.unwrap();
        assert_eq!(model, "gpt-4");
        assert_eq!(provider, Provider::OpenAI);

        let result = router.resolve("claude-3", &config);
        assert!(result.is_some());
        let (model, provider) = result.unwrap();
        assert_eq!(model, "claude-3");
        assert_eq!(provider, Provider::Anthropic);
    }

    #[test]
    fn test_resolve_with_default_provider() {
        let router = Router::new(vec![]).with_default_provider(Some(Provider::OpenAI));
        let config = create_test_config();

        let result = router.resolve("unknown-model", &config);
        assert!(result.is_some());
        let (model, provider) = result.unwrap();
        assert_eq!(model, "unknown-model");
        assert_eq!(provider, Provider::OpenAI);
    }

    #[test]
    fn test_resolve_no_match() {
        let router = Router::new(vec![]);
        let config = create_test_config();

        let result = router.resolve("unknown-model", &config);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_alias_without_explicit_provider() {
        let mut aliases = HashMap::new();
        aliases.insert("my-gpt".to_string(), "gpt-4".to_string());

        let router = Router::new(vec![]).with_aliases(aliases);
        let config = create_test_config();

        let result = router.resolve("my-gpt", &config);
        assert!(result.is_some());
        let (model, provider) = result.unwrap();
        assert_eq!(model, "gpt-4");
        assert_eq!(provider, Provider::OpenAI);
    }

    #[test]
    fn test_resolve_alias_with_default_provider() {
        let mut aliases = HashMap::new();
        aliases.insert("my-model".to_string(), "custom-model".to_string());

        let router = Router::new(vec![])
            .with_aliases(aliases)
            .with_default_provider(Some(Provider::Anthropic));
        let config = create_test_config();

        let result = router.resolve("my-model", &config);
        assert!(result.is_some());
        let (model, provider) = result.unwrap();
        assert_eq!(model, "custom-model");
        assert_eq!(provider, Provider::Anthropic);
    }

    #[test]
    fn test_resolve_priority_explicit_over_inference() {
        let mut aliases = HashMap::new();
        // Map a gpt-like name to anthropic explicitly
        aliases.insert("gpt-custom".to_string(), "anthropic/claude-3".to_string());

        let router = Router::new(vec![]).with_aliases(aliases);
        let config = create_test_config();

        let result = router.resolve("gpt-custom", &config);
        assert!(result.is_some());
        let (model, provider) = result.unwrap();
        assert_eq!(model, "claude-3");
        assert_eq!(provider, Provider::Anthropic);
    }
}
