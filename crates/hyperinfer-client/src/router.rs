use hyperinfer_core::types::{Config, Provider, RoutingRule};

pub struct Router {
    rules: Vec<RoutingRule>,
    model_aliases: std::collections::HashMap<String, String>,
}

impl Router {
    pub fn new(rules: Vec<RoutingRule>) -> Self {
        Self {
            rules,
            model_aliases: std::collections::HashMap::new(),
        }
    }

    pub fn with_aliases(mut self, aliases: std::collections::HashMap<String, String>) -> Self {
        self.model_aliases = aliases;
        self
    }

    pub fn resolve(&self, model: &str, _config: &Config) -> Option<(String, Provider)> {
        // Check model aliases first
        if let Some(alias) = self.model_aliases.get(model) {
            let provider = if alias.starts_with("gpt-") {
                Provider::OpenAI
            } else if alias.starts_with("claude-") {
                Provider::Anthropic
            } else {
                Provider::OpenAI
            };
            return Some((alias.clone(), provider));
        }

        // Default routing logic
        if model.starts_with("gpt-") {
            Some((model.to_string(), Provider::OpenAI))
        } else if model.starts_with("claude-") {
            Some((model.to_string(), Provider::Anthropic))
        } else {
            None
        }
    }
}
