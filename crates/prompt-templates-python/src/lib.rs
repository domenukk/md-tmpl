#![forbid(unsafe_code)]
//! `PyO3` Python bindings for the `prompt-templates` engine.
//!
//! This crate exposes the core Rust template engine to Python, providing:
//!
//! - `PyTemplate`: Load, validate, and render `.tmpl.md` templates.
//! - `PyTemplateCache`: Content-hashed caching layer.
//! - Dynamic type generation: Python classes for enums and models are
//!   generated from frontmatter declarations at template load time.
//! - Value conversion: Python dicts, lists, strings, ints, floats, bools,
//!   and generated enum instances are transparently converted to the engine's
//!   internal [`Value`](prompt_templates::Value) type.

mod convert;
mod pyclass_builder;
mod template;
mod typegen;

use pyo3::prelude::*;

/// Native extension module for `prompt_templates`.
///
/// Exposes `Template`, `TemplateCache`, and helper functions to
/// the pure-Python `prompt_templates` package.
#[pymodule]
fn _prompt_templates(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<template::PyTemplate>()?;
    m.add_class::<template::PyTemplateCache>()?;
    m.add_function(wrap_pyfunction!(typegen::generate_types_for_template, m)?)?;
    Ok(())
}
