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

use std::{
    ffi::{CStr, CString, c_char},
    sync::Arc,
};

use md_tmpl::{Context, Template, TemplateCache, TemplateError};

mod json;

mod cache;
mod context;
mod metadata;
mod render;
mod security;
mod security_host;
mod template;

// Re-export the full `pt_*` FFI surface at the crate root so the public API is
// unchanged after splitting the implementation across submodules.
pub use cache::*;
pub use context::*;
pub use metadata::*;
pub use render::*;
pub use security::*;
pub use security_host::*;
pub use template::*;

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
// Shared FFI constants & CString helpers
// ---------------------------------------------------------------------------

pub(crate) const EMPTY_STR: &str = "";
pub(crate) const EMPTY_JSON_ARRAY: &str = "[]";
pub(crate) const EMPTY_JSON_OBJECT: &str = "{}";
pub(crate) const ERR_NULL_POINTER: &str = "null pointer";
pub(crate) const ERR_NULL_OUT_POINTER: &str = "null out pointer";
pub(crate) const ERR_BUFFER_TOO_LARGE: &str = "buffer length exceeds isize::MAX";
pub(crate) const ERR_NULL_TEMPLATE: &str = "null template";
pub(crate) const ERR_NULL_CONTEXT: &str = "null context";
pub(crate) const ERR_NULL_CACHE: &str = "null cache";
pub(crate) const ERR_INTERNAL_PANIC: &str = "internal FFI panic";
pub(crate) const JSON_BOOL_TRUE: &str = "true";
pub(crate) const JSON_BOOL_FALSE: &str = "false";
pub(crate) const JSON_NULL: &str = "null";
pub(crate) const JSON_TEMPLATE_PLACEHOLDER: &str = "\"<template>\"";

/// Convert an owned string or byte vector into a NUL-free `*mut c_char`,
/// stripping any interior NUL bytes if present.
pub(crate) fn into_cstring_raw(s: impl Into<Vec<u8>>) -> *mut c_char {
    CString::new(s)
        .unwrap_or_else(|e| {
            let cleaned: Vec<u8> = e.into_vec().into_iter().filter(|&b| b != 0).collect();
            CString::new(cleaned).expect("filtered out NUL bytes")
        })
        .into_raw()
}

/// Validate a `(data, len)` FFI byte slice, allowing `(null, 0)` as `&[]` while rejecting
/// `(null, len > 0)` and `len > isize::MAX as usize`.
///
/// # Safety
///
/// If `len > 0` and `data` is non-null, `data` must point to `len` initialized bytes.
pub(crate) unsafe fn raw_bytes_slice<'a>(
    data: *const u8,
    len: usize,
    null_err: &str,
) -> Result<&'a [u8], String> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(null_err.to_string());
    }
    if len > isize::MAX as usize {
        return Err(ERR_BUFFER_TOO_LARGE.to_string());
    }
    Ok(unsafe { std::slice::from_raw_parts(data, len) })
}

/// Convert a C string pointer to a Rust `&str`.
///
/// # Safety
///
/// The pointer must be non-null and point to a valid NUL-terminated UTF-8
/// string.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err(ERR_NULL_POINTER.to_string());
    }
    // SAFETY: caller guarantees non-null, NUL-terminated.
    let cstr = unsafe { CStr::from_ptr(ptr) };
    cstr.to_str().map_err(|e| format!("invalid UTF-8: {e}"))
}

/// Separator between the stable error-kind id and the human-readable message
/// in error strings returned across the FFI boundary.
///
/// Any literal occurrence inside untrusted error messages is replaced with `' '`
/// so bindings can split on `ERR_KIND_SEP` without framing spoofing.
const ERR_KIND_SEP: char = '\u{1f}';

fn strip_err_sep(msg: &str) -> std::borrow::Cow<'_, str> {
    if msg.contains(ERR_KIND_SEP) {
        std::borrow::Cow::Owned(msg.replace(ERR_KIND_SEP, " "))
    } else {
        std::borrow::Cow::Borrowed(msg)
    }
}

/// Allocate a C error string with any embedded `ERR_KIND_SEP` neutralized.
fn err_to_cstring(msg: &str) -> *mut c_char {
    into_cstring_raw(strip_err_sep(msg).into_owned())
}

/// Extract a human-readable panic description without unwinding across FFI.
fn panic_payload_msg(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        format!("{ERR_INTERNAL_PANIC}: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("{ERR_INTERNAL_PANIC}: {s}")
    } else {
        ERR_INTERNAL_PANIC.to_string()
    }
}

/// Catch panics in FFI functions returning `*mut c_char` error strings (null = success).
pub(crate) fn catch_ffi_err(f: impl FnOnce() -> *mut c_char) -> *mut c_char {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(ptr) => ptr,
        Err(payload) => err_to_cstring(&panic_payload_msg(&*payload)),
    }
}

/// Catch panics in FFI functions returning a value + `*mut *mut c_char` `out_err` pointer.
/// Safely returns `err_ret` without dereferencing if `out_err` itself is null.
///
/// # Safety
///
/// If non-null, `out_err` must point to a valid writable `*mut c_char`.
pub(crate) unsafe fn catch_ffi_out_err<T: Copy>(
    out_err: *mut *mut c_char,
    err_ret: T,
    f: impl FnOnce(*mut *mut c_char) -> T,
) -> T {
    if out_err.is_null() {
        return err_ret;
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(out_err))) {
        Ok(val) => val,
        Err(payload) => {
            unsafe { *out_err = err_to_cstring(&panic_payload_msg(&*payload)) };
            err_ret
        }
    }
}

/// Catch panics in FFI functions without an error out-channel, returning `fallback` on unwind.
pub(crate) fn catch_ffi_val<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// Allocate a C error string for a [`TemplateError`]:
/// `"<kind>\u{1f}<message>"`, followed by `"\u{1f}<details-json>"` when the error
/// carries a structured payload.
fn terr_to_cstring(e: &TemplateError) -> *mut c_char {
    let raw_msg = e.to_string();
    let clean_msg = strip_err_sep(&raw_msg);
    let mut out = format!("{}{}{}", e.kind().as_str(), ERR_KIND_SEP, clean_msg);
    if let Some(details) = error_details_json(&e.details()) {
        out.push(ERR_KIND_SEP);
        out.push_str(&details);
    }
    into_cstring_raw(out)
}

/// Serialize an [`md_tmpl::ErrorDetails`] payload as a compact JSON object, or `None`
/// when the error carries no payload. Keys match every binding's typed error fields.
fn error_details_json(d: &md_tmpl::ErrorDetails<'_>) -> Option<String> {
    use std::fmt::Write as _;

    fn push_key(out: &mut String, key: &str) {
        out.push(if out.is_empty() { '{' } else { ',' });
        out.push('"');
        out.push_str(key);
        out.push_str("\":");
    }
    fn push_str(out: &mut String, key: &str, value: Option<&str>) {
        if let Some(value) = value {
            push_key(out, key);
            out.push('"');
            out.push_str(&md_tmpl::escape_json_str(value));
            out.push('"');
        }
    }
    fn push_list(out: &mut String, key: &str, values: &[String]) {
        if values.is_empty() {
            return;
        }
        push_key(out, key);
        out.push('[');
        for (i, value) in values.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&md_tmpl::escape_json_str(value));
            out.push('"');
        }
        out.push(']');
    }

    let mut out = String::new();
    if let Some(line) = d.line {
        push_key(&mut out, "line");
        write!(out, "{line}").expect("write to String");
    }
    push_str(&mut out, "snippet", d.snippet);
    push_str(&mut out, "variable", d.variable);
    push_list(&mut out, "missing", d.missing);
    push_list(&mut out, "extra", d.extra);
    push_str(&mut out, "path", d.path);
    push_str(&mut out, "expected", d.expected);
    push_str(&mut out, "actual", d.actual);
    push_str(&mut out, "filter", d.filter);
    push_str(&mut out, "include", d.include);
    push_str(&mut out, "details", d.details);
    if out.is_empty() {
        None
    } else {
        out.push('}');
        Some(out)
    }
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
    catch_ffi_val((), || {
        if !ptr.is_null() {
            // SAFETY: caller guarantees ptr came from CString::into_raw.
            drop(unsafe { CString::from_raw(ptr) });
        }
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
