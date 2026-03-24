use futures::StreamExt;
use hyperinfer_client::HyperInferClient as RustClient;
use hyperinfer_core::types::Quota;
use hyperinfer_core::{ChatChunk, ChatResponse, Config, HyperInferError, RoutingRule};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use super::registry_wrapper::ProviderRegistryWrapper;

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
    let dict = obj.cast::<PyDict>()?;

    // --- api_keys ---
    let api_keys: HashMap<String, String> = if let Some(val) = dict.get_item("api_keys")? {
        val.extract()?
    } else {
        HashMap::new()
    };

    // --- routing_rules ---
    let mut routing_rules: Vec<RoutingRule> = Vec::new();
    if let Some(val) = dict.get_item("routing_rules")? {
        let list = val.cast::<PyList>()?;
        for item in list.iter() {
            let rule_dict = item.cast::<PyDict>()?;
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
        let q_dict = val.cast::<PyDict>()?;
        for (k, v) in q_dict.iter() {
            let key: String = k.extract()?;
            let q_inner = v.cast::<PyDict>()?;
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

/// Python async iterator backed by an `Arc<Mutex<mpsc::Receiver<…>>>`.
#[pyclass]
pub struct ChunkStream {
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<Result<ChatChunk, String>>>>,
}

#[pymethods]
impl ChunkStream {
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__<'a>(&self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        let rx = self.rx.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut guard = rx.lock().await;
            match guard.recv().await {
                Some(Ok(chunk)) => {
                    // Convert the chunk to a Python dict.
                    Python::try_attach(|py| {
                        let dict = PyDict::new(py);
                        dict.set_item("id", &chunk.id)?;
                        dict.set_item("model", &chunk.model)?;
                        dict.set_item("delta", &chunk.delta)?;
                        dict.set_item(
                            "finish_reason",
                            chunk.finish_reason.as_deref().map(|s| s.to_string()),
                        )?;
                        if let Some(u) = &chunk.usage {
                            let usage = PyDict::new(py);
                            usage.set_item("input_tokens", u.input_tokens)?;
                            usage.set_item("output_tokens", u.output_tokens)?;
                            dict.set_item("usage", usage)?;
                        } else {
                            dict.set_item("usage", py.None())?;
                        }
                        Ok(dict.into_any().unbind())
                    })
                    .ok_or_else(|| {
                        pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
                    })?
                }
                Some(Err(e)) => Err(pyo3::exceptions::PyRuntimeError::new_err(e)),
                // Channel closed — signal end of async iteration.
                None => Err(pyo3::exceptions::PyStopAsyncIteration::new_err(())),
            }
        })
    }
}

#[pyclass]
pub struct HyperInferClient {
    inner: Arc<RwLock<Option<RustClient>>>,
    redis_url: String,
    /// Python config dict stored until `init()` is awaited.
    /// Wrapped in Arc<RwLock> so we can take() and drop the Py object after init.
    config_dict: Arc<RwLock<Option<Py<PyAny>>>>,
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
            config_dict: Arc::new(RwLock::new(config)),
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
            // Acquire write lock upfront for atomic check-and-set.
            let mut inner_guard = inner.write().await;
            if inner_guard.is_some() {
                return Python::try_attach(|py| Ok(py.None())).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
                })?;
            }

            // Parse configuration (still holding inner write lock to prevent races).
            let config_guard = config_dict.read().await;
            let config = match Python::try_attach(|py| {
                if let Some(dict) = &*config_guard {
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
            }) {
                Some(Ok(config)) => config,
                Some(Err(e)) => return Err(e),
                None => {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "Failed to attach to Python",
                    ))
                }
            };
            drop(config_guard);

            // Instantiate client.
            let client = RustClient::new(&redis_url, config)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            // Store the client (we already hold the write lock).
            *inner_guard = Some(client);
            drop(inner_guard);

            let mut config_guard = config_dict.write().await;
            config_guard.take();
            drop(config_guard);

            Python::try_attach(|py| Ok(py.None())).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
            })?
        })
    }

    /// Asynchronously initialise the underlying Rust client with a shared provider registry.
    ///
    /// This allows Python-registered providers (via ProviderRegistryWrapper) to be used
    /// by the HyperInferClient for LLM calls.
    pub fn init_with_registry<'a>(
        &self,
        py: Python<'a>,
        registry_wrapper: &ProviderRegistryWrapper,
    ) -> PyResult<Bound<'a, PyAny>> {
        let redis_url = self.redis_url.clone();
        let inner = self.inner.clone();
        let config_dict = self.config_dict.clone();
        let registry = registry_wrapper.get_registry();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Acquire write lock upfront for atomic check-and-set.
            let mut inner_guard = inner.write().await;
            if inner_guard.is_some() {
                return Python::try_attach(|py| Ok(py.None())).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
                })?;
            }

            // Parse configuration (still holding inner write lock to prevent races).
            let config_guard = config_dict.read().await;
            let config = match Python::try_attach(|py| {
                if let Some(dict) = &*config_guard {
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
            }) {
                Some(Ok(config)) => config,
                Some(Err(e)) => return Err(e),
                None => {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "Failed to attach to Python",
                    ))
                }
            };
            drop(config_guard);

            // Instantiate client.
            let client = RustClient::new(&redis_url, config)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            // Inject the external registry's providers into the client's registry.
            client.inject_provider_registry(&registry);

            // Store the client (we already hold the write lock).
            *inner_guard = Some(client);
            drop(inner_guard);

            let mut config_guard = config_dict.write().await;
            config_guard.take();
            drop(config_guard);

            Python::try_attach(|py| Ok(py.None())).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
            })?
        })
    }

    /// Configure traffic mirroring. Pass `None` to disable.
    #[pyo3(signature = (model=None, sample_rate=None))]
    pub fn set_mirror<'a>(
        &self,
        py: Python<'a>,
        model: Option<String>,
        sample_rate: Option<f64>,
    ) -> PyResult<Bound<'a, PyAny>> {
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.read().await;
            let client = guard.as_ref().ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "Client not initialized. Call init() first.",
                )
            })?;

            let mirror_cfg = match (model, sample_rate) {
                (Some(m), Some(sr)) => Some(hyperinfer_client::MirrorConfig::new(m, sr)),
                (Some(_), None) | (None, Some(_)) => {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "Both 'model' and 'sample_rate' must be provided to enable mirroring",
                    ));
                }
                _ => None,
            };

            client.set_mirror(mirror_cfg).await;

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

    /// Return a `ChunkStream` async iterator that yields token-delta dicts.
    ///
    /// Usage:
    /// ```python
    /// async for chunk in await client.chat_stream(key, request):
    ///     print(chunk["delta"], end="", flush=True)
    /// ```
    #[pyo3(name = "chat_stream")]
    pub fn chat_stream<'a>(
        &self,
        py: Python<'a>,
        key: String,
        request: Py<PyAny>,
    ) -> PyResult<Bound<'a, PyAny>> {
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = inner.read().await;
            let client = guard.as_ref().ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "Client not initialized. Call init() first.",
                )
            })?;

            let chat_request = Python::try_attach(|py| {
                super::types::request_from_py(py, request)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
            })??;

            // Obtain the stream from the Rust client.
            let mut stream = client
                .chat_stream(&key, chat_request)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            // Bridge the Rust stream to a Python async iterator via a channel.
            // Buffer size of 32 keeps memory bounded while the consumer iterates.
            let (tx, rx) = mpsc::channel::<Result<ChatChunk, String>>(32);

            tokio::spawn(async move {
                while let Some(item) = stream.next().await {
                    let send_result = match item {
                        Ok(chunk) => tx.send(Ok(chunk)).await,
                        Err(e) => tx.send(Err(e.to_string())).await,
                    };
                    if send_result.is_err() {
                        // Receiver dropped — consumer stopped iterating.
                        break;
                    }
                }
                // tx is dropped here, closing the channel and signalling StopAsyncIteration.
            });

            Python::try_attach(|py| {
                let iter = ChunkStream {
                    rx: Arc::new(tokio::sync::Mutex::new(rx)),
                };
                Ok(Py::new(py, iter)?.into_bound(py).into_any().unbind())
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to attach to Python")
            })?
        })
    }
}
