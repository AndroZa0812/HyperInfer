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
    /// The provider name is owned by the registry via Arc<str>, so no memory
    /// leaking is required. Returns an error if a provider with the same name
    /// is already registered.
    pub fn register_provider(
        &self,
        name: String,
        chat_callable: Py<PyAny>,
        stream_callable: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let name_arc: Arc<str> = name.clone().into();
        if self.registry.contains(&name_arc) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Provider '{}' is already registered",
                name
            )));
        }
        let provider = PythonProvider::new(name_arc.clone(), chat_callable, stream_callable);
        self.registry.register_arc(name_arc, Arc::new(provider));
        Ok(())
    }

    /// Unregister a provider by name.
    ///
    /// Removes the provider from the registry. The Arc<str> key will be
    /// dropped when the last reference is released.
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
