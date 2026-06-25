#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]
//! WebAssembly bindings for the `prompt-templates` engine.
//!
//! Exposes the core Rust template engine to JavaScript/TypeScript via
//! `wasm-bindgen`, providing:
//!
//! - [`Template`]: Parse and render `.tmpl.md` templates from WASM.

use std::cell::OnceCell;

use js_sys::{Array, Object, Reflect};
use prompt_templates::{Context, Value};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// A JS object typed as `Record<string, unknown>` in TypeScript.
    #[wasm_bindgen(typescript_type = "Record<string, unknown>")]
    pub type JsParams;

    /// A JS value typed as `[string, string][]` in TypeScript.
    #[wasm_bindgen(typescript_type = "[string, string][]")]
    pub type JsDeclarations;

    /// A JS value typed as `Record<string, unknown>` in TypeScript (for return).
    #[wasm_bindgen(typescript_type = "Record<string, unknown>")]
    pub type JsRecord;
}

/// Set a property on a JS object, panicking on failure.
///
/// `Reflect::set` can only fail on frozen/sealed objects or rejecting Proxies.
/// All call sites use freshly created `Object::new()`, so this is infallible.
#[inline]
fn set_prop(obj: &Object, key: &str, value: &JsValue) {
    Reflect::set(obj, &JsValue::from_str(key), value)
        .expect("Reflect::set on a fresh Object cannot fail");
}

// ---------------------------------------------------------------------------
// Template wrapper
// ---------------------------------------------------------------------------

/// A compiled prompt template, usable from JavaScript.
///
/// Create with `Template.fromSource(source)`, then call `.render(params)`.
///
/// # Performance
///
/// Metadata accessors (`consts()`, `declarations()`, `defaults()`,
/// `importedConsts()`) cache their JS representations on first call.
/// Render methods use JSON bulk serialization to minimize WASM boundary
/// crossings.
#[wasm_bindgen]
pub struct Template {
    inner: prompt_templates::Template,
    /// Cached JS representation of `consts()`.
    cached_consts: OnceCell<JsValue>,
    /// Cached JS representation of `declarations()`.
    cached_declarations: OnceCell<JsValue>,
    /// Cached JS representation of `defaults()`.
    cached_defaults: OnceCell<JsValue>,
    /// Cached JS representation of `importedConsts()`.
    cached_imported_consts: OnceCell<JsValue>,
}

impl Template {
    /// Wrap a compiled template, initializing all caches as empty.
    fn new(inner: prompt_templates::Template) -> Self {
        Self {
            inner,
            cached_consts: OnceCell::new(),
            cached_declarations: OnceCell::new(),
            cached_defaults: OnceCell::new(),
            cached_imported_consts: OnceCell::new(),
        }
    }
}

#[wasm_bindgen]
impl Template {
    /// Parse a template from source text.
    ///
    /// Throws a JavaScript error if the source contains a syntax error.
    #[wasm_bindgen(js_name = "fromSource")]
    pub fn from_source(source: &str) -> Result<Template, JsValue> {
        let inner = prompt_templates::Template::from_source(source)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self::new(inner))
    }

    /// Render the template with the given parameters object.
    ///
    /// The `params` argument should be a plain JS object whose keys correspond
    /// to the template's declared parameters. Internally, this uses JSON bulk
    /// serialization to minimize WASM boundary crossings.
    ///
    /// Throws if required parameters are missing, types mismatch, or rendering
    /// fails.
    pub fn render(&self, params: &JsParams) -> Result<String, JsValue> {
        let ctx = json_to_context(params.as_ref())?;
        self.inner
            .render_ctx(&ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Render without strict parameter validation (allows extra params,
    /// skips type checks). Matches the pure-TS `renderUnchecked()` API.
    #[wasm_bindgen(js_name = "renderUnchecked")]
    pub fn render_unchecked(&self, params: &JsParams) -> Result<String, JsValue> {
        let ctx = json_to_context(params.as_ref())?;
        self.inner
            .render_ctx_allowing_extra(&ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Render from a pre-serialized JSON string.
    ///
    /// Accepts a JSON string (e.g. from `JSON.stringify(params)`) instead of
    /// a JS object, avoiding per-field WASM boundary crossings entirely.
    /// One string in, one string out.
    ///
    /// Throws if the JSON is invalid, required parameters are missing, or
    /// rendering fails.
    #[wasm_bindgen(js_name = "renderJson")]
    pub fn render_json(&self, json_str: &str) -> Result<String, JsValue> {
        let ctx = json_str_to_context(json_str)?;
        self.inner
            .render_ctx(&ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Render from a pre-serialized JSON string without strict validation.
    ///
    /// Like `renderJson`, but allows extra (undeclared) parameters.
    ///
    /// Throws if the JSON is invalid or rendering fails.
    #[wasm_bindgen(js_name = "renderUncheckedJson")]
    pub fn render_unchecked_json(&self, json_str: &str) -> Result<String, JsValue> {
        let ctx = json_str_to_context(json_str)?;
        self.inner
            .render_ctx_allowing_extra(&ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Render the template directly from a `FlexBuffers` binary buffer (`Uint8Array`).
    ///
    /// Bypasses all JSON serialization and JS object iteration overhead,
    /// achieving maximum performance across the WASM boundary.
    #[wasm_bindgen(js_name = "renderFlexbuffers")]
    pub fn render_flexbuffers(&self, buffer: &[u8]) -> Result<String, JsValue> {
        let ctx = prompt_templates::Context::from_flexbuffers(buffer)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner
            .render_ctx(&ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Render from a `FlexBuffers` binary buffer without strict validation (allows extra params).
    #[wasm_bindgen(js_name = "renderUncheckedFlexbuffers")]
    pub fn render_unchecked_flexbuffers(&self, buffer: &[u8]) -> Result<String, JsValue> {
        let ctx = prompt_templates::Context::from_flexbuffers(buffer)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner
            .render_ctx_allowing_extra(&ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Return the raw template body text (after frontmatter stripping).
    #[must_use]
    pub fn body(&self) -> String {
        self.inner.body().to_owned()
    }

    /// Return default values for all parameters that declare them.
    ///
    /// Returns a plain JS object. The result is cached after the first call.
    #[must_use]
    pub fn defaults(&self) -> JsRecord {
        self.cached_defaults
            .get_or_init(|| {
                let defaults = self.inner.defaults();
                let obj = Object::new();
                for (key, val) in &defaults {
                    set_prop(&obj, key, &value_to_js(val));
                }
                JsValue::from(obj)
            })
            .clone()
            .unchecked_into()
    }

    /// Return the constants defined in this template's frontmatter.
    ///
    /// Returns a plain JS object. The result is cached after the first call.
    #[must_use]
    pub fn consts(&self) -> JsRecord {
        self.cached_consts
            .get_or_init(|| {
                let consts = self.inner.consts();
                let obj = Object::new();
                for (key, val) in consts.iter() {
                    set_prop(&obj, key, &value_to_js(val));
                }
                JsValue::from(obj)
            })
            .clone()
            .unchecked_into()
    }

    /// Return the parameter declarations as an array of `[name, typeString]`
    /// tuples.
    ///
    /// The result is cached after the first call.
    #[must_use]
    pub fn declarations(&self) -> JsDeclarations {
        self.cached_declarations
            .get_or_init(|| {
                let decls = self.inner.declarations();
                let arr = Array::new_with_length(u32_from_usize(decls.len()));
                for (i, decl) in decls.iter().enumerate() {
                    let tuple = Array::new_with_length(2);
                    tuple.set(0, JsValue::from_str(&decl.name));
                    tuple.set(1, JsValue::from_str(&decl.var_type.to_string()));
                    arr.set(u32_from_usize(i), tuple.into());
                }
                JsValue::from(arr)
            })
            .clone()
            .unchecked_into()
    }

    /// Content hash of the original source text (truncated to u32 for JS).
    ///
    /// Same source → same hash. Useful for cache invalidation.
    #[must_use]
    #[wasm_bindgen(js_name = "sourceHash")]
    #[allow(clippy::cast_possible_truncation)]
    pub fn source_hash(&self) -> u32 {
        self.inner.source_hash() as u32
    }

    /// Return the imported constants from `{% import %}` directives.
    ///
    /// Returns a plain JS object. The result is cached after the first call.
    #[must_use]
    #[wasm_bindgen(js_name = "importedConsts")]
    pub fn imported_consts(&self) -> JsRecord {
        self.cached_imported_consts
            .get_or_init(|| {
                let consts = self.inner.imported_consts();
                let obj = Object::new();
                for (key, val) in consts.iter() {
                    set_prop(&obj, key, &value_to_js(val));
                }
                JsValue::from(obj)
            })
            .clone()
            .unchecked_into()
    }

    /// Parse a template from source, allowing unused parameters.
    ///
    /// Like `fromSource`, but does not error when parameters are declared
    /// but never referenced in the body.
    #[wasm_bindgen(js_name = "fromSourceAllowingUnused")]
    pub fn from_source_allowing_unused(source: &str) -> Result<Template, JsValue> {
        let (inner, _fm) = prompt_templates::Template::compile(
            source,
            prompt_templates::CompileOptions::default().allow_unused(true),
        )
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self::new(inner))
    }

    /// Parse a template from source with a base directory for includes.
    ///
    /// The `base_dir` is used to resolve `{% include %}` and `{% import %}`
    /// directives relative to the template location.
    #[wasm_bindgen(js_name = "fromSourceWithBaseDir")]
    pub fn from_source_with_base_dir(source: &str, base_dir: &str) -> Result<Template, JsValue> {
        let (inner, _fm) = prompt_templates::Template::compile(
            source,
            prompt_templates::CompileOptions::default().base_dir(std::path::Path::new(base_dir)),
        )
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self::new(inner))
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Saturating conversion from `usize` to `u32` (JS array indices are u32).
#[allow(clippy::cast_possible_truncation)]
fn u32_from_usize(n: usize) -> u32 {
    if n > u32::MAX as usize {
        u32::MAX
    } else {
        n as u32
    }
}

// ---------------------------------------------------------------------------
// JS → Rust conversion helpers (JSON bulk serialization path)
// ---------------------------------------------------------------------------

/// Convert a JS value to a [`Context`] directly via `serde_wasm_bindgen`.
///
/// Deserializes the JS object directly into a `Value` and builds the `Context`,
/// avoiding `JSON.stringify` string allocation and JSON parsing overhead entirely.
fn json_to_context(val: &JsValue) -> Result<Context, JsValue> {
    if val.is_null() || val.is_undefined() {
        return Ok(Context::new());
    }
    if !val.is_object() {
        return Ok(Context::new());
    }
    let value: Value = serde_wasm_bindgen::from_value(val.clone())
        .map_err(|e| JsValue::from_str(&format!("serde_wasm_bindgen error: {e}")))?;
    Context::from_value(value).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Parse a JSON string into a [`Context`] via serde.
fn json_str_to_context(json_str: &str) -> Result<Context, JsValue> {
    let json_value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| JsValue::from_str(&e.to_string()))?;
    Context::from_serialize(&json_value).map_err(|e| JsValue::from_str(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Rust → JS conversion helpers
// ---------------------------------------------------------------------------

/// Convert a [`Value`] into a `JsValue` for returning to JavaScript.
fn value_to_js(val: &Value) -> JsValue {
    match val {
        Value::Str(s) => JsValue::from_str(s),
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Int(i) => int_to_js(*i),
        Value::Float(f) => JsValue::from_f64(*f),
        Value::List(items) => list_to_js(items),
        Value::Struct(map) => {
            let obj = Object::new();
            for (key, v) in map.iter() {
                set_prop(&obj, key, &value_to_js(v));
            }
            obj.into()
        }
        Value::Tmpl(_) | Value::None => JsValue::NULL,
    }
}

/// Convert an `i64` into a JS number.
#[allow(clippy::cast_precision_loss)]
fn int_to_js(i: i64) -> JsValue {
    JsValue::from_f64(i as f64)
}

/// Convert a `Value::List` into a JS `Array`.
fn list_to_js(items: &[Value]) -> JsValue {
    let arr = Array::new_with_length(u32_from_usize(items.len()));
    for (i, item) in items.iter().enumerate() {
        arr.set(u32_from_usize(i), value_to_js(item));
    }
    arr.into()
}
