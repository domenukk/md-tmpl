//! Template rendering FFI: context-, JSON-, and `FlexBuffers`-driven render
//! entry points.

use std::{
    ffi::{CString, c_char},
    ptr,
};

use md_tmpl::{Context, Value};

use crate::{
    PtCache, PtContext, PtTemplate, cstr_to_str, err_to_cstring, json::json_to_value,
    terr_to_cstring,
};

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
            unsafe { *out_err = terr_to_cstring(&e) };
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
            unsafe { *out_err = terr_to_cstring(&e) };
            ptr::null_mut()
        }
    }
}

/// Render a template that declares no required parameters, using an empty context.
///
/// Convenience wrapper over [`pt_template_render`] for parameter-less templates.
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
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_render_empty(
    tmpl: *const PtTemplate,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        unsafe { *out_err = err_to_cstring("null template") };
        return ptr::null_mut();
    };
    match tmpl.inner.render_empty() {
        Ok(rendered) => {
            unsafe { *out_err = ptr::null_mut() };
            CString::new(rendered)
                .unwrap_or_else(|_| CString::new("<output contained NUL byte>").unwrap())
                .into_raw()
        }
        Err(e) => {
            unsafe { *out_err = terr_to_cstring(&e) };
            ptr::null_mut()
        }
    }
}

/// Render a template **without** context validation.
///
/// Skips the check that context values match frontmatter declarations. Use
/// only when the caller has already validated the context (e.g. via a prior
/// strict render) and wants to avoid redundant validation.
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
pub unsafe extern "C" fn pt_template_render_unchecked(
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
    match tmpl.inner.render_ctx_unchecked(&ctx.inner) {
        Ok(rendered) => {
            unsafe { *out_err = ptr::null_mut() };
            CString::new(rendered)
                .unwrap_or_else(|_| CString::new("<output contained NUL byte>").unwrap())
                .into_raw()
        }
        Err(e) => {
            unsafe { *out_err = terr_to_cstring(&e) };
            ptr::null_mut()
        }
    }
}

/// Render a template, resolving `{% include %}` directives through a cache.
///
/// Like [`pt_template_render`], but included templates are looked up in the
/// supplied cache — unchanged includes are not re-read or re-compiled. This is
/// the recommended path for hot-reload scenarios with frequent re-renders.
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
/// - `cache` must be a valid cache handle.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_render_cached(
    tmpl: *const PtTemplate,
    ctx: *const PtContext,
    cache: *const PtCache,
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
    let Some(cache) = (unsafe { cache.as_ref() }) else {
        unsafe { *out_err = err_to_cstring("null cache") };
        return ptr::null_mut();
    };
    match tmpl.inner.render_ctx_cached(&ctx.inner, &*cache.inner) {
        Ok(rendered) => {
            unsafe { *out_err = ptr::null_mut() };
            CString::new(rendered)
                .unwrap_or_else(|_| CString::new("<output contained NUL byte>").unwrap())
                .into_raw()
        }
        Err(e) => {
            unsafe { *out_err = terr_to_cstring(&e) };
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
            unsafe { *out_err = terr_to_cstring(&e) };
            ptr::null_mut()
        }
    }
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
            unsafe { *out_err = terr_to_cstring(&e) };
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
            unsafe { *out_err = terr_to_cstring(&e) };
            ptr::null_mut()
        }
    }
}
