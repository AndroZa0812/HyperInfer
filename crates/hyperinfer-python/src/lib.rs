//! Python bindings for HyperInfer Client Library
//!
//! This crate provides PyO3 bindings to expose the Rust Data Plane functionality
//! to Python environments.

mod client;
mod types;

pub use client::HyperInferClient;

use pyo3::prelude::*;

#[pymodule]
fn _hyperinfer(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<HyperInferClient>()?;
    Ok(())
}
