//! Python bindings for HyperInfer Client Library
//!
//! This crate provides PyO3 bindings to expose the Rust Data Plane functionality
//! to Python environments.

mod client;
mod providers;
mod registry_wrapper;
mod types;

pub use client::{ChunkStream, HyperInferClient};
pub use registry_wrapper::{create_provider_registry, ProviderRegistryWrapper};

use pyo3::prelude::*;

#[pyfunction]
#[pyo3(signature = (public_key, secret_key, langfuse_host=None))]
fn init_langfuse_telemetry(
    public_key: &str,
    secret_key: &str,
    langfuse_host: Option<&str>,
) -> PyResult<()> {
    hyperinfer_client::init_langfuse_telemetry(public_key, secret_key, langfuse_host)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
fn shutdown_telemetry() {
    hyperinfer_client::shutdown_telemetry();
}

#[pymodule]
fn _hyperinfer(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<HyperInferClient>()?;
    m.add_class::<ChunkStream>()?;
    m.add_class::<ProviderRegistryWrapper>()?;
    m.add_function(wrap_pyfunction!(init_langfuse_telemetry, m)?)?;
    m.add_function(wrap_pyfunction!(shutdown_telemetry, m)?)?;
    m.add_function(wrap_pyfunction!(create_provider_registry, m)?)?;
    Ok(())
}
