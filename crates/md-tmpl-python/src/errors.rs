//! Map Rust [`TemplateError`] variants to the Python exception hierarchy.
//!
//! The Python exceptions live in `md_tmpl._exceptions` and mirror
//! the Rust enum:
//!
//! - `TemplateSyntaxError` ← `Syntax`, `UndefinedVariable`, `UnknownFilter`, `IncludeNotFound`
//! - `MissingParamsError`  ← `MissingParams`
//! - `TypeMismatchError`   ← `TypeMismatch`
//! - `ExtraParamsError`    ← `ExtraParams`
//! - `TemplateError`       ← everything else (Io, `DeclarationsMutated`)

use md_tmpl::TemplateError;
use pyo3::prelude::*;

/// Convert a [`TemplateError`] into the matching Python exception.
///
/// Falls back to the base `TemplateError` (which extends `ValueError`)
/// if the Python exception module isn't importable — this keeps the
/// bindings usable even if `_exceptions.py` is missing.
pub(crate) fn template_error_to_py(err: &TemplateError) -> PyErr {
    Python::attach(|py| {
        let msg = err.to_string();

        // Try to import the exception module.
        let Ok(exc_mod) = py.import("md_tmpl._exceptions") else {
            // Fallback: bare ValueError if import fails.
            return pyo3::exceptions::PyValueError::new_err(msg);
        };

        let exc_name = match &err {
            TemplateError::Syntax(_)
            | TemplateError::UndefinedVariable(_)
            | TemplateError::UnknownFilter(_)
            | TemplateError::IncludeNotFound(_) => "TemplateSyntaxError",

            TemplateError::MissingParams(_) => "MissingParamsError",

            TemplateError::TypeMismatch { .. } => "TypeMismatchError",

            TemplateError::ExtraParams(_) => "ExtraParamsError",

            // Io, DeclarationsMutated, future variants
            _ => "TemplateError",
        };

        match exc_mod.getattr(exc_name) {
            Ok(exc_type) => {
                // PyErr::from_value requires a bound exception instance.
                match exc_type.call1((msg,)) {
                    Ok(instance) => PyErr::from_value(instance),
                    Err(_) => pyo3::exceptions::PyValueError::new_err(err.to_string()),
                }
            }
            Err(_) => pyo3::exceptions::PyValueError::new_err(err.to_string()),
        }
    })
}
