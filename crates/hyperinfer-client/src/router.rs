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
