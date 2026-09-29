//! Template metadata and defaults/constants introspection FFI.

use std::{ffi::c_char, ptr};

use md_tmpl::{Context, Value};

use crate::{
    EMPTY_JSON_ARRAY, EMPTY_JSON_OBJECT, EMPTY_STR, JSON_BOOL_FALSE, JSON_BOOL_TRUE, JSON_NULL,
    JSON_TEMPLATE_PLACEHOLDER, PtContext, PtTemplate, into_cstring_raw,
};

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
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_name(tmpl: *const PtTemplate) -> *mut c_char {
    match unsafe { tmpl.as_ref() }.and_then(|t| t.inner.name()) {
        Some(name) => into_cstring_raw(name),
        None => ptr::null_mut(),
    }
}

/// Return the template description from frontmatter, or null if none was declared.
///
/// A non-null result must be freed with `pt_free_string`. Null means the
/// template has no `description:` field (distinct from an empty description).
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_description(tmpl: *const PtTemplate) -> *mut c_char {
    match unsafe { tmpl.as_ref() }.and_then(|t| t.inner.description()) {
        Some(desc) => into_cstring_raw(desc),
        None => ptr::null_mut(),
    }
}

/// Return the template body (after frontmatter stripping).
///
/// The returned string must be freed with `pt_free_string`.
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_body(tmpl: *const PtTemplate) -> *mut c_char {
    match unsafe { tmpl.as_ref() } {
        Some(t) => into_cstring_raw(t.inner.body()),
        None => into_cstring_raw(EMPTY_STR),
    }
}

/// Return declarations as a JSON array of `[name, type]` pairs.
///
/// The returned string must be freed with `pt_free_string`.
///
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_declarations(tmpl: *const PtTemplate) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return into_cstring_raw(EMPTY_JSON_ARRAY);
    };
    let decls: Vec<String> = tmpl
        .inner
        .declarations()
        .iter()
        .map(|d| format!("[\"{}\",\"{}\"]", d.name, d.var_type))
        .collect();
    into_cstring_raw(format!("[{}]", decls.join(",")))
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
        Value::Str(s) => format!("\"{}\"", md_tmpl::escape_json_str(s)),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => if *b { JSON_BOOL_TRUE } else { JSON_BOOL_FALSE }.to_string(),
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(value_to_json).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Struct(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", md_tmpl::escape_json_str(k), value_to_json(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        Value::Tmpl(_) => JSON_TEMPLATE_PLACEHOLDER.to_string(),
        Value::None => JSON_NULL.to_string(),
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
        return into_cstring_raw(EMPTY_JSON_OBJECT);
    };
    let defaults = tmpl.inner.defaults();
    if defaults.is_empty() {
        return into_cstring_raw(EMPTY_JSON_OBJECT);
    }
    let pairs: Vec<String> = defaults
        .iter()
        .map(|(k, v)| format!("\"{k}\": {}", value_to_json(v)))
        .collect();
    into_cstring_raw(format!("{{{}}}", pairs.join(", ")))
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
        return into_cstring_raw(EMPTY_JSON_OBJECT);
    };
    let consts = tmpl.inner.consts();
    if consts.is_empty() {
        return into_cstring_raw(EMPTY_JSON_OBJECT);
    }
    let pairs: Vec<String> = consts
        .iter()
        .map(|(k, v)| format!("\"{k}\": {}", value_to_json(v)))
        .collect();
    into_cstring_raw(format!("{{{}}}", pairs.join(", ")))
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
/// # Safety
///
/// `tmpl` must be a valid template handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_imported_consts_json(tmpl: *const PtTemplate) -> *mut c_char {
    let Some(tmpl) = (unsafe { tmpl.as_ref() }) else {
        return into_cstring_raw(EMPTY_JSON_OBJECT);
    };
    let imported = tmpl.inner.imported_consts();
    if imported.is_empty() {
        return into_cstring_raw(EMPTY_JSON_OBJECT);
    }
    let pairs: Vec<String> = imported
        .iter()
        .map(|(k, v)| format!("\"{k}\": {}", value_to_json(v)))
        .collect();
    into_cstring_raw(format!("{{{}}}", pairs.join(", ")))
}
