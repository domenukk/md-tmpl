#![forbid(unsafe_code)]
//! WebAssembly bindings for the `md-tmpl` engine.
//!
//! Exposes the core Rust template engine to JavaScript/TypeScript via
//! `wasm-bindgen`, providing:
//!
//! - [`Template`]: Parse and render `.tmpl.md` templates from WASM.

use std::cell::OnceCell;

use js_sys::{Array, Error, Object, Reflect};
use md_tmpl::{Context, Value};
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

    /// A JS object typed as `Record<string, unknown>` in TypeScript (for env vars).
    #[wasm_bindgen(typescript_type = "Record<string, unknown>")]
    pub type JsEnvRecord;
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

/// Build a JavaScript `Error` object from a [`TemplateError`](md_tmpl::TemplateError).
///
/// Unlike throwing a bare string primitive, the returned value is a real
/// `Error` instance (`instanceof Error` in JS). Its `name` is set to the
/// stable, machine-readable [`ErrorKind`](md_tmpl::ErrorKind) identifier
/// (e.g. `"missing_params"`), so JS callers can branch on `err.name` without
/// parsing the human-readable message. The same identifier is also exposed as
/// an own `kind` property, matching the pure-TS engine and Go binding.
fn js_error(e: &md_tmpl::TemplateError) -> JsValue {
    let err = Error::new(&e.to_string());
    err.set_name(e.kind().as_str());
    set_prop(&err, "kind", &JsValue::from_str(e.kind().as_str()));
    err.into()
}

/// Build a JavaScript `Error` object from a plain message string.
///
/// Used for error paths that do not originate from a
/// [`TemplateError`](md_tmpl::TemplateError) (e.g. malformed env records or
/// `serde` conversion failures), so those also throw real `Error` instances.
/// Its `kind` is the empty-string sentinel, matching the unknown/unclassified
/// kind used by the pure-TS engine and Go binding.
fn js_error_msg(msg: &str) -> JsValue {
    let err = Error::new(msg);
    set_prop(&err, "kind", &JsValue::from_str(""));
    err.into()
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
    inner: md_tmpl::Template,
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
    fn new(inner: md_tmpl::Template) -> Self {
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
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the source contains a syntax or parse error.
    #[wasm_bindgen(js_name = "fromSource")]
    pub fn from_source(source: &str) -> Result<Template, JsValue> {
        let inner = md_tmpl::Template::from_source(source).map_err(|e| js_error(&e))?;
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
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if parameter conversion fails, required parameters
    /// are missing, types mismatch, or rendering fails.
    pub fn render(&self, params: &JsParams) -> Result<String, JsValue> {
        let ctx = json_to_context(params.as_ref())?;
        self.inner.render_ctx(&ctx).map_err(|e| js_error(&e))
    }

    /// Render a template that takes no user-provided parameters.
    ///
    /// Required so the WASM `Template` satisfies the TypeScript `ITemplate`
    /// interface. Declared parameters that provide defaults use those defaults.
    ///
    /// Throws if any declared parameter lacks a default value, or if rendering
    /// fails.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if a declared parameter lacks a default value or
    /// rendering fails.
    #[wasm_bindgen(js_name = "renderEmpty")]
    pub fn render_empty(&self) -> Result<String, JsValue> {
        self.inner.render_empty().map_err(|e| js_error(&e))
    }

    /// Render **without any validation**.
    ///
    /// Skips *all* validation — missing parameters, type mismatches, and extra
    /// undeclared keys are ignored — matching the pure-TS `renderUnchecked()`
    /// API. A wrong-typed or missing parameter does not throw here; only a
    /// genuine rendering error (e.g. an undefined variable referenced by the
    /// body) is reported.
    ///
    /// Throws only if rendering itself fails.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if parameter conversion fails or rendering itself
    /// fails.
    #[wasm_bindgen(js_name = "renderUnchecked")]
    pub fn render_unchecked(&self, params: &JsParams) -> Result<String, JsValue> {
        let ctx = json_to_context(params.as_ref())?;
        self.inner
            .render_ctx_unchecked(&ctx)
            .map_err(|e| js_error(&e))
    }

    /// Render allowing extra (undeclared) parameters.
    ///
    /// Undeclared keys are silently ignored, but missing required parameters
    /// and type mismatches are still validated. This preserves the allow-extra
    /// capability previously exposed (misnamed) by `renderUnchecked`.
    ///
    /// Throws if required parameters are missing, types mismatch, or rendering
    /// fails.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if parameter conversion fails, required parameters
    /// are missing, types mismatch, or rendering fails.
    #[wasm_bindgen(js_name = "renderAllowingExtra")]
    pub fn render_allowing_extra(&self, params: &JsParams) -> Result<String, JsValue> {
        let ctx = json_to_context(params.as_ref())?;
        self.inner
            .render_ctx_allowing_extra(&ctx)
            .map_err(|e| js_error(&e))
    }

    /// Render from a pre-serialized JSON string.
    ///
    /// Accepts a JSON string (e.g. from `JSON.stringify(params)`) instead of
    /// a JS object, avoiding per-field WASM boundary crossings entirely.
    /// One string in, one string out.
    ///
    /// Throws if the JSON is invalid, required parameters are missing, or
    /// rendering fails.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the JSON string is malformed, required
    /// parameters are missing, types mismatch, or rendering fails.
    #[wasm_bindgen(js_name = "renderJson")]
    pub fn render_json(&self, json_str: &str) -> Result<String, JsValue> {
        let ctx = json_str_to_context(json_str)?;
        self.inner.render_ctx(&ctx).map_err(|e| js_error(&e))
    }

    /// Render from a pre-serialized JSON string **without any validation**.
    ///
    /// Like `renderJson`, but skips all validation (missing, type, extra),
    /// matching `renderUnchecked`.
    ///
    /// Throws if the JSON is invalid or rendering fails.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the JSON string is malformed or rendering
    /// fails.
    #[wasm_bindgen(js_name = "renderUncheckedJson")]
    pub fn render_unchecked_json(&self, json_str: &str) -> Result<String, JsValue> {
        let ctx = json_str_to_context(json_str)?;
        self.inner
            .render_ctx_unchecked(&ctx)
            .map_err(|e| js_error(&e))
    }

    /// Render from a pre-serialized JSON string, allowing extra (undeclared)
    /// parameters.
    ///
    /// Like `renderAllowingExtra`, but takes a JSON string. Undeclared keys are
    /// ignored while missing required parameters and types are still validated.
    ///
    /// Throws if required parameters are missing, types mismatch, the JSON is
    /// invalid, or rendering fails.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the JSON string is malformed, required
    /// parameters are missing, types mismatch, or rendering fails.
    #[wasm_bindgen(js_name = "renderAllowingExtraJson")]
    pub fn render_allowing_extra_json(&self, json_str: &str) -> Result<String, JsValue> {
        let ctx = json_str_to_context(json_str)?;
        self.inner
            .render_ctx_allowing_extra(&ctx)
            .map_err(|e| js_error(&e))
    }

    /// Render the template directly from a `FlexBuffers` binary buffer (`Uint8Array`).
    ///
    /// Bypasses all JSON serialization and JS object iteration overhead,
    /// achieving maximum performance across the WASM boundary.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the buffer is not valid `FlexBuffers`, required
    /// parameters are missing, types mismatch, or rendering fails.
    #[wasm_bindgen(js_name = "renderFlexbuffers")]
    pub fn render_flexbuffers(&self, buffer: &[u8]) -> Result<String, JsValue> {
        let ctx = md_tmpl::Context::from_flexbuffers(buffer).map_err(|e| js_error(&e))?;
        self.inner.render_ctx(&ctx).map_err(|e| js_error(&e))
    }

    /// Render from a `FlexBuffers` binary buffer **without any validation**.
    ///
    /// Skips all validation (missing, type, extra), matching `renderUnchecked`.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the buffer is not valid `FlexBuffers` or
    /// rendering fails.
    #[wasm_bindgen(js_name = "renderUncheckedFlexbuffers")]
    pub fn render_unchecked_flexbuffers(&self, buffer: &[u8]) -> Result<String, JsValue> {
        let ctx = md_tmpl::Context::from_flexbuffers(buffer).map_err(|e| js_error(&e))?;
        self.inner
            .render_ctx_unchecked(&ctx)
            .map_err(|e| js_error(&e))
    }

    /// Render from a `FlexBuffers` binary buffer, allowing extra (undeclared)
    /// parameters while still validating missing parameters and types.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the buffer is not valid `FlexBuffers`, required
    /// parameters are missing, types mismatch, or rendering fails.
    #[wasm_bindgen(js_name = "renderAllowingExtraFlexbuffers")]
    pub fn render_allowing_extra_flexbuffers(&self, buffer: &[u8]) -> Result<String, JsValue> {
        let ctx = md_tmpl::Context::from_flexbuffers(buffer).map_err(|e| js_error(&e))?;
        self.inner
            .render_ctx_allowing_extra(&ctx)
            .map_err(|e| js_error(&e))
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
    #[expect(
        clippy::cast_possible_truncation,
        reason = "intentional: wasm_bindgen cannot return u64, lower 32 bits suffice for cache keys"
    )]
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
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the source contains a syntax or parse error.
    #[wasm_bindgen(js_name = "fromSourceAllowingUnused")]
    pub fn from_source_allowing_unused(source: &str) -> Result<Template, JsValue> {
        let (inner, _fm) = md_tmpl::Template::compile(
            source,
            md_tmpl::CompileOptions::default().allow_unused(true),
        )
        .map_err(|e| js_error(&e))?;
        Ok(Self::new(inner))
    }

    /// Parse a template from source with a base directory for includes.
    ///
    /// The `base_dir` is used to resolve `{% include %}` and `{% import %}`
    /// directives relative to the template location.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the source contains a syntax or parse error.
    #[wasm_bindgen(js_name = "fromSourceWithBaseDir")]
    pub fn from_source_with_base_dir(source: &str, base_dir: &str) -> Result<Template, JsValue> {
        let (inner, _fm) = md_tmpl::Template::compile(
            source,
            md_tmpl::CompileOptions::default().base_dir(std::path::Path::new(base_dir)),
        )
        .map_err(|e| js_error(&e))?;
        Ok(Self::new(inner))
    }

    /// Parse a template from source with compile-time environment variables.
    ///
    /// The `env` object provides `key → value` pairs that are resolved at compile
    /// time against `env:` declarations in the frontmatter.
    ///
    /// Throws if the source has errors, required env vars are missing, or types mismatch.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the `env` object is malformed, required env
    /// vars are missing, types mismatch, or the source has parse errors.
    #[wasm_bindgen(js_name = "fromSourceWithEnv")]
    pub fn from_source_with_env(source: &str, env: &JsEnvRecord) -> Result<Template, JsValue> {
        let env_pairs = js_env_to_values(env.as_ref())?;
        let env_refs: Vec<(&str, md_tmpl::Value)> = env_pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();
        let (inner, _fm) =
            md_tmpl::Template::compile(source, md_tmpl::CompileOptions::default().env(&env_refs))
                .map_err(|e| js_error(&e))?;
        Ok(Self::new(inner))
    }

    /// Parse a template from source with full compile options.
    ///
    /// Accepts optional `base_dir`, `env`, and `allow_unused` parameters.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if the `env` object is malformed, required env
    /// vars are missing, types mismatch, or the source has parse errors.
    #[wasm_bindgen(js_name = "fromSourceWithOptions")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "wasm_bindgen requires owned types"
    )]
    pub fn from_source_with_options(
        source: &str,
        base_dir: Option<String>,
        env: Option<JsEnvRecord>,
        allow_unused: Option<bool>,
    ) -> Result<Template, JsValue> {
        let env_pairs = if let Some(ref env_val) = env {
            js_env_to_values(env_val.as_ref())?
        } else {
            Vec::new()
        };
        let env_refs: Vec<(&str, md_tmpl::Value)> = env_pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();
        let mut opts = md_tmpl::CompileOptions::default().env(&env_refs);
        if let Some(allow) = allow_unused {
            opts = opts.allow_unused(allow);
        }
        if let Some(ref dir) = base_dir {
            opts = opts.base_dir(std::path::Path::new(dir));
        }
        let (inner, _fm) = md_tmpl::Template::compile(source, opts).map_err(|e| js_error(&e))?;
        Ok(Self::new(inner))
    }

    /// Set the maximum include depth for rendering this template.
    #[wasm_bindgen(js_name = "setMaxIncludeDepth")]
    pub fn set_max_include_depth(&mut self, depth: usize) {
        self.inner.set_max_include_depth(depth);
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Convert a `usize` collection length/index to `u32` (JS array indices are u32).
///
/// A template's declaration count or list length can never approach `u32::MAX`
/// (~4.3 billion), so a failure here would indicate memory corruption or a
/// logic bug. We surface it loudly rather than silently truncating to a wrong
/// index/length.
fn u32_from_usize(n: usize) -> u32 {
    u32::try_from(n).expect("collection length exceeds u32::MAX — impossible for a template")
}

/// Convert an `f64` to `i64`, assuming the caller has verified `n` is a whole
/// number within `±2^53` (the JS safe-integer range).
#[expect(
    clippy::cast_possible_truncation,
    reason = "caller guarantees n is within ±2^53 and integral"
)]
fn safe_f64_to_i64(n: f64) -> i64 {
    n as i64
}

/// Extract `(key, value)` pairs from a JS `Record<string, unknown>` object.
fn js_env_to_values(val: &JsValue) -> Result<Vec<(String, md_tmpl::Value)>, JsValue> {
    if val.is_null() || val.is_undefined() {
        return Ok(Vec::new());
    }
    let obj: &Object = val
        .dyn_ref::<Object>()
        .ok_or_else(|| js_error_msg("env must be an object"))?;
    let keys = Object::keys(obj);
    let mut pairs = Vec::with_capacity(keys.length() as usize);
    for i in 0..keys.length() {
        let key = keys.get(i);
        let key_str = key
            .as_string()
            .ok_or_else(|| js_error_msg("env key must be a string"))?;
        let val = Reflect::get(obj, &key).map_err(|_| js_error_msg("failed to read env value"))?;
        let typed_val = js_to_env_value(&val)?;
        pairs.push((key_str, typed_val));
    }
    Ok(pairs)
}

/// Convert a JS value to a template Value for env.
fn js_to_env_value(val: &JsValue) -> Result<md_tmpl::Value, JsValue> {
    if val.is_string() {
        Ok(md_tmpl::Value::Str(val.as_string().unwrap()))
    } else if let Some(b) = val.as_bool() {
        Ok(md_tmpl::Value::Bool(b))
    } else if let Some(n) = val.as_f64() {
        // Check if it's an integer within the safe i64 range.
        // JS numbers are IEEE 754 f64, so integers beyond ±2^53 lose
        // precision. 2^53 is exactly representable as f64.
        const MAX_SAFE: f64 = 9_007_199_254_740_992.0; // 2^53
        const MIN_SAFE: f64 = -9_007_199_254_740_992.0; // -(2^53)
        if n.fract() == 0.0 && (MIN_SAFE..=MAX_SAFE).contains(&n) {
            Ok(md_tmpl::Value::Int(safe_f64_to_i64(n)))
        } else {
            Ok(md_tmpl::Value::Float(n))
        }
    } else if val.is_null() || val.is_undefined() {
        Ok(md_tmpl::Value::None)
    } else {
        // Complex objects (arrays, objects) — use serde.
        serde_wasm_bindgen::from_value(val.clone())
            .map_err(|e| js_error_msg(&format!("env value conversion error: {e}")))
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
        .map_err(|e| js_error_msg(&format!("serde_wasm_bindgen error: {e}")))?;
    Context::from_value(value).map_err(|e| js_error(&e))
}

/// Parse a JSON string into a [`Context`] via serde.
fn json_str_to_context(json_str: &str) -> Result<Context, JsValue> {
    let json_value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| js_error_msg(&e.to_string()))?;
    Context::from_serialize(&json_value).map_err(|e| js_error(&e))
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
///
/// JS numbers are IEEE 754 f64; integers beyond ±2^53 lose precision.
/// This matches the JS `Number` semantics that callers already expect.
#[expect(
    clippy::cast_precision_loss,
    reason = "JS numbers are f64; precision loss mirrors JS semantics"
)]
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
