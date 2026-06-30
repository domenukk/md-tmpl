#![forbid(unsafe_code)]
//! `PyO3` Python bindings for the `md-tmpl` engine.
//!
//! This crate exposes the core Rust template engine to Python, providing:
//!
//! - `PyTemplate`: Load, validate, and render `.tmpl.md` templates.
//! - `PyTemplateCache`: Content-hashed caching layer.
//! - Dynamic type generation: Python classes for enums and models are
//!   generated from frontmatter declarations at template load time.
//! - Value conversion: Python dicts, lists, strings, ints, floats, bools,
//!   and generated enum instances are transparently converted to the engine's
//!   internal [`Value`](md_tmpl::Value) type.

mod convert;
mod errors;
mod pyclass_builder;
mod template;
mod typegen;

use pyo3::prelude::*;

/// Native extension module for `md_tmpl`.
///
/// Exposes `Template`, `TemplateCache`, and helper functions to
/// the pure-Python `md_tmpl` package.
#[pymodule]
fn _md_tmpl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<template::PyTemplate>()?;
    m.add_class::<template::PyTemplateCache>()?;
    m.add_function(wrap_pyfunction!(typegen::generate_types_for_template, m)?)?;
    m.add_function(wrap_pyfunction!(
        typegen::generate_python_source_for_template,
        m
    )?)?;
    Ok(())
}
