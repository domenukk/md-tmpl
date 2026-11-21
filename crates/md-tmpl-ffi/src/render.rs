//! Template rendering FFI: context-, JSON-, and `FlexBuffers`-driven render
//! entry points.

use std::{ffi::c_char, ptr};

use md_tmpl::{Context, Value};

use crate::{
    ERR_NULL_CACHE, ERR_NULL_CONTEXT, ERR_NULL_TEMPLATE, PtCache, PtContext, PtTemplate,
    catch_ffi_out_err, cstr_to_str, err_to_cstring, into_cstring_raw, json::json_to_value,
    raw_bytes_slice, terr_to_cstring,
};

const ERR_RENDER_JSON_NOT_OBJECT: &str = "pt_template_render_json: JSON must be an object";
const ERR_RENDER_FLEXBUFFERS_NULL: &str = "pt_template_render_flexbuffers: null pointer passed";

/// Render a template with the given context (strict mode).
///
/// Returns the rendered output as a C string (caller must free with
/// `pt_free_string`). On error, writes the error string to `*out_err`
/// and returns null.
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
    unsafe {
        catch_ffi_out_err(out_err, ptr::null_mut(), |out_err| {
            let Some(tmpl) = tmpl.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_TEMPLATE);
                return ptr::null_mut();
            };
            let Some(ctx) = ctx.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_CONTEXT);
                return ptr::null_mut();
            };
            match tmpl.inner.render_ctx(&ctx.inner) {
                Ok(rendered) => {
                    *out_err = ptr::null_mut();
                    into_cstring_raw(rendered)
                }
                Err(e) => {
                    *out_err = terr_to_cstring(&e);
                    ptr::null_mut()
                }
            }
        })
    }
}

/// Render a template, allowing extra (undeclared) parameters.
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
    unsafe {
        catch_ffi_out_err(out_err, ptr::null_mut(), |out_err| {
            let Some(tmpl) = tmpl.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_TEMPLATE);
                return ptr::null_mut();
            };
            let Some(ctx) = ctx.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_CONTEXT);
                return ptr::null_mut();
            };
            match tmpl.inner.render_ctx_allowing_extra(&ctx.inner) {
                Ok(rendered) => {
                    *out_err = ptr::null_mut();
                    into_cstring_raw(rendered)
                }
                Err(e) => {
                    *out_err = terr_to_cstring(&e);
                    ptr::null_mut()
                }
            }
        })
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
/// # Safety
///
/// - `tmpl` must be a valid template handle.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_render_empty(
    tmpl: *const PtTemplate,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        catch_ffi_out_err(out_err, ptr::null_mut(), |out_err| {
            let Some(tmpl) = tmpl.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_TEMPLATE);
                return ptr::null_mut();
            };
            match tmpl.inner.render_empty() {
                Ok(rendered) => {
                    *out_err = ptr::null_mut();
                    into_cstring_raw(rendered)
                }
                Err(e) => {
                    *out_err = terr_to_cstring(&e);
                    ptr::null_mut()
                }
            }
        })
    }
}

/// Render a template, resolving `{% include %}` directives through a cache.
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
    unsafe {
        catch_ffi_out_err(out_err, ptr::null_mut(), |out_err| {
            let Some(tmpl) = tmpl.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_TEMPLATE);
                return ptr::null_mut();
            };
            let Some(ctx) = ctx.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_CONTEXT);
                return ptr::null_mut();
            };
            let Some(cache) = cache.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_CACHE);
                return ptr::null_mut();
            };
            match tmpl.inner.render_ctx_cached(&ctx.inner, &*cache.inner) {
                Ok(rendered) => {
                    *out_err = ptr::null_mut();
                    into_cstring_raw(rendered)
                }
                Err(e) => {
                    *out_err = terr_to_cstring(&e);
                    ptr::null_mut()
                }
            }
        })
    }
}

/// Render a template directly from a JSON object string, in a single FFI call.
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
    unsafe {
        catch_ffi_out_err(out_err, ptr::null_mut(), |out_err| {
            let Some(tmpl) = tmpl.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_TEMPLATE);
                return ptr::null_mut();
            };
            let json_str = match cstr_to_str(json) {
                Ok(s) => s,
                Err(e) => {
                    *out_err = err_to_cstring(&e);
                    return ptr::null_mut();
                }
            };

            let obj = match json_to_value(json_str) {
                Ok(val) => val,
                Err(e) => {
                    *out_err = err_to_cstring(&e);
                    return ptr::null_mut();
                }
            };
            let Value::Struct(map) = obj else {
                *out_err = err_to_cstring(ERR_RENDER_JSON_NOT_OBJECT);
                return ptr::null_mut();
            };

            let mut ctx = Context::new();
            for (key, val) in map.iter() {
                if let Err(e) = ctx.try_set(key, val.clone()) {
                    *out_err = terr_to_cstring(&e);
                    return ptr::null_mut();
                }
            }

            let result = if allow_extra {
                tmpl.inner.render_ctx_allowing_extra(&ctx)
            } else {
                tmpl.inner.render_ctx(&ctx)
            };
            match result {
                Ok(rendered) => {
                    *out_err = ptr::null_mut();
                    into_cstring_raw(rendered)
                }
                Err(e) => {
                    *out_err = terr_to_cstring(&e);
                    ptr::null_mut()
                }
            }
        })
    }
}

/// Render a template directly from a `FlexBuffers` map binary buffer, in a single FFI call.
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
    unsafe {
        catch_ffi_out_err(out_err, ptr::null_mut(), |out_err| {
            let Some(tmpl) = tmpl.as_ref() else {
                *out_err = err_to_cstring(ERR_NULL_TEMPLATE);
                return ptr::null_mut();
            };
            let slice = match raw_bytes_slice(data, len, ERR_RENDER_FLEXBUFFERS_NULL) {
                Ok(s) => s,
                Err(e) => {
                    *out_err = err_to_cstring(&e);
                    return ptr::null_mut();
                }
            };
            let ctx = match Context::from_flexbuffers(slice) {
                Ok(c) => c,
                Err(e) => {
                    *out_err = terr_to_cstring(&e);
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
                    *out_err = ptr::null_mut();
                    into_cstring_raw(rendered)
                }
                Err(e) => {
                    *out_err = terr_to_cstring(&e);
                    ptr::null_mut()
                }
            }
        })
    }
}
