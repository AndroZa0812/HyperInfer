use super::providers::PythonProvider;
use hyperinfer_providers::ProviderRegistry;
use pyo3::prelude::*;
use std::sync::Arc;

#[pyclass]
pub struct ProviderRegistryWrapper {
    registry: Arc<ProviderRegistry>,
}

#[pymethods]
impl ProviderRegistryWrapper {
    #[new]
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ProviderRegistry::new()),
        }
    }

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
