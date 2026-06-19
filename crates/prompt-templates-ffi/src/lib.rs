//! C FFI bindings for the `prompt-templates` engine.
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

use prompt_templates::{CompileOptions, Context, Template, TemplateCache, Value};

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

/// Convert a JSON string to a template `Value`.
fn json_to_value(json: &str) -> Result<Value, String> {
    // Simple recursive JSON parser — avoids pulling in serde_json as a dependency.
    // We leverage the serde feature to deserialize.
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Err("empty JSON string".to_string());
    }
    parse_json_value(trimmed).map(|(val, _)| val)
}

/// Recursive JSON parser that returns (Value, `remaining_str`).
fn parse_json_value(s: &str) -> Result<(Value, &str), String> {
    let s = s.trim_start();
    if s.is_empty() {
        return Err("unexpected end of JSON".to_string());
    }

    match s.as_bytes()[0] {
        b'"' => parse_json_string(s),
        b'{' => parse_json_object(s),
        b'[' => parse_json_array(s),
        b't' | b'f' => parse_json_bool(s),
        b'n' => parse_json_null(s),
        _ => parse_json_number(s),
    }
}

fn parse_json_string(s: &str) -> Result<(Value, &str), String> {
    debug_assert!(s.starts_with('"'));
    let s = &s[1..]; // skip opening quote
    let mut result = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                return Ok((Value::Str(result), &s[i + 1..]));
            }
            '\\' => {
                if let Some((_, escaped)) = chars.next() {
                    match escaped {
                        '"' | '\\' | '/' => result.push(escaped),
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        'b' => result.push('\u{08}'),
                        'f' => result.push('\u{0C}'),
                        'u' => {
                            // Parse 4 hex digits
                            let mut hex = String::with_capacity(4);
                            for _ in 0..4 {
                                if let Some((_, h)) = chars.next() {
                                    hex.push(h);
                                } else {
                                    return Err("incomplete unicode escape".to_string());
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16)
                                .map_err(|e| format!("invalid unicode escape: {e}"))?;
                            let ch = char::from_u32(code)
                                .ok_or_else(|| format!("invalid unicode code point: {code}"))?;
                            result.push(ch);
                        }
                        _ => {
                            result.push('\\');
                            result.push(escaped);
                        }
                    }
                }
            }
            _ => result.push(c),
        }
    }
    Err("unterminated string".to_string())
}

fn parse_json_object(s: &str) -> Result<(Value, &str), String> {
    debug_assert!(s.starts_with('{'));
    let mut s = s[1..].trim_start();
    let mut map = std::collections::HashMap::new();

    if let Some(rest) = s.strip_prefix('}') {
        return Ok((Value::Struct(Arc::new(map.into_iter().collect())), rest));
    }

    loop {
        // Parse key
        if !s.starts_with('"') {
            return Err(format!(
                "expected string key, got: {}",
                &s[..s.len().min(20)]
            ));
        }
        let (key_val, rest) = parse_json_string(s)?;
        let Value::Str(key) = key_val else {
            unreachable!()
        };
        s = rest.trim_start();

        // Expect colon
        if !s.starts_with(':') {
            return Err("expected ':' after key".to_string());
        }
        s = s[1..].trim_start();

        // Parse value
        let (val, rest) = parse_json_value(s)?;
        map.insert(key, val);
        s = rest.trim_start();

        if s.starts_with('}') {
            s = &s[1..];
            break;
        }
        if s.starts_with(',') {
            s = s[1..].trim_start();
        } else {
            return Err("expected ',' or '}' in object".to_string());
        }
    }

    Ok((Value::Struct(Arc::new(map.into_iter().collect())), s))
}

fn parse_json_array(s: &str) -> Result<(Value, &str), String> {
    debug_assert!(s.starts_with('['));
    let mut s = s[1..].trim_start();
    let mut items = Vec::new();

    if let Some(rest) = s.strip_prefix(']') {
        return Ok((Value::List(Arc::new(items)), rest));
    }

    loop {
        let (val, rest) = parse_json_value(s)?;
        items.push(val);
        s = rest.trim_start();

        if s.starts_with(']') {
            s = &s[1..];
            break;
        }
        if s.starts_with(',') {
            s = s[1..].trim_start();
        } else {
            return Err("expected ',' or ']' in array".to_string());
        }
    }

    Ok((Value::List(Arc::new(items)), s))
}

fn parse_json_bool(s: &str) -> Result<(Value, &str), String> {
    if let Some(rest) = s.strip_prefix("true") {
        Ok((Value::Bool(true), rest))
    } else if let Some(rest) = s.strip_prefix("false") {
        Ok((Value::Bool(false), rest))
    } else {
        Err(format!("unexpected token: {}", &s[..s.len().min(10)]))
    }
}

fn parse_json_null(s: &str) -> Result<(Value, &str), String> {
    if let Some(rest) = s.strip_prefix("null") {
        // Map JSON null to the template engine's `None` variant, used by
        // `option<T>` types (desugared to `enum<Some(val=T), None>`).
        Ok((
            Value::Str(prompt_templates::consts::OPTION_NONE.to_string()),
            rest,
        ))
    } else {
        Err(format!("unexpected token: {}", &s[..s.len().min(10)]))
    }
}

fn parse_json_number(s: &str) -> Result<(Value, &str), String> {
    let end = s
        .find(|c: char| {
            !c.is_ascii_digit() && c != '-' && c != '+' && c != '.' && c != 'e' && c != 'E'
        })
        .unwrap_or(s.len());
    let num_str = &s[..end];

    if num_str.contains('.') || num_str.contains('e') || num_str.contains('E') {
        let f: f64 = num_str
            .parse()
            .map_err(|e| format!("invalid float '{num_str}': {e}"))?;
        Ok((Value::Float(f), &s[end..]))
    } else {
        let i: i64 = num_str
            .parse()
            .map_err(|e| format!("invalid integer '{num_str}': {e}"))?;
        Ok((Value::Int(i), &s[end..]))
    }
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
    match tmpl.inner.render(&ctx.inner) {
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
    match tmpl.inner.render_with(
        &ctx.inner,
        prompt_templates::RenderOptions::default().allow_extra(true),
    ) {
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
    let result = tmpl.inner.render_with(
        &ctx,
        prompt_templates::RenderOptions::default().allow_extra(allow_extra),
    );
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

    let result = tmpl.inner.render_with(
        &ctx,
        prompt_templates::RenderOptions::default().allow_extra(allow_extra),
    );
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

            let name_escaped = fm.name.replace('\\', "\\\\").replace('"', "\\\"");
            let desc_escaped = fm.description.replace('\\', "\\\\").replace('"', "\\\"");
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
    let Some(_tmpl) = (unsafe { tmpl.as_ref() }) else {
        return CString::new("{}").unwrap().into_raw();
    };
    // imported_consts lives on the Frontmatter, which Template doesn't
    // expose directly yet. For templates loaded via from_source,
    // imported_consts is always empty anyway. When the core API adds
    // an imported_consts() accessor, this stub can be completed.
    CString::new("{}").unwrap().into_raw()
}

/// Parse a JSON string of `[["name", "type"], ...]` pairs.
fn parse_json_string_pairs(json: &str) -> Result<Vec<Vec<String>>, String> {
    let trimmed = json.trim();
    if !trimmed.starts_with('[') {
        return Err("expected JSON array".to_string());
    }

    let (val, _) = parse_json_array(trimmed)?;
    let Value::List(items) = val else {
        return Err("expected JSON array".to_string());
    };

    let mut result = Vec::new();
    for item in items.iter() {
        let Value::List(pair) = item else {
            return Err("expected [name, type] pair".to_string());
        };
        let mut strings = Vec::new();
        for elem in pair.iter() {
            let Value::Str(s) = elem else {
                return Err("expected string in pair".to_string());
            };
            strings.push(s.clone());
        }
        result.push(strings);
    }
    Ok(result)
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
mod tests {
    use super::*;

    #[test]
    fn test_from_source_and_render() {
        let source = CString::new("---\nparams:\n  - name = str\n---\nHello {{ name }}!").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null(), "expected no error");
        assert!(!tmpl.is_null());

        let ctx = pt_context_new();
        let key = CString::new("name").unwrap();
        let val = CString::new("world").unwrap();
        let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
        assert!(err.is_null());

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null(), "expected no render error");
        assert!(!result.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "Hello world!");

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_from_source_syntax_error() {
        let source = CString::new("no frontmatter").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(!err.is_null(), "expected syntax error");
        assert!(tmpl.is_null());
        unsafe { pt_free_string(err) };
    }

    #[test]
    fn test_context_set_int() {
        let source = CString::new("---\nparams: [count = int]\n---\nCount: {{ count }}").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let ctx = pt_context_new();
        let key = CString::new("count").unwrap();
        let err = unsafe { pt_context_set_int(ctx, key.as_ptr(), 42) };
        assert!(err.is_null());

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "Count: 42");

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_context_set_bool() {
        let source = CString::new(
            "---\nparams: [flag = bool]\n---\n> {% if flag %}\n\nyes\n\n> {% else %}\n\nno\n\n> {% /if %}",
        )
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let ctx = pt_context_new();
        let key = CString::new("flag").unwrap();
        let err = unsafe { pt_context_set_bool(ctx, key.as_ptr(), true) };
        assert!(err.is_null());

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "yes\n");

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_context_set_json_list() {
        let source = CString::new(
            "---\nparams:\n  - items = list<label = str>\n---\n> {% for item in items %}\n\n{{ item.label }}\n\n> {% /for %}",
        )
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let ctx = pt_context_new();
        let key = CString::new("items").unwrap();
        let json = CString::new(r#"[{"label":"alpha"},{"label":"beta"}]"#).unwrap();
        let err = unsafe { pt_context_set_json(ctx, key.as_ptr(), json.as_ptr()) };
        assert!(err.is_null());

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "alpha\nbeta\n");

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_source_hash() {
        let source = CString::new("---\nparams: [x = str]\n---\n{{ x }}").unwrap();
        let mut t1: *mut PtTemplate = ptr::null_mut();
        let mut t2: *mut PtTemplate = ptr::null_mut();
        unsafe {
            pt_template_from_source(source.as_ptr(), &raw mut t1);
            pt_template_from_source(source.as_ptr(), &raw mut t2);
        }
        let h1 = unsafe { pt_template_source_hash(t1) };
        let h2 = unsafe { pt_template_source_hash(t2) };
        assert_eq!(h1, h2);
        unsafe {
            pt_template_free(t1);
            pt_template_free(t2);
        }
    }

    #[test]
    fn test_declarations() {
        let source =
            CString::new("---\nparams: [name = str, count = int]\n---\n{{ name }} {{ count }}")
                .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        let declarations_raw = unsafe { pt_template_declarations(tmpl) };
        let declarations_text = unsafe { CStr::from_ptr(declarations_raw) }
            .to_str()
            .unwrap();
        assert!(declarations_text.contains("name"));
        assert!(declarations_text.contains("str"));
        assert!(declarations_text.contains("count"));
        assert!(declarations_text.contains("int"));
        unsafe {
            pt_free_string(declarations_raw);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_render_missing_param_error() {
        let source =
            CString::new("---\nparams: [name = str, age = int]\n---\n{{ name }} {{ age }}")
                .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        let ctx = pt_context_new();
        let key = CString::new("name").unwrap();
        let val = CString::new("Alice").unwrap();
        unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(result.is_null(), "expected render to fail");
        assert!(!render_err.is_null());
        let err_str = unsafe { CStr::from_ptr(render_err) }.to_str().unwrap();
        assert!(
            err_str.contains("missing"),
            "error should mention 'missing': {err_str}"
        );

        unsafe {
            pt_free_string(render_err);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_render_allowing_extra() {
        let source = CString::new("---\nparams: [name = str]\n---\nHello {{ name }}!").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        let ctx = pt_context_new();
        let key = CString::new("name").unwrap();
        let val = CString::new("world").unwrap();
        unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
        let extra_key = CString::new("bogus").unwrap();
        let extra_val = CString::new("ignored").unwrap();
        unsafe { pt_context_set_str(ctx, extra_key.as_ptr(), extra_val.as_ptr()) };

        // Strict mode should fail
        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(
            result.is_null(),
            "strict render should fail with extra params"
        );
        unsafe { pt_free_string(render_err) };

        // Allow-extra mode should succeed
        let mut render_err2: *mut c_char = ptr::null_mut();
        let result2 = unsafe { pt_template_render_allowing_extra(tmpl, ctx, &raw mut render_err2) };
        assert!(render_err2.is_null());
        assert!(!result2.is_null());
        let result_str = unsafe { CStr::from_ptr(result2) }.to_str().unwrap();
        assert_eq!(result_str, "Hello world!");

        unsafe {
            pt_free_string(result2);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_cache_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.tmpl.md");
        std::fs::write(&path, "---\nparams: [x = str]\n---\n{{ x }}").unwrap();

        let cache = pt_cache_new();
        let path_c = CString::new(path.to_str().unwrap()).unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_cache_load(cache, path_c.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());
        assert!(!tmpl.is_null());

        let count = unsafe { pt_cache_template_count(cache) };
        assert_eq!(count, 1);

        unsafe { pt_cache_clear(cache) };
        let count_after = unsafe { pt_cache_template_count(cache) };
        assert_eq!(count_after, 0);

        unsafe {
            pt_template_free(tmpl);
            pt_cache_free(cache);
        }
    }

    #[test]
    fn test_json_to_value_dict() {
        let val = json_to_value(r#"{"name":"Alice","score":42}"#).unwrap();
        assert!(val.is_struct());
        assert_eq!(val.get_field("name").unwrap().as_str(), Some("Alice"));
        assert_eq!(val.get_field("score").unwrap().as_int(), Some(42));
    }

    #[test]
    fn test_json_to_value_nested() {
        let val = json_to_value(r#"{"items":[{"label":"a"},{"label":"b"}]}"#).unwrap();
        let items = val.get_field("items").unwrap().as_list().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get_field("label").unwrap().as_str(), Some("a"));
    }

    #[test]
    fn test_context_set_float_render() {
        let source = CString::new("---\nparams: [score = float]\n---\n{{ score }}").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let ctx = pt_context_new();
        let key = CString::new("score").unwrap();
        let err = unsafe { pt_context_set_float(ctx, key.as_ptr(), 3.25) };
        assert!(err.is_null());

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "3.25");

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_template_body() {
        let source = CString::new("---\nparams: [x = str]\n---\nBody: {{ x }}").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        let body = unsafe { pt_template_body(tmpl) };
        let body_str = unsafe { CStr::from_ptr(body) }.to_str().unwrap();
        assert!(body_str.contains("Body:"));
        unsafe {
            pt_free_string(body);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_context_set_tmpl_render() {
        // Template that takes a tmpl<> param: card = tmpl<title = str>
        // and iterates over items, including card for each
        let card_source =
            CString::new("---\nname: card\nparams: [title = str]\n---\n* {{ title }}").unwrap();
        let mut card_tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe {
            pt_template_from_source_allowing_unused(card_source.as_ptr(), &raw mut card_tmpl)
        };
        assert!(!card_tmpl.is_null());

        let main_source = CString::new(
            "---\nparams:\n  - card = tmpl<title = str>\n  - items = list<name = str>\n---\n> {% for item in items %}\n> {% include card with title=item.name %}\n> {% /for %}"
        ).unwrap();
        let mut main_tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(main_source.as_ptr(), &raw mut main_tmpl) };
        assert!(err.is_null());

        let ctx = pt_context_new();
        let card_key = CString::new("card").unwrap();
        let err = unsafe { pt_context_set_tmpl(ctx, card_key.as_ptr(), card_tmpl) };
        assert!(err.is_null());

        let items_key = CString::new("items").unwrap();
        let items_json = CString::new(r#"[{"name":"Alpha"},{"name":"Beta"}]"#).unwrap();
        let err = unsafe { pt_context_set_json(ctx, items_key.as_ptr(), items_json.as_ptr()) };
        assert!(err.is_null());

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(main_tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null(), "render error: {:?}", unsafe {
            render_err.as_ref().map(|p| CStr::from_ptr(p))
        });
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert!(
            result_str.contains("Alpha"),
            "expected Alpha in output, got: {result_str}"
        );
        assert!(
            result_str.contains("Beta"),
            "expected Beta in output, got: {result_str}"
        );

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(main_tmpl);
            pt_template_free(card_tmpl);
        }
    }

    #[test]
    fn test_value_to_json_roundtrip() {
        assert_eq!(value_to_json(&Value::Int(42)), "42");
        assert_eq!(value_to_json(&Value::Bool(true)), "true");
        assert_eq!(value_to_json(&Value::Bool(false)), "false");
        assert_eq!(value_to_json(&Value::Str("hello".into())), "\"hello\"");
        assert_eq!(
            value_to_json(&Value::Str("say \"hi\"\n".into())),
            "\"say \\\"hi\\\"\\n\""
        );
        assert_eq!(value_to_json(&Value::Float(3.5)), "3.5");
    }

    #[test]
    fn test_defaults_json() {
        let source = CString::new(
            "---\nparams:\n  - name = str := \"World\"\n  - count = int := 5\n  - flag = bool\n---\n{{ name }} {{ count }} {{ flag }}",
        )
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(!tmpl.is_null());

        let raw_cstr = unsafe { pt_template_defaults_json(tmpl) };
        let text = unsafe { CStr::from_ptr(raw_cstr) }.to_str().unwrap();
        assert!(
            text.contains("\"name\": \"World\""),
            "expected name default in JSON: {text}"
        );
        assert!(
            text.contains("\"count\": 5"),
            "expected count default in JSON: {text}"
        );
        assert!(
            !text.contains("flag"),
            "flag should not appear in defaults: {text}"
        );

        unsafe {
            pt_free_string(raw_cstr);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_defaults_json_empty() {
        let source = CString::new("---\nparams: [x = str]\n---\n{{ x }}").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        let raw_cstr = unsafe { pt_template_defaults_json(tmpl) };
        let text = unsafe { CStr::from_ptr(raw_cstr) }.to_str().unwrap();
        assert_eq!(text, "{}");

        unsafe {
            pt_free_string(raw_cstr);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_consts_json() {
        let source = CString::new(
            "---\nconsts:\n  - MAX = int := 100\n  - GREETING = str := \"hello\"\nparams: []\n---\n{{ MAX }} {{ GREETING }}",
        )
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(!tmpl.is_null());

        let raw_cstr = unsafe { pt_template_consts_json(tmpl) };
        let text = unsafe { CStr::from_ptr(raw_cstr) }.to_str().unwrap();
        assert!(
            text.contains("\"MAX\": 100"),
            "expected MAX const in JSON: {text}"
        );
        assert!(
            text.contains("\"GREETING\": \"hello\""),
            "expected GREETING const in JSON: {text}"
        );

        unsafe {
            pt_free_string(raw_cstr);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_consts_json_empty() {
        let source = CString::new("---\nparams: [x = str]\n---\n{{ x }}").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        let raw_cstr = unsafe { pt_template_consts_json(tmpl) };
        let text = unsafe { CStr::from_ptr(raw_cstr) }.to_str().unwrap();
        assert_eq!(text, "{}");

        unsafe {
            pt_free_string(raw_cstr);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_defaults_context() {
        let source = CString::new(
            "---\nparams:\n  - name = str := \"World\"\n  - greeting = str\n---\n{{ greeting }} {{ name }}",
        )
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(!tmpl.is_null());

        let ctx = unsafe { pt_template_defaults_context(tmpl) };
        assert!(!ctx.is_null());

        // Set the non-default param
        let key = CString::new("greeting").unwrap();
        let val = CString::new("Hello").unwrap();
        let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
        assert!(err.is_null());

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null(), "render error");
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "Hello World");

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_defaults_context_override() {
        let source =
            CString::new("---\nparams:\n  - name = str := \"World\"\n---\n{{ name }}").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        let ctx = unsafe { pt_template_defaults_context(tmpl) };
        let key = CString::new("name").unwrap();
        let val = CString::new("Alice").unwrap();
        unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "Alice");

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_from_source_with_base_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("header.tmpl.md"),
            "---\nname: header\nparams: [title = str]\n---\n# {{ title }}",
        )
        .unwrap();

        let source = CString::new(
            "---\nparams: [title = str]\n---\n> {% include [header](header.tmpl.md) with title=title %}\n\nBody",
        )
        .unwrap();
        let base_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe {
            pt_template_from_source_with_base_dir(source.as_ptr(), base_dir.as_ptr(), &raw mut tmpl)
        };
        assert!(err.is_null(), "error: {:?}", unsafe {
            err.as_ref().map(|p| CStr::from_ptr(p))
        });
        assert!(!tmpl.is_null());

        let ctx = pt_context_new();
        let key = CString::new("title").unwrap();
        let val = CString::new("Test").unwrap();
        unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert!(
            result_str.contains("Test"),
            "expected Test in: {result_str}"
        );

        unsafe {
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_from_source_with_frontmatter() {
        let source = CString::new(
            "---\nname: greeting\ndescription: A greeting template\nparams: [name = str]\n---\nHello {{ name }}!",
        )
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let mut fm: *mut c_char = ptr::null_mut();
        let err = unsafe {
            pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
        };
        assert!(err.is_null(), "expected no error");
        assert!(!tmpl.is_null());
        assert!(!fm.is_null());

        let fm_str = unsafe { CStr::from_ptr(fm) }.to_str().unwrap();
        assert!(
            fm_str.contains("\"name\":\"greeting\""),
            "expected name in fm: {fm_str}"
        );
        assert!(
            fm_str.contains("\"description\":\"A greeting template\""),
            "expected desc in fm: {fm_str}"
        );
        assert!(
            fm_str.contains("\"has_params\":true"),
            "expected has_params in fm: {fm_str}"
        );
        assert!(
            fm_str.contains("\"allow_unused\":false"),
            "expected allow_unused in fm: {fm_str}"
        );

        // Verify template still works
        let ctx = pt_context_new();
        let key = CString::new("name").unwrap();
        let val = CString::new("World").unwrap();
        unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
        assert!(render_err.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "Hello World!");

        unsafe {
            pt_free_string(fm);
            pt_free_string(result);
            pt_context_free(ctx);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_from_source_with_frontmatter_no_params() {
        let source =
            CString::new("---\nname: static\ndescription: No params\nparams: []\n---\nHello!")
                .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let mut fm: *mut c_char = ptr::null_mut();
        let err = unsafe {
            pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
        };
        assert!(err.is_null());
        assert!(!tmpl.is_null());

        let fm_str = unsafe { CStr::from_ptr(fm) }.to_str().unwrap();
        assert!(
            fm_str.contains("\"name\":\"static\""),
            "expected name in fm: {fm_str}"
        );
        assert!(
            fm_str.contains("\"params\":[]"),
            "expected empty params in fm: {fm_str}"
        );

        unsafe {
            pt_free_string(fm);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_from_source_with_frontmatter_error() {
        let source = CString::new("no frontmatter at all").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let mut fm: *mut c_char = ptr::null_mut();
        let err = unsafe {
            pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
        };
        assert!(!err.is_null(), "expected error for invalid source");
        assert!(tmpl.is_null(), "template should be null on error");
        unsafe { pt_free_string(err) };
    }

    #[test]
    fn test_validate_declarations_match() {
        let source =
            CString::new("---\nparams: [name = str, count = int]\n---\n{{ name }} {{ count }}")
                .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(!tmpl.is_null());

        // Same declarations should validate
        let expected = CString::new(r#"[["name","str"],["count","int"]]"#).unwrap();
        let err = unsafe { pt_template_validate_declarations(tmpl, expected.as_ptr()) };
        assert!(err.is_null(), "expected matching declarations");

        unsafe { pt_template_free(tmpl) };
    }

    #[test]
    fn test_validate_declarations_mismatch_retyped() {
        let source = CString::new("---\nparams: [name = str]\n---\n{{ name }}").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        // Different type: expect "retyped"
        let expected = CString::new(r#"[["name","int"]]"#).unwrap();
        let err = unsafe { pt_template_validate_declarations(tmpl, expected.as_ptr()) };
        assert!(!err.is_null(), "expected mismatch error");
        let err_str = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
        assert!(
            err_str.contains("retyped"),
            "expected retyped in error: {err_str}"
        );

        unsafe {
            pt_free_string(err);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_validate_declarations_mismatch_added() {
        let source =
            CString::new("---\nparams: [name = str, count = int]\n---\n{{ name }} {{ count }}")
                .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        // Expected has fewer params — template has "added" count
        let expected = CString::new(r#"[["name","str"]]"#).unwrap();
        let err = unsafe { pt_template_validate_declarations(tmpl, expected.as_ptr()) };
        assert!(!err.is_null(), "expected mismatch error for added param");
        let err_str = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
        assert!(
            err_str.contains("added"),
            "expected 'added' in error: {err_str}"
        );

        unsafe {
            pt_free_string(err);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_render_json_single_shot() {
        let source =
            CString::new("---\nparams: [name = str, count = int]\n---\n{{ name }}: {{ count }}")
                .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let json = CString::new(r#"{"name":"Alice","count":42}"#).unwrap();
        let mut render_err: *mut c_char = ptr::null_mut();
        let result =
            unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
        assert!(render_err.is_null(), "expected no render error");
        assert!(!result.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "Alice: 42");

        unsafe {
            pt_free_string(result);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_render_json_allow_extra() {
        let source = CString::new("---\nparams: [name = str]\n---\nHello {{ name }}!").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        let json = CString::new(r#"{"name":"world","extra":"ignored"}"#).unwrap();

        // Strict mode should fail with extra key.
        let mut render_err: *mut c_char = ptr::null_mut();
        let result =
            unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
        assert!(
            result.is_null(),
            "strict render should fail with extra params"
        );
        unsafe { pt_free_string(render_err) };

        // Allow-extra mode should succeed.
        let mut render_err2: *mut c_char = ptr::null_mut();
        let result2 =
            unsafe { pt_template_render_json(tmpl, json.as_ptr(), true, &raw mut render_err2) };
        assert!(render_err2.is_null());
        assert!(!result2.is_null());
        let result_str = unsafe { CStr::from_ptr(result2) }.to_str().unwrap();
        assert_eq!(result_str, "Hello world!");

        unsafe {
            pt_free_string(result2);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_render_json_non_object_error() {
        let source = CString::new("---\nparams: [x = str]\n---\n{{ x }}").unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

        let json = CString::new(r"[1, 2, 3]").unwrap();
        let mut render_err: *mut c_char = ptr::null_mut();
        let result =
            unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
        assert!(result.is_null());
        assert!(!render_err.is_null());
        let err_str = unsafe { CStr::from_ptr(render_err) }.to_str().unwrap();
        assert!(
            err_str.contains("object"),
            "expected 'object' in error: {err_str}"
        );

        unsafe {
            pt_free_string(render_err);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_render_flexbuffers_single_shot() {
        use serde::Serialize;
        #[derive(Serialize)]
        struct Params {
            name: String,
            count: i64,
        }

        let source =
            CString::new("---\nparams: [name = str, count = int]\n---\n{{ name }}: {{ count }}")
                .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let params = Params {
            name: "Alice".to_string(),
            count: 42,
        };
        let data = flexbuffers::to_vec(&params).unwrap();

        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe {
            pt_template_render_flexbuffers(
                tmpl,
                data.as_ptr(),
                data.len(),
                false,
                &raw mut render_err,
            )
        };
        assert!(render_err.is_null(), "expected no render error");
        assert!(!result.is_null());
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "Alice: 42");

        unsafe {
            pt_free_string(result);
            pt_template_free(tmpl);
        }
    }

    // -- option<T> tests --

    #[test]
    fn test_option_json_null_renders_none_via_match() {
        let source = CString::new(concat!(
            "---\nparams:\n  - label = option<str>\n---\n",
            "> {% match label %}\n",
            "> {% case Some %}\n\n",
            "got:{{ label.val }}\n\n",
            "> {% case None %}\n\n",
            "empty\n\n",
            "> {% /match %}"
        ))
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let json = CString::new(r#"{"label":null}"#).unwrap();
        let mut render_err: *mut c_char = ptr::null_mut();
        let result =
            unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
        assert!(render_err.is_null(), "expected no render error");
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str.trim(), "empty");

        unsafe {
            pt_free_string(result);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_option_json_some_renders_value_via_match() {
        let source = CString::new(concat!(
            "---\nparams:\n  - label = option<str>\n---\n",
            "> {% match label %}\n",
            "> {% case Some %}\n\n",
            "got:{{ label.val }}\n\n",
            "> {% case None %}\n\n",
            "empty\n\n",
            "> {% /match %}"
        ))
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let json = CString::new(r#"{"label":{"__kind__":"Some","val":"hello"}}"#).unwrap();
        let mut render_err: *mut c_char = ptr::null_mut();
        let result =
            unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
        assert!(render_err.is_null(), "expected no render error");
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert!(
            result_str.contains("got:hello"),
            "expected 'got:hello', got '{result_str}'"
        );

        unsafe {
            pt_free_string(result);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_option_json_null_via_has() {
        let source = CString::new(concat!(
            "---\nparams:\n  - label = option<str>\n---\n",
            "> {% if has(label) %}\n\n",
            "got:{{ label.val }}\n\n",
            "> {% else %}\n\n",
            "empty\n\n",
            "> {% /if %}"
        ))
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let json = CString::new(r#"{"label":null}"#).unwrap();
        let mut render_err: *mut c_char = ptr::null_mut();
        let result =
            unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
        assert!(render_err.is_null(), "expected no render error");
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str.trim(), "empty");

        unsafe {
            pt_free_string(result);
            pt_template_free(tmpl);
        }
    }

    #[test]
    fn test_option_json_some_via_has() {
        let source = CString::new(concat!(
            "---\nparams:\n  - label = option<str>\n---\n",
            "> {% if has(label) %}\n\n",
            "got:{{ label.val }}\n\n",
            "> {% else %}\n\n",
            "empty\n\n",
            "> {% /if %}"
        ))
        .unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
        assert!(err.is_null());

        let json = CString::new(r#"{"label":{"__kind__":"Some","val":"world"}}"#).unwrap();
        let mut render_err: *mut c_char = ptr::null_mut();
        let result =
            unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
        assert!(render_err.is_null(), "expected no render error");
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert!(
            result_str.contains("got:world"),
            "expected 'got:world', got '{result_str}'"
        );

        unsafe {
            pt_free_string(result);
            pt_template_free(tmpl);
        }
    }
}
