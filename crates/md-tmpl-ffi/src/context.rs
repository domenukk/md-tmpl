//! Rendering [`Context`] lifecycle and value-setter FFI.

use std::{ffi::c_char, ptr};

use md_tmpl::{Context, Value};

use crate::{
    ERR_NULL_CONTEXT, ERR_NULL_TEMPLATE, PtContext, PtTemplate, catch_ffi_err, catch_ffi_val,
    cstr_to_str, err_to_cstring, json::json_to_value, raw_bytes_slice, terr_to_cstring,
};

const ERR_MERGE_JSON_NOT_OBJECT: &str = "pt_context_merge_json: JSON must be an object";
const ERR_SET_FLEXBUFFERS_NULL: &str = "pt_context_set_flexbuffers: null pointer passed";
const ERR_MERGE_FLEXBUFFERS_NULL: &str = "pt_context_merge_flexbuffers: null pointer passed";

fn set_ctx_val(ctx: &mut PtContext, key: &str, value: impl Into<Value>) -> *mut c_char {
    match ctx.inner.try_set(key, value) {
        Ok(()) => ptr::null_mut(),
        Err(e) => terr_to_cstring(&e),
    }
}

/// Create a new empty rendering context.
#[unsafe(no_mangle)]
pub extern "C" fn pt_context_new() -> *mut PtContext {
    catch_ffi_val(ptr::null_mut(), || {
        Box::into_raw(Box::new(PtContext {
            inner: Context::new(),
        }))
    })
}

/// Free a context handle.
///
/// # Safety
///
/// `ctx` must have been returned by `pt_context_new` and must not be freed
/// twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_context_free(ctx: *mut PtContext) {
    catch_ffi_val((), || {
        if !ctx.is_null() {
            drop(unsafe { Box::from_raw(ctx) });
        }
    });
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let key = match unsafe { cstr_to_str(key) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        let value = match unsafe { cstr_to_str(value) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        set_ctx_val(ctx, key, value)
    })
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let key = match unsafe { cstr_to_str(key) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        set_ctx_val(ctx, key, value)
    })
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let key = match unsafe { cstr_to_str(key) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        set_ctx_val(ctx, key, value)
    })
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let key = match unsafe { cstr_to_str(key) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        set_ctx_val(ctx, key, value)
    })
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let key = match unsafe { cstr_to_str(key) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        set_ctx_val(ctx, key, Value::None)
    })
}

/// Set a JSON value in the context (for complex types: lists, dicts, enums).
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let key = match unsafe { cstr_to_str(key) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        let json_str = match unsafe { cstr_to_str(json) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        match json_to_value(json_str) {
            Ok(val) => set_ctx_val(ctx, key, val),
            Err(e) => err_to_cstring(&e),
        }
    })
}

/// Set a template-typed parameter in the context.
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let key = match unsafe { cstr_to_str(key) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
            return err_to_cstring(ERR_NULL_TEMPLATE);
        };
        set_ctx_val(ctx, key, Value::from(&tmpl.inner))
    })
}

/// Merge all top-level keys from a JSON object into a context.
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
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
            return err_to_cstring(ERR_MERGE_JSON_NOT_OBJECT);
        };
        for (key, val) in map.iter() {
            if let Err(e) = ctx.inner.try_set(key, val.clone()) {
                return terr_to_cstring(&e);
            }
        }
        ptr::null_mut()
    })
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let key = match unsafe { cstr_to_str(key) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        let slice = match unsafe { raw_bytes_slice(data, len, ERR_SET_FLEXBUFFERS_NULL) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        match Value::from_flexbuffers(slice) {
            Ok(val) => set_ctx_val(ctx, key, val),
            Err(e) => terr_to_cstring(&e),
        }
    })
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
    catch_ffi_err(|| {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return err_to_cstring(ERR_NULL_CONTEXT);
        };
        let slice = match unsafe { raw_bytes_slice(data, len, ERR_MERGE_FLEXBUFFERS_NULL) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        let new_ctx = match Context::from_flexbuffers(slice) {
            Ok(c) => c,
            Err(e) => return terr_to_cstring(&e),
        };
        for (k, v) in new_ctx.into_inner() {
            if let Err(e) = ctx.inner.try_set(&k, v) {
                return terr_to_cstring(&e);
            }
        }
        ptr::null_mut()
    })
}
