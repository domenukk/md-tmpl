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
mod template;

// Re-export the full `pt_*` FFI surface at the crate root so the public API is
// unchanged after splitting the implementation across submodules.
pub use cache::*;
pub use context::*;
pub use metadata::*;
pub use render::*;
pub use security::*;
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
pub(crate) const ERR_NULL_TEMPLATE: &str = "null template";
pub(crate) const ERR_NULL_CONTEXT: &str = "null context";
pub(crate) const ERR_NULL_CACHE: &str = "null cache";
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

/// Allocate a C error string. Returns null on success (no error).
fn err_to_cstring(msg: &str) -> *mut c_char {
    into_cstring_raw(msg)
}

/// Separator between the stable error-kind id and the human-readable message
/// in error strings returned across the FFI boundary.
///
/// The ASCII Unit Separator (0x1F) never appears in template messages or
/// rendered output, so bindings can split on it unambiguously.
const ERR_KIND_SEP: char = '\u{1f}';

/// Allocate a C error string for a [`TemplateError`], prefixed with its stable
/// kind id and [`ERR_KIND_SEP`], e.g. `"missing_params\u{1f}missing required
/// parameter: name"`.
///
/// Bindings split on the separator to recover a machine-readable error kind
/// alongside the human-readable message. Reuses [`err_to_cstring`] for the
/// NUL-safe allocation.
fn terr_to_cstring(e: &TemplateError) -> *mut c_char {
    err_to_cstring(&format!("{}{}{}", e.kind().as_str(), ERR_KIND_SEP, e))
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
