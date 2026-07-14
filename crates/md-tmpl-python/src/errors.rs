//! Map Rust [`TemplateError`] variants to the Python exception hierarchy.
//!
//! The Python exceptions live in `md_tmpl._exceptions` and now map 1:1
//! onto the Rust enum, so each variant surfaces as its own Python type
//! carrying a stable `kind` attribute and structured fields:
//!
//! - `TemplateSyntaxError`       ← `Syntax` (with `line`, `snippet`)
//! - `UndefinedVariableError`    ← `UndefinedVariable` (with `variable`)
//! - `MissingParamsError`        ← `MissingParams` (with `missing`)
//! - `TypeMismatchError`         ← `TypeMismatch` (with `path`, `expected`, `actual`)
//! - `UnknownFilterError`        ← `UnknownFilter` (with `filter`)
//! - `IncludeNotFoundError`      ← `IncludeNotFound` (with `include`)
//! - `DeclarationsMutatedError`  ← `DeclarationsMutated` (with `details`)
//! - `ExtraParamsError`          ← `ExtraParams` (with `extra`)
//! - `TemplatePanicError`        ← `Panic`
//! - `TemplateError`             ← `Io` (constructed with `kind="io"`)

use md_tmpl::TemplateError;
use pyo3::{prelude::*, types::PyDict};

/// Convert a [`TemplateError`] into the matching Python exception.
///
/// Each variant is mapped to its own Python exception class, passing the
/// human-readable message (`err.to_string()`) as the first positional
/// argument and the variant's structured payload as keyword arguments.
///
/// Falls back to a bare `ValueError` (still a superclass of every
/// `TemplateError`) if the `_exceptions` module can't be imported, or if
/// looking up / constructing the specific exception type fails — this keeps
/// the bindings usable even if `_exceptions.py` is missing or out of sync.
pub(crate) fn template_error_to_py(err: &TemplateError) -> PyErr {
    Python::attach(|py| {
        let msg = err.to_string();

        // Try to import the exception module. Fall back to a bare
        // ValueError if it isn't importable.
        let Ok(exc_mod) = py.import("md_tmpl._exceptions") else {
            return pyo3::exceptions::PyValueError::new_err(msg);
        };

        // Select the target exception class name and build its keyword
        // arguments from the variant payload.
        let (exc_name, kwargs) = match err {
            TemplateError::Syntax(syntax) => {
                let kwargs = PyDict::new(py);
                let line = syntax.line.map(|l| l as u64);
                if kwargs.set_item("line", line).is_err()
                    || kwargs.set_item("snippet", syntax.snippet.clone()).is_err()
                {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("TemplateSyntaxError", Some(kwargs))
            }

            TemplateError::UndefinedVariable(name) => {
                let kwargs = PyDict::new(py);
                if kwargs.set_item("variable", name.clone()).is_err() {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("UndefinedVariableError", Some(kwargs))
            }

            TemplateError::MissingParams(names) => {
                let kwargs = PyDict::new(py);
                if kwargs.set_item("missing", names.clone()).is_err() {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("MissingParamsError", Some(kwargs))
            }

            TemplateError::TypeMismatch {
                name,
                expected,
                actual,
                ..
            } => {
                let kwargs = PyDict::new(py);
                if kwargs.set_item("path", name.clone()).is_err()
                    || kwargs.set_item("expected", expected.clone()).is_err()
                    || kwargs.set_item("actual", actual.clone()).is_err()
                {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("TypeMismatchError", Some(kwargs))
            }

            TemplateError::UnknownFilter(name) => {
                let kwargs = PyDict::new(py);
                if kwargs.set_item("filter", name.clone()).is_err() {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("UnknownFilterError", Some(kwargs))
            }

            TemplateError::IncludeNotFound(path) => {
                let kwargs = PyDict::new(py);
                if kwargs.set_item("include", path.clone()).is_err() {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("IncludeNotFoundError", Some(kwargs))
            }

            TemplateError::DeclarationsMutated { details } => {
                let kwargs = PyDict::new(py);
                if kwargs.set_item("details", details.clone()).is_err() {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("DeclarationsMutatedError", Some(kwargs))
            }

            TemplateError::ExtraParams(names) => {
                let kwargs = PyDict::new(py);
                if kwargs.set_item("extra", names.clone()).is_err() {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("ExtraParamsError", Some(kwargs))
            }

            TemplateError::Panic(_) => ("TemplatePanicError", None),

            // Io: use the base TemplateError, tagging it with kind="io".
            // Matching Io explicitly (rather than `_`) is deliberate: if the
            // core adds a new variant, this match fails to compile, forcing us
            // to keep the typed-error parity contract up to date.
            TemplateError::Io(_) => {
                let kwargs = PyDict::new(py);
                if kwargs.set_item("kind", "io").is_err() {
                    return pyo3::exceptions::PyValueError::new_err(msg);
                }
                ("TemplateError", Some(kwargs))
            }
        };

        let Ok(exc_type) = exc_mod.getattr(exc_name) else {
            return pyo3::exceptions::PyValueError::new_err(msg);
        };

        let constructed = match kwargs {
            Some(kwargs) => exc_type.call((msg.clone(),), Some(&kwargs)),
            None => exc_type.call1((msg.clone(),)),
        };

        match constructed {
            Ok(instance) => PyErr::from_value(instance),
            Err(_) => pyo3::exceptions::PyValueError::new_err(msg),
        }
    })
}
