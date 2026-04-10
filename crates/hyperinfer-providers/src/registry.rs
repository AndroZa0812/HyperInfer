use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

#[allow(clippy::type_complexity)]
pub struct ProviderRegistry {
    providers: Arc<RwLock<HashMap<Arc<str>, Arc<dyn crate::provider_trait::LlmProvider>>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register<P: crate::provider_trait::LlmProvider + 'static>(&self, provider: P) {
        let name = Arc::from(provider.name());
        let mut providers = self.providers.write().unwrap();
        if providers.contains_key(&name) {
            panic!("Provider '{}' is already registered", name);
        }
        providers.insert(name, Arc::new(provider));
    }

    pub fn register_arc_if_absent(
        &self,
        name: Arc<str>,
        provider: Arc<dyn crate::provider_trait::LlmProvider>,
    ) -> Result<(), Arc<str>> {
        let mut providers = self.providers.write().unwrap();
        if providers.contains_key(&name) {
            return Err(name);
        }
        providers.insert(name, provider);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn crate::provider_trait::LlmProvider>> {
        let providers = self.providers.read().unwrap();
        providers.get(name).cloned()
    }

    pub fn list(&self) -> Vec<Arc<str>> {
        let providers = self.providers.read().unwrap();
        providers.keys().cloned().collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        let providers = self.providers.read().unwrap();
        providers.contains_key(name)
    }

    pub fn unregister(&self, name: &str) -> Option<Arc<dyn crate::provider_trait::LlmProvider>> {
        let mut providers = self.providers.write().unwrap();
        providers.remove(name)
    }

    /// Get a StreamingProvider for the named provider, suitable for calling into_stream().
    /// Returns None if the provider is not registered or does not support streaming.
    /// Clones the Arc (O(1)) rather than deep-cloning the provider internals.
    pub fn get_streaming(&self, name: &str) -> Option<crate::provider_trait::StreamingProvider> {
        let providers = self.providers.read().unwrap();
        let provider = providers.get(name)?;
        if !provider.supports_streaming() {
            return None;
        }
        Some(crate::provider_trait::StreamingProvider::new(Arc::clone(
            provider,
        )))
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Cloning produces a shared, reference-counted view of the registry.
/// Both the original and the clone share the same underlying provider map;
/// no providers are deep-copied. This is the same semantics as `Arc::clone()`.
impl Clone for ProviderRegistry {
    fn clone(&self) -> Self {
        Self {
            providers: self.providers.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_trait::LlmProvider;
    use async_trait::async_trait;
    use futures::Stream;
    use hyperinfer_core::{ChatChunk, ChatRequest, ChatResponse, HyperInferError};
    use std::pin::Pin;

    struct MockProvider {
        name: &'static str,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            self.name
        }

        async fn chat(
            &self,
            _request: &ChatRequest,
            _api_key: &str,
        ) -> Result<ChatResponse, HyperInferError> {
            Ok(ChatResponse::default())
        }

        fn stream(
            &self,
            _request: &ChatRequest,
            _api_key: &str,
        ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + '_>> {
            Box::pin(futures::stream::empty())
        }
    }

    #[test]
    fn test_registry_new() {
        let registry = ProviderRegistry::new();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_registry_default() {
        let registry = ProviderRegistry::default();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_registry_register() {
        let registry = ProviderRegistry::new();
        let provider = MockProvider {
            name: "test-provider",
        };
        registry.register(provider);
        assert!(registry.contains("test-provider"));
    }

    #[test]
    fn test_registry_get() {
        let registry = ProviderRegistry::new();
        let provider = MockProvider {
            name: "test-provider",
        };
        registry.register(provider);

        let retrieved = registry.get("test-provider");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "test-provider");

        assert!(registry.get("non-existent").is_none());
    }

    #[test]
    fn test_registry_contains() {
        let registry = ProviderRegistry::new();
        let provider = MockProvider {
            name: "test-provider",
        };
        registry.register(provider);

        assert!(registry.contains("test-provider"));
        assert!(!registry.contains("non-existent"));
    }

    #[test]
    #[should_panic(expected = "Provider 'test-provider' is already registered")]
    fn test_registry_register_duplicate() {
        let registry = ProviderRegistry::new();
        registry.register(MockProvider {
            name: "test-provider",
        });
        registry.register(MockProvider {
            name: "test-provider",
        });
    }

    #[test]
    fn test_registry_register_arc_if_absent() {
        let registry = ProviderRegistry::new();
        let name: Arc<str> = Arc::from("test-provider");
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider {
            name: "test-provider",
        });

        let result = registry.register_arc_if_absent(name.clone(), provider.clone());
        assert!(result.is_ok());
        assert!(registry.contains("test-provider"));

        let result = registry.register_arc_if_absent(name.clone(), provider);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), name);
    }

    #[test]
    fn test_registry_list() {
        let registry = ProviderRegistry::new();
        registry.register(MockProvider { name: "p1" });
        registry.register(MockProvider { name: "p2" });

        let list = registry.list();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&Arc::from("p1")));
        assert!(list.contains(&Arc::from("p2")));
    }

    #[test]
    fn test_registry_unregister() {
        let registry = ProviderRegistry::new();
        registry.register(MockProvider {
            name: "test-provider",
        });

        let removed = registry.unregister("test-provider");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name(), "test-provider");
        assert!(!registry.contains("test-provider"));

        assert!(registry.unregister("non-existent").is_none());
    }

    #[test]
    fn test_registry_clone() {
        let registry = ProviderRegistry::new();
        let clone = registry.clone();

        registry.register(MockProvider {
            name: "test-provider",
        });

        assert!(clone.contains("test-provider"));

        clone.register(MockProvider { name: "cloned-p" });
        assert!(registry.contains("cloned-p"));
    }
}
