//! `PyO3` wrappers for [`Template`] and [`TemplateCache`].

use std::{collections::HashMap, sync::Arc};

use prompt_templates::{Context, Frontmatter, Template, TemplateCache, VarType};
use pyo3::{prelude::*, types::PyDict};

use crate::convert::py_to_value;

/// A parsed, validated template ready for rendering.
///
/// Load from a file or an in-memory string. Parameters are type-checked
/// against frontmatter declarations at render time.
///
/// The engine is strict by default:
/// - Extra (undeclared) parameters in `render()` raise `ValueError`
/// - Unused declared parameters at parse time raise `ValueError`
///
/// Use `allow_extra=True` in render calls, or
/// `from_source_allowing_unused()` to relax these checks.
///
/// Examples:
///
/// ```python
/// from prompt_templates import Template
/// tmpl = Template.from_source("---\nparams:\n  - name = str\n---\nHello {{ name }}!")
/// tmpl.render(name="world")
/// # => 'Hello world!'
/// ```
#[pyclass(name = "Template")]
pub(crate) struct PyTemplate {
    inner: Template,
    frontmatter: Frontmatter,
}

#[pymethods]
impl PyTemplate {
    /// Load a template from a `.tmpl.md` file.
    ///
    /// Args:
    ///     path: Path to the template file.
    ///
    /// Returns:
    ///     Template: A parsed and validated template.
    ///
    /// Raises:
    ///     `ValueError`: If the file cannot be read or contains syntax errors.
    #[staticmethod]
    fn from_file(path: &str) -> PyResult<Self> {
        let (tmpl, fm) = Template::from_file_with_frontmatter(std::path::Path::new(path))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: tmpl,
            frontmatter: fm,
        })
    }

    /// Parse a template from an in-memory string.
    ///
    /// Unused declared parameters (present in frontmatter but not in
    /// the template body) are rejected. Use `from_source_allowing_unused()`
    /// to suppress this check.
    ///
    /// Args:
    ///     source: Template source with YAML frontmatter.
    ///
    /// Returns:
    ///     Template: A parsed and validated template.
    ///
    /// Raises:
    ///     `ValueError`: If the source contains syntax errors or unused params.
    #[staticmethod]
    fn from_source(source: &str) -> PyResult<Self> {
        let (tmpl, fm) = Template::from_source_with_frontmatter(source)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: tmpl,
            frontmatter: fm,
        })
    }

    /// Parse a template, allowing declared parameters that aren't used.
    ///
    /// Like `from_source()`, but does not reject parameters that are
    /// declared in frontmatter but never referenced in the template body.
    ///
    /// Args:
    ///     source: Template source with YAML frontmatter.
    ///
    /// Returns:
    ///     Template: A parsed template.
    ///
    /// Raises:
    ///     `ValueError`: If the source contains syntax errors.
    #[staticmethod]
    fn from_source_allowing_unused(source: &str) -> PyResult<Self> {
        let fm = prompt_templates::parse_frontmatter(source)
            .map(|(fm, _)| fm)
            .unwrap_or_default();
        let tmpl = Template::from_source_allowing_unused(source)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: tmpl,
            frontmatter: fm,
        })
    }

    /// Render the template with the given keyword arguments (strict).
    ///
    /// All arguments are validated against the frontmatter type declarations.
    /// Missing parameters, type mismatches, and **extra undeclared parameters**
    /// produce clear error messages.
    ///
    /// Pass `allow_extra=True` to ignore extra parameters.
    ///
    /// Args:
    ///     `allow_extra`: If True, extra kwargs not declared in frontmatter
    ///         are silently ignored. Default: False.
    ///     **kwargs: Template parameters as keyword arguments.
    ///
    /// Returns:
    ///     str: The rendered output.
    ///
    /// Raises:
    ///     `TypeError`: If a value cannot be converted to a template type.
    ///     `ValueError`: If validation or rendering fails.
    ///
    /// Examples:
    ///     >>> tmpl.render(name="world", count=42)
    #[pyo3(signature = (*, allow_extra=false, **kwargs))]
    fn render(&self, allow_extra: bool, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        let ctx = build_context(kwargs)?;
        let result = if allow_extra {
            self.inner.render_allowing_extra(&ctx)
        } else {
            self.inner.render(&ctx)
        };
        result.map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Render the template from a dict of parameters.
    ///
    /// Equivalent to `render(**params)` but accepts a dict directly.
    /// Extra keys are rejected by default.
    ///
    /// Args:
    ///     params: Dictionary of template parameters.
    ///     `allow_extra`: If True, extra keys are silently ignored.
    ///
    /// Returns:
    ///     str: The rendered output.
    ///
    /// Raises:
    ///     `ValueError`: If validation or rendering fails.
    #[pyo3(signature = (params, *, allow_extra=false))]
    fn render_dict(&self, params: &Bound<'_, PyDict>, allow_extra: bool) -> PyResult<String> {
        let ctx = build_context(Some(params))?;
        let result = if allow_extra {
            self.inner.render_allowing_extra(&ctx)
        } else {
            self.inner.render(&ctx)
        };
        result.map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Return the declared parameter names and their types.
    ///
    /// Returns:
    ///     `list[tuple[str, str]]`: Each element is (name, `type_string`).
    ///
    /// Examples:
    ///
    /// ```python
    /// tmpl.declarations()
    /// # => [('reviewer', 'str'), ('status', 'enum<...>')]
    /// ```
    fn declarations(&self) -> Vec<(String, String)> {
        self.inner
            .declarations()
            .iter()
            .map(|d| (d.name.clone(), d.var_type.to_string()))
            .collect()
    }

    /// Return the content hash of the template source.
    ///
    /// Two templates with the same source produce the same hash.
    /// Useful for detecting unchanged files during hot-reload.
    ///
    /// Returns:
    ///     int: A non-cryptographic content hash.
    fn source_hash(&self) -> u64 {
        self.inner.source_hash()
    }

    /// Return default values for parameters that declare them.
    ///
    /// Returns:
    ///     dict: Mapping of parameter name → default value.
    fn defaults(&self, py: Python<'_>) -> PyResult<PyObject> {
        let defaults = self.inner.defaults();
        let dict = PyDict::new(py);
        for (k, v) in &defaults {
            dict.set_item(k, crate::convert::value_to_py(py, v)?)?;
        }
        Ok(dict.into_any().unbind())
    }

    /// Return constants defined in the template's frontmatter `consts:` block.
    ///
    /// Returns:
    ///     dict: Mapping of constant name → value.
    fn consts(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for decl in &self.frontmatter.consts {
            if let Some(ref value) = decl.default_value {
                dict.set_item(&decl.name, crate::convert::value_to_py(py, value)?)?;
            }
        }
        Ok(dict.into_any().unbind())
    }

    /// Return constants imported from other templates.
    ///
    /// These are keyed by `stem.NAME` (e.g. `other.MAX_RETRIES`).
    ///
    /// Returns:
    ///     dict: Mapping of qualified constant name → value.
    fn imported_consts(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (k, v) in &self.frontmatter.imported_consts {
            dict.set_item(k, crate::convert::value_to_py(py, v)?)?;
        }
        Ok(dict.into_any().unbind())
    }

    /// Validate that this template's declarations match an expected set.
    ///
    /// Call this after re-loading a template from disk to ensure that
    /// the `params:` block hasn't been modified.
    ///
    /// Args:
    ///     expected: List of (name, `type_string`) tuples.
    ///
    /// Raises:
    ///     `ValueError`: If the declarations don't match.
    fn validate_declarations_against(&self, expected: Vec<(String, String)>) -> PyResult<()> {
        let current = self.declarations();
        // Consume `expected` by iterating; a plain `==` only borrows.
        let matches =
            current.len() == expected.len() && current.iter().zip(expected).all(|(a, b)| *a == b);
        if matches {
            Ok(())
        } else {
            Err(pyo3::exceptions::PyValueError::new_err(format!(
                "template declarations changed: got {current:?}"
            )))
        }
    }

    fn __repr__(&self) -> String {
        let decls: Vec<String> = self
            .inner
            .declarations()
            .iter()
            .map(|d| format!("{}={}", d.name, d.var_type))
            .collect();
        format!("Template(params=[{}])", decls.join(", "))
    }
}

impl PyTemplate {
    /// Borrow the inner Rust `Template` — used by the Python import hook
    /// when generating types.
    pub(crate) fn inner(&self) -> &Template {
        &self.inner
    }

    /// Access the type aliases from this template's frontmatter.
    pub(crate) fn type_aliases(&self) -> &HashMap<String, VarType> {
        &self.frontmatter.type_aliases
    }
}

/// Content-hashed template cache for hot-reload scenarios.
///
/// Unchanged files return cached compilations with zero re-parsing.
///
/// Examples:
///     >>> cache = `TemplateCache()`
///     >>> tmpl = cache.load("prompts/greeting.tmpl.md")
///     >>> tmpl.render(name="world")
#[pyclass(name = "TemplateCache")]
pub(crate) struct PyTemplateCache {
    inner: Arc<TemplateCache>,
}

#[pymethods]
impl PyTemplateCache {
    /// Create a new empty template cache.
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(TemplateCache::new()),
        }
    }

    /// Load a template, returning a cached version if unchanged.
    ///
    /// Args:
    ///     path: Path to the `.tmpl.md` file.
    ///
    /// Returns:
    ///     Template: A parsed template (possibly from cache).
    ///
    /// Raises:
    ///     `ValueError`: If the file cannot be read or contains errors.
    fn load(&self, path: &str) -> PyResult<PyTemplate> {
        let p = std::path::Path::new(path);
        let (tmpl, fm) = self
            .inner
            .load_with_frontmatter(p)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyTemplate {
            inner: tmpl,
            frontmatter: fm,
        })
    }
}

/// Build a [`Context`] from Python keyword arguments.
fn build_context(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Context> {
    let Some(kwargs) = kwargs else {
        return Ok(Context::new());
    };
    let mut ctx = Context::with_capacity(kwargs.len());
    for (key, value) in kwargs.iter() {
        let key_str: String = key.extract()?;
        let val = py_to_value(&value)?;
        ctx.set(key_str, val);
    }
    Ok(ctx)
}

/// Create a `PyTemplate` from a file path (used by typegen).
pub(crate) fn load_template(path: &str) -> PyResult<PyTemplate> {
    PyTemplate::from_file(path)
}
