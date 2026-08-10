//! Minimal Python bridge for the Rust platform engine.

use pyo3::prelude::*;

/// Return a banner proving that Python reached the Rust implementation.
#[pyfunction]
fn rust_banner() -> &'static str {
    "options platform bridge: Rust"
}

/// Native Python module backed by the Rust platform engine.
#[pymodule]
fn platform_py(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(rust_banner, module)?)?;
    Ok(())
}
