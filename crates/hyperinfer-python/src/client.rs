use hyperinfer_client::HyperInferClient as RustClient;
use hyperinfer_core::{ChatResponse, Config, HyperInferError};
use pyo3::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;

#[pyclass]
pub struct HyperInferClient {
    inner: Arc<RwLock<Option<RustClient>>>,
    redis_url: String,
}

#[pymethods]
impl HyperInferClient {
    #[new]
    pub fn new(redis_url: String) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            redis_url,
        }
    }

    pub fn init<'a>(&self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        let redis_url = self.redis_url.clone();
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let config = Config {
                api_keys: std::collections::HashMap::new(),
                routing_rules: Vec::new(),
                quotas: std::collections::HashMap::new(),
                model_aliases: std::collections::HashMap::new(),
                default_provider: None,
            };

            let client = RustClient::new(&redis_url, config)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            let mut guard = inner.write().await;
            *guard = Some(client);

            Python::try_attach(|py| Ok(py.None())).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
            })?
        })
    }

    #[pyo3(name = "chat")]
    pub fn chat<'a>(
        &self,
        py: Python<'a>,
        key: String,
        request: Py<PyAny>,
    ) -> PyResult<Bound<'a, PyAny>> {
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let client = inner.read().await;

            let client = client.as_ref().ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "Client not initialized. Call init() first.",
                )
            })?;

            let request = Python::try_attach(|py| {
                super::types::request_from_py(py, request)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
            })??;

            let response: ChatResponse =
                client
                    .chat(&key, request)
                    .await
                    .map_err(|e: HyperInferError| {
                        pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
                    })?;

            Python::try_attach(|py| super::types::response_to_py(py, response)).ok_or_else(
                || pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python"),
            )?
        })
    }
}
