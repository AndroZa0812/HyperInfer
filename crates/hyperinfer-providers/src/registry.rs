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

    /// Get a StreamingProvider for the named provider, suitable for calling stream().
    /// Returns None if the provider is not registered.
    pub fn get_streaming(&self, name: &str) -> Option<crate::provider_trait::StreamingProvider> {
        let providers = self.providers.read().unwrap();
        providers.get(name).map(|p| {
            let inner: &dyn crate::provider_trait::LlmProvider = &**p;
            let boxed: Box<dyn crate::provider_trait::LlmProvider + Send + 'static> =
                dyn_clone::clone_box(inner);
            crate::provider_trait::StreamingProvider::new(boxed)
        })
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
