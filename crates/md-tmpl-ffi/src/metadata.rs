//! Template metadata and defaults/constants introspection FFI.

use std::{
    ffi::{CString, c_char},
    ptr,
};

use md_tmpl::{Context, Value};

use crate::{PtContext, PtTemplate};

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

/// Return the template name from frontmatter, or null if none was declared.
///
/// A non-null result must be freed with `pt_free_string`. Null means the
/// template has no `name:` field (distinct from an empty name).
///
/// # Panics
///
/// Panics if `CString::new` fails on the name (should not happen unless the
/// name contains interior NUL bytes).
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_name(tmpl: *const PtTemplate) -> *mut c_char {
    match unsafe { tmpl.as_ref() }.and_then(|t| t.inner.name()) {
        Some(name) => CString::new(name)
            .unwrap_or_else(|_| CString::new("<NUL byte in name>").unwrap())
            .into_raw(),
        None => ptr::null_mut(),
    }
}

/// Return the template description from frontmatter, or null if none was declared.
///
/// A non-null result must be freed with `pt_free_string`. Null means the
/// template has no `description:` field (distinct from an empty description).
///
/// # Panics
///
/// Panics if `CString::new` fails on the description (should not happen unless
/// it contains interior NUL bytes).
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_description(tmpl: *const PtTemplate) -> *mut c_char {
    match unsafe { tmpl.as_ref() }.and_then(|t| t.inner.description()) {
        Some(desc) => CString::new(desc)
            .unwrap_or_else(|_| CString::new("<NUL byte in description>").unwrap())
            .into_raw(),
        None => ptr::null_mut(),
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
    CString::new(json)
        .unwrap_or_else(|_| CString::new("<NUL byte in output>").unwrap())
        .into_raw()
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

/// Convert a template `Value` to a JSON string.
pub(crate) fn value_to_json(val: &Value) -> String {
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
/// # Panics
///
/// Panics if `CString::new` fails on the JSON output (should not happen
/// unless default values contain interior NUL bytes).
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
    CString::new(json)
        .unwrap_or_else(|_| CString::new("<NUL byte in output>").unwrap())
        .into_raw()
}

/// Returns the constants as a JSON object string.
///
/// Only template-level constants are included (not imported ones).
/// The caller must free the string with `pt_free_string`.
///
/// Returns `"{}"` if no constants exist.
///
/// # Panics
///
/// Panics if `CString::new` fails on the JSON output (should not happen
/// unless constant values contain interior NUL bytes).
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
    CString::new(json)
        .unwrap_or_else(|_| CString::new("<NUL byte in output>").unwrap())
        .into_raw()
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

/// Returns the imported constants as a JSON object string.
///
/// These are constants imported from other templates via `imports:` directives,
/// keyed by `stem.NAME` (e.g. `other.MAX_RETRIES`).
///
/// The caller must free the string with `pt_free_string`.
/// Returns `"{}"` if no imported constants exist.
///
/// # Panics
///
/// Panics if `CString::new` fails on the JSON output (should not happen
/// unless imported constant values contain interior NUL bytes).
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
    CString::new(json)
        .unwrap_or_else(|_| CString::new("<NUL byte in output>").unwrap())
        .into_raw()
}
