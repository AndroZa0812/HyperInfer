use super::providers::PythonProvider;
use hyperinfer_providers::ProviderRegistry;
use pyo3::prelude::*;
use std::sync::Arc;

#[pyclass]
pub struct ProviderRegistryWrapper {
    registry: Arc<ProviderRegistry>,
}

impl ProviderRegistryWrapper {
    pub(crate) fn get_registry(&self) -> Arc<ProviderRegistry> {
        self.registry.clone()
    }
}

#[pymethods]
impl ProviderRegistryWrapper {
    #[new]
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ProviderRegistry::new()),
        }
    }

    /// Register a Python callable as a provider.
    ///
    /// Note: This function uses `Box::leak()` to convert the provider name to a
    /// `'static` lifetime for use as a registry key. This means repeated calls
    /// with the same name will leak memory. Providers should be registered once
    /// at startup and not re-registered.
    pub fn register_provider(
        &self,
        name: String,
        chat_callable: Py<PyAny>,
        stream_callable: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let name_static = Box::leak(name.into_boxed_str());
        let provider = PythonProvider::new(name_static, chat_callable, stream_callable);
        self.registry.register_arc(name_static, Arc::new(provider));
        Ok(())
    }

    /// Unregister a provider by name.
    ///
    /// Note: This does not reclaim the memory leaked by `register_provider`
    /// (the leaked string memory). It only removes the provider entry from
    /// the registry. See `register_provider` for more details.
    pub fn unregister_provider(&self, name: &str) -> bool {
        self.registry.unregister(name).is_some()
    }

    pub fn list_providers(&self) -> Vec<String> {
        self.registry.list().iter().map(|s| s.to_string()).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.registry.contains(name)
    }
}

impl Default for ProviderRegistryWrapper {
    fn default() -> Self {
        Self::new()
    }
}

#[pyfunction]
pub fn create_provider_registry() -> ProviderRegistryWrapper {
    ProviderRegistryWrapper::new()
}
