use hyperinfer_client::HyperInferClient as RustClient;
use hyperinfer_core::types::Quota;
use hyperinfer_core::{ChatResponse, Config, HyperInferError, RoutingRule};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Convert a Python config `dict` (as returned by `Config.to_dict()`) into
/// the Rust `hyperinfer_core::Config`.
///
/// Expected dict shape:
/// ```python
/// {
///     "api_keys": {"openai": "sk-...", "anthropic": "sk-ant-..."},
///     "routing_rules": [{"name": "...", "priority": 1, "fallback_models": [...]}],
///     "quotas": {"my-key": {"max_requests_per_minute": 60, ...}},
///     "model_aliases": {"my-gpt": "openai/gpt-4"},
///     "default_provider": "openai",   # or null
/// }
/// ```
fn config_from_py(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Config> {
    let dict = obj.downcast::<PyDict>()?;

    // --- api_keys ---
    let api_keys: HashMap<String, String> = if let Some(val) = dict.get_item("api_keys")? {
        val.extract()?
    } else {
        HashMap::new()
    };

    // --- routing_rules ---
    let mut routing_rules: Vec<RoutingRule> = Vec::new();
    if let Some(val) = dict.get_item("routing_rules")? {
        let list = val.downcast::<PyList>()?;
        for item in list.iter() {
            let rule_dict = item.downcast::<PyDict>()?;
            let name: String = rule_dict
                .get_item("name")?
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("routing rule missing 'name'")
                })?
                .extract()?;
            let priority: u32 = rule_dict
                .get_item("priority")?
                .map(|v| v.extract::<u32>())
                .transpose()?
                .unwrap_or(0);
            let fallback_models: Vec<String> = rule_dict
                .get_item("fallback_models")?
                .map(|v| v.extract::<Vec<String>>())
                .transpose()?
                .unwrap_or_default();
            routing_rules.push(RoutingRule {
                name,
                priority,
                fallback_models,
            });
        }
    }

    // --- quotas ---
    let mut quotas: HashMap<String, Quota> = HashMap::new();
    if let Some(val) = dict.get_item("quotas")? {
        let q_dict = val.downcast::<PyDict>()?;
        for (k, v) in q_dict.iter() {
            let key: String = k.extract()?;
            let q_inner = v.downcast::<PyDict>()?;
            let max_requests_per_minute: Option<u64> = q_inner
                .get_item("max_requests_per_minute")?
                .and_then(|v| if v.is_none() { None } else { Some(v) })
                .map(|v| v.extract())
                .transpose()?;
            let max_tokens_per_minute: Option<u64> = q_inner
                .get_item("max_tokens_per_minute")?
                .and_then(|v| if v.is_none() { None } else { Some(v) })
                .map(|v| v.extract())
                .transpose()?;
            let budget_cents: Option<u64> = q_inner
                .get_item("budget_cents")?
                .and_then(|v| if v.is_none() { None } else { Some(v) })
                .map(|v| v.extract())
                .transpose()?;
            quotas.insert(
                key,
                Quota {
                    max_requests_per_minute,
                    max_tokens_per_minute,
                    budget_cents,
                },
            );
        }
    }

    // --- model_aliases ---
    let model_aliases: HashMap<String, String> =
        if let Some(val) = dict.get_item("model_aliases")? {
            val.extract()?
        } else {
            HashMap::new()
        };

    // --- default_provider ---
    let default_provider: Option<hyperinfer_core::Provider> =
        if let Some(val) = dict.get_item("default_provider")? {
            if val.is_none() {
                None
            } else {
                let s: String = val.extract()?;
                match s.to_lowercase().as_str() {
                    "openai" => Some(hyperinfer_core::Provider::OpenAI),
                    "anthropic" => Some(hyperinfer_core::Provider::Anthropic),
                    _ => {
                        return Err(pyo3::exceptions::PyValueError::new_err(format!(
                            "Unknown provider: '{}'",
                            s
                        )))
                    }
                }
            }
        } else {
            None
        };

    // Suppress unused-variable warning – `py` is required by the signature
    // for lifetime reasons even when not explicitly called.
    let _ = py;

    Ok(Config {
        api_keys,
        routing_rules,
        quotas,
        model_aliases,
        default_provider,
    })
}

#[pyclass]
pub struct HyperInferClient {
    inner: Arc<RwLock<Option<RustClient>>>,
    redis_url: String,
    /// Python config dict stored until `init()` is awaited.
    /// Wrapped in Arc so it can be sent into an async future without requiring
    /// `Py<PyAny>` to implement Clone (which it does not in PyO3 0.28).
    config_dict: Arc<Option<Py<PyAny>>>,
}

#[pymethods]
impl HyperInferClient {
    /// Create a new (uninitialised) client.
    ///
    /// `config` is an optional Python dict as returned by
    /// `hyperinfer.Config.to_dict()`.  When omitted an empty configuration
    /// is used (useful for testing without real API keys).
    #[new]
    #[pyo3(signature = (redis_url, config=None))]
    pub fn new(redis_url: String, config: Option<Py<PyAny>>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            redis_url,
            config_dict: Arc::new(config),
        }
    }

    /// Asynchronously initialise the underlying Rust client.
    ///
    /// Must be awaited before calling `chat()`.
    pub fn init<'a>(&self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        let redis_url = self.redis_url.clone();
        let inner = self.inner.clone();
        let config_dict = self.config_dict.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Deserialise the Python config dict (if provided) on the Python
            // thread, then release the GIL for the async Rust initialisation.
            let config = Python::try_attach(|py| {
                if let Some(dict) = config_dict.as_ref() {
                    let bound: Bound<'_, PyAny> = dict.bind(py).clone();
                    config_from_py(py, &bound)
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
                } else {
                    Ok(Config {
                        api_keys: std::collections::HashMap::new(),
                        routing_rules: Vec::new(),
                        quotas: std::collections::HashMap::new(),
                        model_aliases: std::collections::HashMap::new(),
                        default_provider: None,
                    })
                }
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
            })??;

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
