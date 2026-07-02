//! C FFI bindings for the `md-tmpl` engine.
//!
//! This crate exposes the core template engine to C (and therefore Go, via
//! cgo). All types are opaque pointers; callers allocate and free handles
//! through the `pt_*` function family.
//!
//! # Thread safety
//!
//! All returned handles are `Send + Sync`-safe. The underlying `Template` and
//! `TemplateCache` types use `Arc` internally, so cloned handles are cheap.
//!
//! # Error handling
//!
//! Functions that can fail return a `*mut c_char` error string. A null return
//! means success. The caller owns the error string and must free it with
//! `pt_free_string`.

// FFI code uses match+unsafe patterns that are more readable than let...else
// with unsafe blocks. The JSON parser intentionally strips prefixes manually.
// Panic paths in FFI are only reachable via CString::new on controlled strings.
#![allow(
    clippy::manual_let_else,
    clippy::manual_strip,
    clippy::missing_panics_doc
)]

use std::{
    ffi::{CStr, CString, c_char},
    path::Path,
    ptr,
    sync::Arc,
};

use md_tmpl::{CompileOptions, Context, Template, TemplateCache, Value};

mod json;
use json::{json_to_value, parse_json_string_pairs};

// ---------------------------------------------------------------------------
// Opaque handles
// ---------------------------------------------------------------------------

/// Opaque handle to a compiled `Template`.
pub struct PtTemplate {
    inner: Template,
}

/// Opaque handle to a `TemplateCache`.
pub struct PtCache {
    inner: Arc<TemplateCache>,
}

/// Opaque handle to a rendering `Context`.
pub struct PtContext {
    inner: Context,
}

// ---------------------------------------------------------------------------
// Helper: C string → Rust string conversion
// ---------------------------------------------------------------------------

/// Convert a C string pointer to a Rust `&str`.
///
/// # Safety
///
/// The pointer must be non-null and point to a valid NUL-terminated UTF-8
/// string.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err("null pointer".to_string());
    }
    // SAFETY: caller guarantees non-null, NUL-terminated.
    let cstr = unsafe { CStr::from_ptr(ptr) };
    cstr.to_str().map_err(|e| format!("invalid UTF-8: {e}"))
}

/// Allocate a C error string. Returns null on success (no error).
fn err_to_cstring(msg: &str) -> *mut c_char {
    CString::new(msg)
        .unwrap_or_else(|_| CString::new("error message contained NUL byte").unwrap())
        .into_raw()
}

// ---------------------------------------------------------------------------
// String lifecycle
// ---------------------------------------------------------------------------

/// Free a string returned by any `pt_*` function.
///
/// # Safety
///
/// `ptr` must have been returned by a `pt_*` function and must not be freed
/// twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        // SAFETY: caller guarantees ptr came from CString::into_raw.
        drop(unsafe { CString::from_raw(ptr) });
    }
}

// ---------------------------------------------------------------------------
// Template lifecycle
// ---------------------------------------------------------------------------

/// Parse a template from an in-memory source string.
///
/// On success, writes the template handle to `*out` and returns null.
/// On failure, returns an error string (caller must free).
///
/// # Safety
///
/// - `source` must be a valid NUL-terminated UTF-8 string.
/// - `out` must be a valid pointer to a `*mut PtTemplate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_from_source(
    source: *const c_char,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    let source = match unsafe { cstr_to_str(source) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    match Template::from_source(source) {
        Ok(tmpl) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out = Box::into_raw(handle) };
            ptr::null_mut()
        }
        Err(e) => err_to_cstring(&e.to_string()),
    }
}

/// Parse a template, allowing unused declared parameters.
///
/// # Safety
///
/// Same as `pt_template_from_source`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_from_source_allowing_unused(
    source: *const c_char,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    let source = match unsafe { cstr_to_str(source) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    match Template::compile(source, CompileOptions::default().allow_unused(true)) {
        Ok((tmpl, _fm)) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out = Box::into_raw(handle) };
            ptr::null_mut()
        }
        Err(e) => err_to_cstring(&e.to_string()),
    }
}

/// Load a template from a file path.
///
/// # Safety
///
/// - `path` must be a valid NUL-terminated UTF-8 file path.
/// - `out` must be a valid pointer to a `*mut PtTemplate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_from_file(
    path: *const c_char,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    let path_str = match unsafe { cstr_to_str(path) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    match Template::from_file(Path::new(path_str)) {
        Ok(tmpl) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out = Box::into_raw(handle) };
            ptr::null_mut()
        }
        Err(e) => err_to_cstring(&e.to_string()),
    }
}

/// Free a template handle.
///
/// # Safety
///
/// `tmpl` must have been returned by a `pt_template_*` function and must not
/// be freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_free(tmpl: *mut PtTemplate) {
    if !tmpl.is_null() {
        drop(unsafe { Box::from_raw(tmpl) });
    }
}

// ---------------------------------------------------------------------------
// Context lifecycle
// ---------------------------------------------------------------------------

/// Create a new empty rendering context.
#[unsafe(no_mangle)]
pub extern "C" fn pt_context_new() -> *mut PtContext {
    Box::into_raw(Box::new(PtContext {
        inner: Context::new(),
    }))
}

/// Free a context handle.
///
/// # Safety
///
/// `ctx` must have been returned by `pt_context_new` and must not be freed
/// twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_free(ctx: *mut PtContext) {
    if !ctx.is_null() {
        drop(unsafe { Box::from_raw(ctx) });
    }
}

/// Set a string value in the context.
///
/// # Safety
///
/// - `ctx` must be a valid context handle.
/// - `key` and `value` must be valid NUL-terminated UTF-8 strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_set_str(
    ctx: *mut PtContext,
    key: *const c_char,
    value: *const c_char,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let key = match unsafe { cstr_to_str(key) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let value = match unsafe { cstr_to_str(value) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    ctx.inner.set(key, value);
    ptr::null_mut()
}

/// Set an integer value in the context.
///
/// # Safety
///
/// `ctx` must be a valid context handle. `key` must be a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_set_int(
    ctx: *mut PtContext,
    key: *const c_char,
    value: i64,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let key = match unsafe { cstr_to_str(key) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    ctx.inner.set(key, value);
    ptr::null_mut()
}

/// Set a float value in the context.
///
/// # Safety
///
/// `ctx` must be a valid context handle. `key` must be a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_set_float(
    ctx: *mut PtContext,
    key: *const c_char,
    value: f64,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let key = match unsafe { cstr_to_str(key) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    ctx.inner.set(key, value);
    ptr::null_mut()
}

/// Set a bool value in the context.
///
/// # Safety
///
/// `ctx` must be a valid context handle. `key` must be a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_set_bool(
    ctx: *mut PtContext,
    key: *const c_char,
    value: bool,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let key = match unsafe { cstr_to_str(key) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    ctx.inner.set(key, value);
    ptr::null_mut()
}

/// Set a None (absent/null) value in the context.
///
/// Use this for `option(T)` parameters to indicate an absent value.
///
/// # Safety
///
/// `ctx` must be a valid context handle. `key` must be a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_set_none(
    ctx: *mut PtContext,
    key: *const c_char,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let key = match unsafe { cstr_to_str(key) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    ctx.inner.set(key, Value::None);
    ptr::null_mut()
}

/// Set a JSON value in the context (for complex types: lists, dicts, enums).
///
/// The JSON string is deserialized into a template `Value` using serde.
///
/// # Safety
///
/// - `ctx` must be a valid context handle.
/// - `key` and `json` must be valid NUL-terminated UTF-8 strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_set_json(
    ctx: *mut PtContext,
    key: *const c_char,
    json: *const c_char,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let key = match unsafe { cstr_to_str(key) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let json_str = match unsafe { cstr_to_str(json) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    // Parse JSON into serde_json::Value, then convert to template Value.
    match json_to_value(json_str) {
        Ok(val) => {
            ctx.inner.set(key, val);
            ptr::null_mut()
        }
        Err(e) => err_to_cstring(&e),
    }
}

/// Set a template-typed parameter in the context.
///
/// The template is cloned (`Arc`-shared) into the context — the caller retains
/// ownership of the original.
///
/// # Safety
///
/// - `ctx` must be a valid context handle from [`pt_context_new`].
/// - `key` must be a valid NUL-terminated UTF-8 C string.
/// - `tmpl` must be a valid template handle from one of the `pt_template_from_*` functions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_set_tmpl(
    ctx: *mut PtContext,
    key: *const c_char,
    tmpl: *const PtTemplate,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let key = match unsafe { cstr_to_str(key) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return err_to_cstring("null template");
    };
    ctx.inner.set(key, Value::from(&tmpl.inner));
    ptr::null_mut()
}

/// Merge all top-level keys from a JSON object into a context.
///
/// The JSON string must be a JSON object (`{}`). Each key becomes a
/// context variable set to the corresponding JSON value.
///
/// Returns 0 on success, -1 on error (sets the error string retrievable
/// via the return value pattern used by other `pt_context_set_*` functions).
///
/// # Safety
///
/// - `ctx` must be a valid context handle from [`pt_context_new`].
/// - `json` must be a valid NUL-terminated UTF-8 C string containing a JSON object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_merge_json(
    ctx: *mut PtContext,
    json: *const c_char,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let json_str = match unsafe { cstr_to_str(json) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let obj = match json_to_value(json_str) {
        Ok(val) => val,
        Err(e) => return err_to_cstring(&e),
    };
    let Value::Struct(map) = obj else {
        return err_to_cstring("pt_context_merge_json: JSON must be an object");
    };
    for (key, val) in map.iter() {
        ctx.inner.set(key, val.clone());
    }
    ptr::null_mut()
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render a template with the given context (strict mode).
///
/// Returns the rendered output as a C string (caller must free with
/// `pt_free_string`). On error, writes the error string to `*out_err`
/// and returns null.
///
/// # Panics
///
/// Panics if the rendered output contains a NUL byte (should not happen in
/// practice with valid template output).
///
/// # Safety
///
/// - `tmpl` must be a valid template handle.
/// - `ctx` must be a valid context handle.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_render(
    tmpl: *const PtTemplate,
    ctx: *const PtContext,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        unsafe { *out_err = err_to_cstring("null template") };
        return ptr::null_mut();
    };
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        unsafe { *out_err = err_to_cstring("null context") };
        return ptr::null_mut();
    };
    match tmpl.inner.render_ctx(&ctx.inner) {
        Ok(rendered) => {
            unsafe { *out_err = ptr::null_mut() };
            CString::new(rendered)
                .unwrap_or_else(|_| CString::new("<output contained NUL byte>").unwrap())
                .into_raw()
        }
        Err(e) => {
            unsafe { *out_err = err_to_cstring(&e.to_string()) };
            ptr::null_mut()
        }
    }
}

/// Render a template, allowing extra (undeclared) parameters.
///
/// # Panics
///
/// Panics if the rendered output contains a NUL byte (should not happen in
/// practice with valid template output).
///
/// # Safety
///
/// Same as `pt_template_render`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_render_allowing_extra(
    tmpl: *const PtTemplate,
    ctx: *const PtContext,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        unsafe { *out_err = err_to_cstring("null template") };
        return ptr::null_mut();
    };
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        unsafe { *out_err = err_to_cstring("null context") };
        return ptr::null_mut();
    };
    match tmpl.inner.render_ctx_allowing_extra(&ctx.inner) {
        Ok(rendered) => {
            unsafe { *out_err = ptr::null_mut() };
            CString::new(rendered)
                .unwrap_or_else(|_| CString::new("<output contained NUL byte>").unwrap())
                .into_raw()
        }
        Err(e) => {
            unsafe { *out_err = err_to_cstring(&e.to_string()) };
            ptr::null_mut()
        }
    }
}

/// Render a template directly from a JSON object string, in a single FFI call.
///
/// Parses the JSON into a context and renders in one shot — avoids the
/// overhead of separate context creation, merge, and render calls.
///
/// When `allow_extra` is `true`, undeclared context keys are silently ignored.
///
/// Returns the rendered output as a C string (caller must free with
/// `pt_free_string`). On error, writes the error string to `*out_err`
/// and returns null.
///
/// # Panics
///
/// Panics if the rendered output contains a NUL byte (should not happen in
/// practice with valid template output).
///
/// # Safety
///
/// - `tmpl` must be a valid template handle.
/// - `json` must be a valid NUL-terminated UTF-8 C string containing a JSON object.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_render_json(
    tmpl: *const PtTemplate,
    json: *const c_char,
    allow_extra: bool,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        unsafe { *out_err = err_to_cstring("null template") };
        return ptr::null_mut();
    };
    let json_str = match unsafe { cstr_to_str(json) } {
        Ok(s) => s,
        Err(e) => {
            unsafe { *out_err = err_to_cstring(&e) };
            return ptr::null_mut();
        }
    };

    // Parse JSON into context values.
    let obj = match json_to_value(json_str) {
        Ok(val) => val,
        Err(e) => {
            unsafe { *out_err = err_to_cstring(&e) };
            return ptr::null_mut();
        }
    };
    let Value::Struct(map) = obj else {
        unsafe { *out_err = err_to_cstring("pt_template_render_json: JSON must be an object") };
        return ptr::null_mut();
    };

    // Populate context from parsed values.
    let mut ctx = Context::new();
    for (key, val) in map.iter() {
        ctx.set(key, val.clone());
    }

    // Render with the requested strictness.
    let result = if allow_extra {
        tmpl.inner.render_ctx_allowing_extra(&ctx)
    } else {
        tmpl.inner.render_ctx(&ctx)
    };
    match result {
        Ok(rendered) => {
            unsafe { *out_err = ptr::null_mut() };
            CString::new(rendered)
                .unwrap_or_else(|_| CString::new("<output contained NUL byte>").unwrap())
                .into_raw()
        }
        Err(e) => {
            unsafe { *out_err = err_to_cstring(&e.to_string()) };
            ptr::null_mut()
        }
    }
}

/// Set a `FlexBuffers` value in the context (for complex types: lists, dicts, enums, etc.).
///
/// # Safety
///
/// - `ctx` must be a valid context handle.
/// - `key` must be a valid NUL-terminated UTF-8 string.
/// - `data` must point to a valid buffer of `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_set_flexbuffers(
    ctx: *mut PtContext,
    key: *const c_char,
    data: *const u8,
    len: usize,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    let key = match unsafe { cstr_to_str(key) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    if data.is_null() {
        return err_to_cstring("pt_context_set_flexbuffers: null pointer passed");
    }
    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    match Value::from_flexbuffers(slice) {
        Ok(val) => {
            ctx.inner.set(key, val);
            ptr::null_mut()
        }
        Err(e) => err_to_cstring(&e.to_string()),
    }
}

/// Merge all top-level keys from a `FlexBuffers` map into a context.
///
/// # Safety
///
/// - `ctx` must be a valid context handle.
/// - `data` must point to a valid buffer of `len` bytes containing a `FlexBuffers` map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_merge_flexbuffers(
    ctx: *mut PtContext,
    data: *const u8,
    len: usize,
) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else {
        return err_to_cstring("null context");
    };
    if data.is_null() {
        return err_to_cstring("pt_context_merge_flexbuffers: null pointer passed");
    }
    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    let new_ctx = match Context::from_flexbuffers(slice) {
        Ok(c) => c,
        Err(e) => return err_to_cstring(&e.to_string()),
    };
    for (k, v) in new_ctx.into_inner() {
        ctx.inner.set(&k, v);
    }
    ptr::null_mut()
}

/// Render a template directly from a `FlexBuffers` map binary buffer, in a single FFI call.
///
/// # Panics
///
/// Panics if the rendered output contains a NUL byte.
///
/// # Safety
///
/// - `tmpl` must be a valid template handle.
/// - `data` must point to a valid buffer of `len` bytes containing a `FlexBuffers` map.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_render_flexbuffers(
    tmpl: *const PtTemplate,
    data: *const u8,
    len: usize,
    allow_extra: bool,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        unsafe { *out_err = err_to_cstring("null template") };
        return ptr::null_mut();
    };
    if data.is_null() {
        unsafe { *out_err = err_to_cstring("pt_template_render_flexbuffers: null pointer passed") };
        return ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    let ctx = match Context::from_flexbuffers(slice) {
        Ok(c) => c,
        Err(e) => {
            unsafe { *out_err = err_to_cstring(&e.to_string()) };
            return ptr::null_mut();
        }
    };

    let result = if allow_extra {
        tmpl.inner.render_ctx_allowing_extra(&ctx)
    } else {
        tmpl.inner.render_ctx(&ctx)
    };
    match result {
        Ok(rendered) => {
            unsafe { *out_err = ptr::null_mut() };
            CString::new(rendered)
                .unwrap_or_else(|_| CString::new("<output contained NUL byte>").unwrap())
                .into_raw()
        }
        Err(e) => {
            unsafe { *out_err = err_to_cstring(&e.to_string()) };
            ptr::null_mut()
        }
    }
}

// ---------------------------------------------------------------------------
// Template metadata
// ---------------------------------------------------------------------------

/// Return the source hash of a template.
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_source_hash(tmpl: *const PtTemplate) -> u64 {
    match unsafe { tmpl.as_ref() } {
        Some(t) => t.inner.source_hash(),
        None => 0,
    }
}

/// Return the template body (after frontmatter stripping).
///
/// The returned string must be freed with `pt_free_string`.
///
/// # Panics
///
/// Panics if `CString::new` fails on the body content (should not happen
/// unless the body contains interior NUL bytes).
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_body(tmpl: *const PtTemplate) -> *mut c_char {
    match unsafe { tmpl.as_ref() } {
        Some(t) => CString::new(t.inner.body())
            .unwrap_or_else(|_| CString::new("").unwrap())
            .into_raw(),
        None => CString::new("").unwrap().into_raw(),
    }
}

/// Return declarations as a JSON array of `[name, type]` pairs.
///
/// The returned string must be freed with `pt_free_string`.
///
/// # Panics
///
/// Panics if `CString::new` fails on the JSON output (should not happen
/// unless declarations contain interior NUL bytes).
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_declarations(tmpl: *const PtTemplate) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return CString::new("[]").unwrap().into_raw();
    };
    let decls: Vec<String> = tmpl
        .inner
        .declarations()
        .iter()
        .map(|d| format!("[\"{}\",\"{}\"]", d.name, d.var_type))
        .collect();
    let json = format!("[{}]", decls.join(","));
    CString::new(json).unwrap().into_raw()
}

/// Set the maximum include depth.
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_set_max_include_depth(tmpl: *mut PtTemplate, depth: usize) {
    if let Some(t) = unsafe { tmpl.as_mut() } {
        t.inner.set_max_include_depth(depth);
    }
}

// ---------------------------------------------------------------------------
// Defaults / Constants introspection
// ---------------------------------------------------------------------------

/// Convert a template `Value` to a JSON string.
fn value_to_json(val: &Value) -> String {
    match val {
        Value::Str(s) => {
            let escaped = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t");
            format!("\"{escaped}\"")
        }
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(value_to_json).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Struct(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{k}\": {}", value_to_json(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        Value::Tmpl(_) => "\"<template>\"".to_string(),
        Value::None => "null".to_string(),
    }
}

/// Returns the default values as a JSON object string.
///
/// Only parameters with defaults are included. The caller must free
/// the string with `pt_free_string`.
///
/// Returns `"{}"` if no defaults exist.
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_defaults_json(tmpl: *const PtTemplate) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return CString::new("{}").unwrap().into_raw();
    };
    let defaults = tmpl.inner.defaults();
    if defaults.is_empty() {
        return CString::new("{}").unwrap().into_raw();
    }
    let pairs: Vec<String> = defaults
        .iter()
        .map(|(k, v)| format!("\"{k}\": {}", value_to_json(v)))
        .collect();
    let json = format!("{{{}}}", pairs.join(", "));
    CString::new(json).unwrap().into_raw()
}

/// Returns the constants as a JSON object string.
///
/// Only template-level constants are included (not imported ones).
/// The caller must free the string with `pt_free_string`.
///
/// Returns `"{}"` if no constants exist.
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_consts_json(tmpl: *const PtTemplate) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return CString::new("{}").unwrap().into_raw();
    };
    let consts = tmpl.inner.consts();
    if consts.is_empty() {
        return CString::new("{}").unwrap().into_raw();
    }
    let pairs: Vec<String> = consts
        .iter()
        .map(|(k, v)| format!("\"{k}\": {}", value_to_json(v)))
        .collect();
    let json = format!("{{{}}}", pairs.join(", "));
    CString::new(json).unwrap().into_raw()
}

/// Create a context pre-filled with all default parameter values.
///
/// Returns a new context handle that the caller must free with
/// `pt_context_free`. If no defaults exist, returns an empty context.
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_defaults_context(tmpl: *const PtTemplate) -> *mut PtContext {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return Box::into_raw(Box::new(PtContext {
            inner: Context::new(),
        }));
    };
    Box::into_raw(Box::new(PtContext {
        inner: tmpl.inner.defaults_context(),
    }))
}

/// Parse a template from source with a base directory for resolving includes.
///
/// Like `pt_template_from_source`, but includes are resolved relative to
/// `base_dir` instead of the current directory.
///
/// # Safety
///
/// - `source` must be a valid NUL-terminated UTF-8 C string.
/// - `base_dir` must be a valid NUL-terminated UTF-8 file path.
/// - `out` must be a valid pointer to a `*mut PtTemplate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_from_source_with_base_dir(
    source: *const c_char,
    base_dir: *const c_char,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    let src = match unsafe { cstr_to_str(source) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let dir = match unsafe { cstr_to_str(base_dir) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    match Template::compile(
        src,
        CompileOptions::default().base_dir(std::path::Path::new(dir)),
    ) {
        Ok((tmpl, _fm)) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out = Box::into_raw(handle) };
            ptr::null_mut()
        }
        Err(e) => err_to_cstring(&e.to_string()),
    }
}

/// Parse a template from source and return frontmatter metadata separately.
///
/// On success:
/// - Writes the template handle to `*out_tmpl`
/// - Writes frontmatter JSON string to `*out_fm` (caller must free with `pt_free_string`)
/// - Returns null (no error)
///
/// On failure:
/// - Returns an error string (caller must free with `pt_free_string`)
///
/// The frontmatter JSON contains:
/// ```json
/// {
///   "name": "greeting",
///   "description": "A greeting template",
///   "has_params": true,
///   "allow_unused": false,
///   "params": ["name", "count"]
/// }
/// ```
///
/// # Safety
///
/// - `source` must be a valid NUL-terminated UTF-8 C string.
/// - `out_tmpl` must be a valid pointer to a `*mut PtTemplate`.
/// - `out_fm` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_from_source_with_frontmatter(
    source: *const c_char,
    out_tmpl: *mut *mut PtTemplate,
    out_fm: *mut *mut c_char,
) -> *mut c_char {
    let src = match unsafe { cstr_to_str(source) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    match Template::compile(src, CompileOptions::default()) {
        Ok((tmpl, fm)) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out_tmpl = Box::into_raw(handle) };

            let name_escaped = fm
                .name
                .unwrap_or_default()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            let desc_escaped = fm
                .description
                .unwrap_or_default()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            let params: Vec<String> = fm.params.iter().map(|p| format!("\"{p}\"")).collect();

            let json = format!(
                "{{\"name\":\"{name_escaped}\",\"description\":\"{desc_escaped}\",\"has_params\":{},\"allow_unused\":{},\"params\":[{}]}}",
                fm.has_params,
                fm.allow_unused,
                params.join(",")
            );
            unsafe {
                *out_fm = CString::new(json).unwrap().into_raw();
            }
            ptr::null_mut()
        }
        Err(e) => {
            unsafe { *out_tmpl = ptr::null_mut() };
            err_to_cstring(&e.to_string())
        }
    }
}

/// Validate that a template's declarations match an expected set.
///
/// `expected_json` must be a JSON array of `["name", "type"]` pairs, in the
/// same format returned by `pt_template_declarations`.
///
/// Returns null on success (declarations match), or an error string describing
/// the differences.
///
/// # Safety
///
/// - `tmpl` must be a valid template handle.
/// - `expected_json` must be a valid NUL-terminated UTF-8 JSON string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_validate_declarations(
    tmpl: *const PtTemplate,
    expected_json: *const c_char,
) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return err_to_cstring("null template");
    };
    let json_str = match unsafe { cstr_to_str(expected_json) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };

    // Parse the expected JSON array of [name, type] pairs.
    let parsed: Vec<Vec<String>> = match parse_json_string_pairs(json_str) {
        Ok(v) => v,
        Err(e) => return err_to_cstring(&e),
    };

    // Compare against current declarations using string representations,
    // since parse_type_annotation is pub(crate) and not accessible from FFI.
    let current: Vec<(String, String)> = tmpl
        .inner
        .declarations()
        .iter()
        .map(|d| (d.name.clone(), d.var_type.to_string()))
        .collect();

    let expected_pairs: Vec<(String, String)> = parsed
        .iter()
        .filter_map(|pair| {
            if pair.len() == 2 {
                Some((pair[0].clone(), pair[1].clone()))
            } else {
                None
            }
        })
        .collect();

    if current == expected_pairs {
        return ptr::null_mut();
    }

    // Build a human-readable diff.
    let current_names: std::collections::HashSet<&str> =
        current.iter().map(|(n, _)| n.as_str()).collect();
    let expected_names: std::collections::HashSet<&str> =
        expected_pairs.iter().map(|(n, _)| n.as_str()).collect();

    let mut parts = Vec::new();

    let missing: Vec<&str> = expected_names.difference(&current_names).copied().collect();
    if !missing.is_empty() {
        parts.push(format!("removed: {}", missing.join(", ")));
    }

    let extra: Vec<&str> = current_names.difference(&expected_names).copied().collect();
    if !extra.is_empty() {
        parts.push(format!("added: {}", extra.join(", ")));
    }

    let current_map: std::collections::HashMap<&str, &str> = current
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let expected_map: std::collections::HashMap<&str, &str> = expected_pairs
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let retyped: Vec<String> = current_names
        .intersection(&expected_names)
        .filter_map(|name| {
            let cur = current_map[name];
            let exp = expected_map[name];
            if cur == exp {
                None
            } else {
                Some(format!("{name}: {exp} → {cur}"))
            }
        })
        .collect();
    if !retyped.is_empty() {
        parts.push(format!("retyped: {}", retyped.join(", ")));
    }

    err_to_cstring(&format!("declarations changed: {}", parts.join("; ")))
}

/// Returns the imported constants as a JSON object string.
///
/// These are constants imported from other templates via `imports:` directives,
/// keyed by `stem.NAME` (e.g. `other.MAX_RETRIES`).
///
/// The caller must free the string with `pt_free_string`.
/// Returns `"{}"` if no imported constants exist.
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_imported_consts_json(tmpl: *const PtTemplate) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return CString::new("{}").unwrap().into_raw();
    };
    let imported = tmpl.inner.imported_consts();
    if imported.is_empty() {
        return CString::new("{}").unwrap().into_raw();
    }
    let pairs: Vec<String> = imported
        .iter()
        .map(|(k, v)| format!("\"{k}\": {}", value_to_json(v)))
        .collect();
    let json = format!("{{{}}}", pairs.join(", "));
    CString::new(json).unwrap().into_raw()
}

// ---------------------------------------------------------------------------
// Cache lifecycle
// ---------------------------------------------------------------------------

/// Create a new template cache.
#[unsafe(no_mangle)]
pub extern "C" fn pt_cache_new() -> *mut PtCache {
    Box::into_raw(Box::new(PtCache {
        inner: Arc::new(TemplateCache::new()),
    }))
}

/// Free a cache handle.
///
/// # Safety
///
/// `cache` must have been returned by `pt_cache_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_free(cache: *mut PtCache) {
    if !cache.is_null() {
        drop(unsafe { Box::from_raw(cache) });
    }
}

/// Load a template through the cache.
///
/// # Safety
///
/// - `cache` must be a valid cache handle.
/// - `path` must be a valid NUL-terminated UTF-8 file path.
/// - `out` must be a valid pointer to a `*mut PtTemplate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_load(
    cache: *const PtCache,
    path: *const c_char,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    let Some(cache) = (unsafe { cache.as_ref() }) else {
        return err_to_cstring("null cache");
    };
    let path_str = match unsafe { cstr_to_str(path) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    match cache.inner.load(Path::new(path_str)) {
        Ok(tmpl) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out = Box::into_raw(handle) };
            ptr::null_mut()
        }
        Err(e) => err_to_cstring(&e.to_string()),
    }
}

/// Clear all entries from the cache.
///
/// # Safety
///
/// `cache` must be a valid cache handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_clear(cache: *const PtCache) {
    if let Some(c) = unsafe { cache.as_ref() } {
        c.inner.clear();
    }
}

/// Return the number of cached templates.
///
/// # Safety
///
/// `cache` must be a valid cache handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_template_count(cache: *const PtCache) -> usize {
    match unsafe { cache.as_ref() } {
        Some(c) => c.inner.template_count(),
        None => 0,
    }
}

/// Return the number of cached includes.
///
/// # Safety
///
/// `cache` must be a valid cache handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_include_count(cache: *const PtCache) -> usize {
    match unsafe { cache.as_ref() } {
        Some(c) => c.inner.include_count(),
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
