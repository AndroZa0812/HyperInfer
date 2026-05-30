use hyperinfer_core::Provider;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: String,
    pub model_name: String,
    pub provider: Provider,
    pub model: String,
    pub api_key_ref: String,
    pub base_url: Option<String>,
    pub weight: u32,
    pub rpm_limit: Option<u64>,
    pub tpm_limit: Option<u64>,
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
    pub order: u32,
    pub tags: HashMap<String, String>,
}

impl Deployment {
    pub fn new(model_name: String, provider: Provider, model: String, api_key_ref: String) -> Self {
        let id = Self::generate_id(&provider, &model, &None, &api_key_ref);
        Self {
            id,
            model_name,
            provider,
            model,
            api_key_ref,
            base_url: None,
            weight: 1,
            rpm_limit: None,
            tpm_limit: None,
            input_cost_per_1k: None,
            output_cost_per_1k: None,
            order: 0,
            tags: HashMap::new(),
        }
    }

    pub fn generate_id(
        provider: &Provider,
        model: &str,
        base_url: &Option<String>,
        api_key_ref: &str,
    ) -> String {
        let base_url_str = base_url.as_deref().unwrap_or("");
        let input = format!("{}:{}:{}:{}", provider, model, base_url_str, api_key_ref);
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let result = hasher.finalize();
        hex::encode(&result[..8])
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.id = Self::generate_id(
            &self.provider,
            &self.model,
            &Some(base_url.clone()),
            &self.api_key_ref,
        );
        self.base_url = Some(base_url);
        self
    }

    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_rpm_limit(mut self, rpm_limit: u64) -> Self {
        self.rpm_limit = Some(rpm_limit);
        self
    }

    pub fn with_tpm_limit(mut self, tpm_limit: u64) -> Self {
        self.tpm_limit = Some(tpm_limit);
        self
    }

    pub fn with_input_cost(mut self, cost: f64) -> Self {
        self.input_cost_per_1k = Some(cost);
        self
    }

    pub fn with_output_cost(mut self, cost: f64) -> Self {
        self.output_cost_per_1k = Some(cost);
        self
    }

    pub fn with_order(mut self, order: u32) -> Self {
        self.order = order;
        self
    }

    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }
}

#[derive(Debug, Clone)]
pub struct DeploymentPool {
    deployments: HashMap<String, Vec<Arc<Deployment>>>,
}

impl DeploymentPool {
    pub fn new() -> Self {
        Self {
            deployments: HashMap::new(),
        }
    }

    pub fn add(&mut self, deployment: Deployment) {
        let model_name = deployment.model_name.clone();
        let entry = self.deployments.entry(model_name).or_default();
        entry.push(Arc::new(deployment));
        entry.sort_by_key(|d| d.order);
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let mut found = false;
        let mut empty_keys = Vec::new();

        for (key, deployments) in self.deployments.iter_mut() {
            let initial_len = deployments.len();
            deployments.retain(|d| d.id != id);
            if deployments.len() < initial_len {
                found = true;
            }
            if deployments.is_empty() {
                empty_keys.push(key.clone());
            }
        }

        for key in empty_keys {
            self.deployments.remove(&key);
        }

        found
    }

    pub fn get(&self, model_name: &str) -> Option<&[Arc<Deployment>]> {
        self.deployments.get(model_name).map(|v| v.as_slice())
    }

    pub fn model_names(&self) -> Vec<String> {
        self.deployments.keys().cloned().collect()
    }

    pub fn rebuild(&mut self) {
        for deployments in self.deployments.values_mut() {
            deployments.sort_by_key(|d| d.order);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.deployments.is_empty()
    }

    pub fn total_deployments(&self) -> usize {
        self.deployments.values().map(|v| v.len()).sum()
    }
}

impl Default for DeploymentPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_id_determinism() {
        let d1 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        );
        let d2 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        );
        assert_eq!(d1.id, d2.id);
    }

    #[test]
    fn test_deployment_id_differs_by_api_key() {
        let d1 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        );
        let d2 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key2".to_string(),
        );
        assert_ne!(d1.id, d2.id);
    }

    #[test]
    fn test_deployment_id_differs_by_base_url() {
        let d1 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        );
        let d2 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        )
        .with_base_url("https://custom.api.com".to_string());
        assert_ne!(d1.id, d2.id);
    }

    #[test]
    fn test_deployment_id_differs_by_provider() {
        let d1 = Deployment::new(
            "model".to_string(),
            Provider::OpenAI,
            "model".to_string(),
            "key1".to_string(),
        );
        let d2 = Deployment::new(
            "model".to_string(),
            Provider::Anthropic,
            "model".to_string(),
            "key1".to_string(),
        );
        assert_ne!(d1.id, d2.id);
    }

    #[test]
    fn test_deployment_builder_defaults() {
        let d = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        );
        assert_eq!(d.weight, 1);
        assert_eq!(d.order, 0);
        assert!(d.base_url.is_none());
        assert!(d.rpm_limit.is_none());
        assert!(d.tpm_limit.is_none());
        assert!(d.input_cost_per_1k.is_none());
        assert!(d.output_cost_per_1k.is_none());
        assert!(d.tags.is_empty());
    }

    #[test]
    fn test_deployment_builder_chain() {
        let d = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        )
        .with_base_url("https://api.openai.com".to_string())
        .with_weight(5)
        .with_rpm_limit(1000)
        .with_tpm_limit(50000)
        .with_input_cost(0.03)
        .with_output_cost(0.06)
        .with_order(10)
        .with_tag("env".to_string(), "prod".to_string())
        .with_tag("region".to_string(), "us-east".to_string());

        assert_eq!(d.base_url, Some("https://api.openai.com".to_string()));
        assert_eq!(d.weight, 5);
        assert_eq!(d.rpm_limit, Some(1000));
        assert_eq!(d.tpm_limit, Some(50000));
        assert_eq!(d.input_cost_per_1k, Some(0.03));
        assert_eq!(d.output_cost_per_1k, Some(0.06));
        assert_eq!(d.order, 10);
        assert_eq!(d.tags.len(), 2);
        assert_eq!(d.tags.get("env"), Some(&"prod".to_string()));
        assert_eq!(d.tags.get("region"), Some(&"us-east".to_string()));
    }

    #[test]
    fn test_deployment_serialization_roundtrip() {
        let d = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        )
        .with_weight(3)
        .with_rpm_limit(500)
        .with_tag("env".to_string(), "test".to_string());

        let json = serde_json::to_string(&d).unwrap();
        let deserialized: Deployment = serde_json::from_str(&json).unwrap();

        assert_eq!(d.id, deserialized.id);
        assert_eq!(d.model_name, deserialized.model_name);
        assert_eq!(d.provider, deserialized.provider);
        assert_eq!(d.model, deserialized.model);
        assert_eq!(d.weight, deserialized.weight);
        assert_eq!(d.rpm_limit, deserialized.rpm_limit);
        assert_eq!(d.tags, deserialized.tags);
    }

    #[test]
    fn test_pool_grouping_by_model_name() {
        let mut pool = DeploymentPool::new();

        pool.add(Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        ));
        pool.add(Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key2".to_string(),
        ));
        pool.add(Deployment::new(
            "claude-3".to_string(),
            Provider::Anthropic,
            "claude-3-opus".to_string(),
            "key3".to_string(),
        ));

        let gpt4_deployments = pool.get("gpt-4").unwrap();
        assert_eq!(gpt4_deployments.len(), 2);

        let claude_deployments = pool.get("claude-3").unwrap();
        assert_eq!(claude_deployments.len(), 1);

        assert_eq!(pool.total_deployments(), 3);
    }

    #[test]
    fn test_pool_ordering_by_order_field() {
        let mut pool = DeploymentPool::new();

        pool.add(
            Deployment::new(
                "gpt-4".to_string(),
                Provider::OpenAI,
                "gpt-4".to_string(),
                "key1".to_string(),
            )
            .with_order(3),
        );
        pool.add(
            Deployment::new(
                "gpt-4".to_string(),
                Provider::OpenAI,
                "gpt-4".to_string(),
                "key2".to_string(),
            )
            .with_order(1),
        );
        pool.add(
            Deployment::new(
                "gpt-4".to_string(),
                Provider::OpenAI,
                "gpt-4".to_string(),
                "key3".to_string(),
            )
            .with_order(2),
        );

        let deployments = pool.get("gpt-4").unwrap();
        assert_eq!(deployments[0].order, 1);
        assert_eq!(deployments[1].order, 2);
        assert_eq!(deployments[2].order, 3);
    }

    #[test]
    fn test_pool_remove() {
        let mut pool = DeploymentPool::new();

        let d1 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        );
        let d2 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key2".to_string(),
        );
        let d1_id = d1.id.clone();

        pool.add(d1);
        pool.add(d2);
        assert_eq!(pool.total_deployments(), 2);

        let removed = pool.remove(&d1_id);
        assert!(removed);
        assert_eq!(pool.total_deployments(), 1);

        let removed_again = pool.remove(&d1_id);
        assert!(!removed_again);
    }

    #[test]
    fn test_pool_model_names() {
        let mut pool = DeploymentPool::new();

        pool.add(Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        ));
        pool.add(Deployment::new(
            "claude-3".to_string(),
            Provider::Anthropic,
            "claude-3-opus".to_string(),
            "key2".to_string(),
        ));
        pool.add(Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key3".to_string(),
        ));

        let mut names = pool.model_names();
        names.sort();
        assert_eq!(names, vec!["claude-3".to_string(), "gpt-4".to_string()]);
    }

    #[test]
    fn test_pool_rebuild() {
        let mut pool = DeploymentPool::new();

        let d1 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key1".to_string(),
        )
        .with_order(1);
        let d2 = Deployment::new(
            "gpt-4".to_string(),
            Provider::OpenAI,
            "gpt-4".to_string(),
            "key2".to_string(),
        )
        .with_order(2);

        pool.add(d2);
        pool.add(d1);

        let deployments = pool.get("gpt-4").unwrap();
        assert_eq!(deployments[0].order, 1);
        assert_eq!(deployments[1].order, 2);

        pool.rebuild();

        let deployments = pool.get("gpt-4").unwrap();
        assert_eq!(deployments[0].order, 1);
        assert_eq!(deployments[1].order, 2);
    }
}
