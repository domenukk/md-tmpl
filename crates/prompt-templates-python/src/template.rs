//! `PyO3` wrappers for [`Template`] and [`TemplateCache`].

use std::sync::Arc;

use hashbrown::HashMap;
use prompt_templates::{CompileOptions, Context, Frontmatter, Template, TemplateCache, VarType};
use pyo3::{Py, prelude::*, types::PyDict};

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
    #[allow(clippy::needless_pass_by_value)] // PyO3 requires owned PathBuf
    fn from_file(path: std::path::PathBuf) -> PyResult<Self> {
        let (tmpl, fm) = Template::compile_file(&path, CompileOptions::default())
            .map_err(|e| crate::errors::template_error_to_py(&e))?;
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
        let (tmpl, fm) = Template::compile(source, CompileOptions::default())
            .map_err(|e| crate::errors::template_error_to_py(&e))?;
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
        let (tmpl, fm) = Template::compile(source, CompileOptions::default().allow_unused(true))
            .map_err(|e| crate::errors::template_error_to_py(&e))?;
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
        if allow_extra {
            self.inner
                .render_ctx_allowing_extra(&ctx)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        } else {
            self.inner
                .render_ctx(&ctx)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        }
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
        if allow_extra {
            self.inner
                .render_ctx_allowing_extra(&ctx)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        } else {
            self.inner
                .render_ctx(&ctx)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        }
    }

    /// Render the template using a cache for include resolution.
    ///
    /// Includes are resolved from the cache instead of reading files
    /// from disk, improving performance when rendering many templates
    /// with shared includes.
    ///
    /// Args:
    ///     cache: A `TemplateCache` instance for include resolution.
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
    #[pyo3(signature = (cache, *, allow_extra=false, **kwargs))]
    fn render_cached(
        &self,
        cache: &PyTemplateCache,
        allow_extra: bool,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        let ctx = build_context(kwargs)?;
        if allow_extra {
            self.inner
                .render_ctx_cached_allowing_extra(&ctx, &*cache.inner)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        } else {
            self.inner
                .render_ctx_cached(&ctx, &*cache.inner)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        }
    }

    /// Render the template from a dict, using a cache for includes.
    ///
    /// Combines `render_dict()` with cache-aware include resolution.
    ///
    /// Args:
    ///     params: Dictionary of template parameters.
    ///     cache: A `TemplateCache` instance for include resolution.
    ///     `allow_extra`: If True, extra keys are silently ignored.
    ///
    /// Returns:
    ///     str: The rendered output.
    #[pyo3(signature = (params, cache, *, allow_extra=false))]
    fn render_cached_dict(
        &self,
        params: &Bound<'_, PyDict>,
        cache: &PyTemplateCache,
        allow_extra: bool,
    ) -> PyResult<String> {
        let ctx = build_context(Some(params))?;
        if allow_extra {
            self.inner
                .render_ctx_cached_allowing_extra(&ctx, &*cache.inner)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        } else {
            self.inner
                .render_ctx_cached(&ctx, &*cache.inner)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        }
    }

    /// Render the template from a `FlexBuffers` binary buffer.
    ///
    /// Args:
    ///     buffer: `FlexBuffers` binary buffer (bytes).
    ///     `allow_extra`: If True, extra keys are silently ignored.
    ///
    /// Returns:
    ///     str: The rendered output.
    #[pyo3(signature = (buffer, *, allow_extra=false))]
    fn render_flexbuffers(&self, buffer: &[u8], allow_extra: bool) -> PyResult<String> {
        let ctx = prompt_templates::Context::from_flexbuffers(buffer)
            .map_err(|e| crate::errors::template_error_to_py(&e))?;
        if allow_extra {
            self.inner
                .render_ctx_allowing_extra(&ctx)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        } else {
            self.inner
                .render_ctx(&ctx)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        }
    }

    /// Render the template from a `FlexBuffers` binary buffer, using a cache for includes.
    ///
    /// Args:
    ///     buffer: `FlexBuffers` binary buffer (bytes).
    ///     cache: A `TemplateCache` instance for include resolution.
    ///     `allow_extra`: If True, extra keys are silently ignored.
    ///
    /// Returns:
    ///     str: The rendered output.
    #[pyo3(signature = (buffer, cache, *, allow_extra=false))]
    fn render_cached_flexbuffers(
        &self,
        buffer: &[u8],
        cache: &PyTemplateCache,
        allow_extra: bool,
    ) -> PyResult<String> {
        let ctx = prompt_templates::Context::from_flexbuffers(buffer)
            .map_err(|e| crate::errors::template_error_to_py(&e))?;
        if allow_extra {
            self.inner
                .render_ctx_cached_allowing_extra(&ctx, &*cache.inner)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        } else {
            self.inner
                .render_ctx_cached(&ctx, &*cache.inner)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        }
    }

    /// Render the template from a JSON string.
    ///
    /// This is the **fastest rendering path from Python** — use it when your
    /// parameters are JSON-serialisable and you want maximum throughput.
    ///
    /// Why is this faster than `render(**kwargs)`?
    /// - `render(**kwargs)` does N per-key Python→Rust FFI crossings with
    ///   `isinstance` type checks on each value.
    /// - `render_json()` does a single FFI crossing; `CPython`'s C-optimised
    ///   `json.dumps` serialises the dict (~1µs), then Rust deserialises the
    ///   JSON string directly into the template's internal `Value` type.
    ///
    /// Args:
    ///     `json_str`: A JSON string representing a dict of template parameters.
    ///         Use `json.dumps(params)` to produce this.
    ///     `allow_extra`: If True, extra keys are silently ignored.
    ///
    /// Returns:
    ///     str: The rendered output.
    ///
    /// Raises:
    ///     `ValueError`: If the JSON is invalid or rendering fails.
    ///
    /// Examples:
    ///
    /// ```python
    /// import json
    /// from prompt_templates import Template
    ///
    /// tmpl = Template.from_source("---\nparams:\n  - name = str\n---\nHello {{ name }}!")
    /// tmpl.render_json(json.dumps({"name": "world"}))
    /// # => 'Hello world!'
    /// ```
    #[pyo3(signature = (json_str, *, allow_extra=false))]
    fn render_json(&self, json_str: &str, allow_extra: bool) -> PyResult<String> {
        let ctx = json_str_to_context(json_str)?;
        if allow_extra {
            self.inner
                .render_ctx_allowing_extra(&ctx)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        } else {
            self.inner
                .render_ctx(&ctx)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        }
    }

    /// Render the template from a JSON string, using a cache for includes.
    ///
    /// Combines `render_json()` with cache-aware include resolution.
    ///
    /// Args:
    ///     `json_str`: A JSON string representing a dict of template parameters.
    ///     cache: A `TemplateCache` instance for include resolution.
    ///     `allow_extra`: If True, extra keys are silently ignored.
    ///
    /// Returns:
    ///     str: The rendered output.
    #[pyo3(signature = (json_str, cache, *, allow_extra=false))]
    fn render_json_cached(
        &self,
        json_str: &str,
        cache: &PyTemplateCache,
        allow_extra: bool,
    ) -> PyResult<String> {
        let ctx = json_str_to_context(json_str)?;
        if allow_extra {
            self.inner
                .render_ctx_cached_allowing_extra(&ctx, &*cache.inner)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        } else {
            self.inner
                .render_ctx_cached(&ctx, &*cache.inner)
                .map_err(|e| crate::errors::template_error_to_py(&e))
        }
    }

    /// Render a template that takes no user-provided parameters.
    ///
    /// If the template declares parameters, those must all have defaults.
    /// Calling ``render_empty()`` on a template with required (no-default)
    /// parameters raises ``ValueError``.
    ///
    /// More efficient than ``render()`` with no kwargs — skips context
    /// building entirely.
    ///
    /// Returns:
    ///     str: The rendered output.
    ///
    /// Raises:
    ///     ``ValueError``: If any declared parameter lacks a default value.
    ///
    /// Examples:
    ///
    /// ```python
    /// tmpl = Template.from_source("Hello world!")
    /// tmpl.render_empty()  # => 'Hello world!'
    ///
    /// tmpl = Template.from_source("---\nparams:\n  - name = str := \"world\"\n---\nHello {{ name }}!")
    /// tmpl.render_empty()  # => 'Hello world!'
    /// ```
    fn render_empty(&self) -> PyResult<String> {
        self.inner
            .render_empty()
            .map_err(|e| crate::errors::template_error_to_py(&e))
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
    fn defaults(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
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
    fn consts(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
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
    fn imported_consts(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
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

    /// Parse a template from source with a base directory for include resolution.
    ///
    /// Use this when your template source contains `{% include %}` directives
    /// and the included files should be resolved relative to `base_dir`.
    ///
    /// Args:
    ///     source: Template source with YAML frontmatter.
    ///     `base_dir`: Directory path for resolving includes.
    ///
    /// Returns:
    ///     Template: A parsed and validated template.
    ///
    /// Raises:
    ///     `ValueError`: If the source contains syntax errors or includes cannot be resolved.
    #[staticmethod]
    #[allow(clippy::needless_pass_by_value)] // PyO3 requires owned PathBuf
    fn from_source_with_base_dir(source: &str, base_dir: std::path::PathBuf) -> PyResult<Self> {
        let (tmpl, fm) = Template::compile(source, CompileOptions::default().base_dir(&base_dir))
            .map_err(|e| crate::errors::template_error_to_py(&e))?;
        Ok(Self {
            inner: tmpl,
            frontmatter: fm,
        })
    }

    /// Return the raw template body after frontmatter stripping.
    ///
    /// Returns:
    ///     str: The template body text.
    fn body(&self) -> String {
        self.inner.body().to_string()
    }

    /// Set the maximum include depth for rendering this template.
    ///
    /// Controls how deeply nested `{% include %}` directives can recurse.
    ///
    /// Args:
    ///     depth: Maximum nesting depth for includes.
    fn set_max_include_depth(&mut self, depth: usize) {
        self.inner.set_max_include_depth(depth);
    }

    /// Enter the context manager — returns `self` unchanged.
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Exit the context manager — never suppresses exceptions.
    #[allow(clippy::unused_self)]
    fn __exit__(
        &self,
        _exc_type: &Bound<'_, PyAny>,
        _exc_val: &Bound<'_, PyAny>,
        _exc_tb: &Bound<'_, PyAny>,
    ) -> bool {
        false
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

    /// Construct from a pre-existing `Template` and `Frontmatter`.
    ///
    /// Used by `value_to_py` to wrap `Value::Tmpl` back into a Python object.
    pub(crate) fn from_inner(inner: Template, frontmatter: Frontmatter) -> Self {
        Self { inner, frontmatter }
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
    ///
    /// Args:
    ///     `max_entries`: Optional maximum number of cached templates.
    ///         When exceeded, the least-recently-used entry is evicted.
    ///         ``None`` (default) disables eviction.
    #[new]
    #[pyo3(signature = (*, max_entries=None))]
    fn new(max_entries: Option<usize>) -> Self {
        let mut cache = TemplateCache::new();
        if let Some(max) = max_entries {
            cache = cache.with_max_entries(max);
        }
        Self {
            inner: Arc::new(cache),
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
    #[allow(clippy::needless_pass_by_value)] // PyO3 requires owned PathBuf
    fn load(&self, path: std::path::PathBuf) -> PyResult<PyTemplate> {
        let (tmpl, fm) = self
            .inner
            .load_with_frontmatter(&path)
            .map_err(|e| crate::errors::template_error_to_py(&e))?;
        Ok(PyTemplate {
            inner: tmpl,
            frontmatter: fm,
        })
    }

    /// Invalidate all cached entries.
    ///
    /// Call after a bulk file update to force re-compilation on next load.
    fn clear(&self) {
        self.inner.clear();
    }

    /// Return the number of cached main templates.
    ///
    /// Returns:
    ///     int: Number of cached templates.
    fn template_count(&self) -> usize {
        self.inner.template_count()
    }

    /// Return the number of cached include templates.
    ///
    /// Returns:
    ///     int: Number of cached includes.
    fn include_count(&self) -> usize {
        self.inner.include_count()
    }

    /// Return the total number of cached entries (templates + includes).
    ///
    /// Returns:
    ///     int: Number of cached entries.
    fn __len__(&self) -> usize {
        self.inner.template_count() + self.inner.include_count()
    }

    /// Enter the context manager — returns `self` unchanged.
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Exit the context manager — never suppresses exceptions.
    #[allow(clippy::unused_self)]
    fn __exit__(
        &self,
        _exc_type: &Bound<'_, PyAny>,
        _exc_val: &Bound<'_, PyAny>,
        _exc_tb: &Bound<'_, PyAny>,
    ) -> bool {
        false
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

/// Build a [`Context`] from a JSON string.
///
/// Deserialises the JSON via `serde_json` into the engine's [`Value`] type,
/// then wraps it in a `Context`.  This avoids per-key Python→Rust FFI
/// crossings — the entire dict is transferred as a single string.
fn json_str_to_context(json_str: &str) -> PyResult<Context> {
    let value: prompt_templates::Value = serde_json::from_str(json_str)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("invalid JSON: {e}")))?;
    Context::from_value(value).map_err(|e| crate::errors::template_error_to_py(&e))
}

/// Create a `PyTemplate` from a file path (used by typegen).
pub(crate) fn load_template(path: &str) -> PyResult<PyTemplate> {
    PyTemplate::from_file(std::path::PathBuf::from(path))
}
