//! C FFI bindings for the host-side envelope, XML codec, notice, and truncation primitives.

use std::{borrow::Cow, ffi::c_char, ptr};

use md_tmpl::Value;

use crate::{
    err_to_cstring, into_cstring_raw,
    json::json_to_value,
    metadata::value_to_json,
    security::{bytes_to_str_lossy, opt_cstr_to_str, serialize_delimiter_pairs},
    terr_to_cstring,
};

/// String constants exposed by [`pt_security_constants_json`] (`name`, `value`).
const STRING_CONSTANTS: &[(&str, &str)] = &[
    ("DEFAULT_QUARANTINE_TAG", md_tmpl::DEFAULT_QUARANTINE_TAG),
    ("DEFAULT_SANITIZE_TAG", md_tmpl::DEFAULT_SANITIZE_TAG),
    ("DEFAULT_SANITIZE_NOTICE", md_tmpl::DEFAULT_SANITIZE_NOTICE),
    (
        "SANITIZE_NOTICE_TAG_PLACEHOLDER",
        md_tmpl::SANITIZE_NOTICE_TAG_PLACEHOLDER,
    ),
    (
        "TOOL_OUTPUT_QUARANTINE_TAG",
        md_tmpl::TOOL_OUTPUT_QUARANTINE_TAG,
    ),
    (
        "TOOL_OUTPUT_QUARANTINE_OPEN_TAG",
        md_tmpl::TOOL_OUTPUT_QUARANTINE_OPEN_TAG,
    ),
    (
        "TOOL_OUTPUT_QUARANTINE_CLOSE_TAG",
        md_tmpl::TOOL_OUTPUT_QUARANTINE_CLOSE_TAG,
    ),
    (
        "UNTRUSTED_TOOL_OUTPUT_TAG",
        md_tmpl::UNTRUSTED_TOOL_OUTPUT_TAG,
    ),
    (
        "UNTRUSTED_QUARANTINE_OPEN_TAG",
        md_tmpl::UNTRUSTED_QUARANTINE_OPEN_TAG,
    ),
    (
        "UNTRUSTED_QUARANTINE_CLOSE_TAG",
        md_tmpl::UNTRUSTED_QUARANTINE_CLOSE_TAG,
    ),
    ("DEFAULT_TRUNCATE_MARKER", md_tmpl::DEFAULT_TRUNCATE_MARKER),
    (
        "DEFAULT_TRUNCATE_MARKER_COMPACT",
        md_tmpl::DEFAULT_TRUNCATE_MARKER_COMPACT,
    ),
    (
        "DEFAULT_LIST_TRUNCATE_MARKER",
        md_tmpl::DEFAULT_LIST_TRUNCATE_MARKER,
    ),
];
/// JSON key holding [`md_tmpl::XML_DECODE_PAIRS`] in [`pt_security_constants_json`].
const XML_DECODE_PAIRS_KEY: &str = "XML_DECODE_PAIRS";
/// Error returned when the list passed to [`pt_truncate_middle_list_json`] is not a JSON array.
const ERR_EXPECTED_JSON_ARRAY: &str = "expected a JSON array of items";

/// Store `err` in `out_err` and return null.
///
/// # Safety
///
/// `out_err` must be a valid pointer to a `*mut c_char`.
unsafe fn fail(out_err: *mut *mut c_char, err: &str) -> *mut c_char {
    unsafe { *out_err = err_to_cstring(err) };
    ptr::null_mut()
}

/// Return all host sanitization string constants plus `XML_DECODE_PAIRS` as one JSON object.
/// Caller must free the returned string with `pt_free_string`.
#[unsafe(no_mangle)]
pub extern "C" fn pt_security_constants_json() -> *mut c_char {
    let mut out = String::from("{");
    for &(name, value) in STRING_CONSTANTS {
        out.push('"');
        out.push_str(name);
        out.push_str("\":\"");
        out.push_str(&md_tmpl::escape_json_str(value));
        out.push_str("\",");
    }
    out.push('"');
    out.push_str(XML_DECODE_PAIRS_KEY);
    out.push_str("\":");
    out.push_str(&serialize_delimiter_pairs(md_tmpl::XML_DECODE_PAIRS));
    out.push('}');
    into_cstring_raw(out)
}

/// Apply an infallible string transform to a `(data, len)` buffer (invalid UTF-8 is replaced).
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
unsafe fn map_str(
    data: *const u8,
    len: usize,
    out_err: *mut *mut c_char,
    transform: impl FnOnce(&str) -> Cow<'_, str>,
) -> *mut c_char {
    match unsafe { bytes_to_str_lossy(data, len) } {
        Ok(s) => {
            unsafe { *out_err = ptr::null_mut() };
            into_cstring_raw(transform(&s).into_owned())
        }
        Err(e) => unsafe { fail(out_err, &e) },
    }
}

/// Decode the standard XML entity references (`&quot;`, `&#39;`, `&apos;`, `&lt;`, `&gt;`, `&amp;`).
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_decode_xml(
    data: *const u8,
    len: usize,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    unsafe { map_str(data, len, out_err, md_tmpl::decode_xml_str) }
}

/// Escape a string for an XML attribute value.
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_escape_xml_attr(
    data: *const u8,
    len: usize,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    unsafe { map_str(data, len, out_err, md_tmpl::escape_xml_attr_str) }
}

/// Escape an XML element body value (`&`, `<`, `>`, and NUL).
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_escape_xml_body(
    data: *const u8,
    len: usize,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    unsafe { map_str(data, len, out_err, md_tmpl::escape_xml_body_str) }
}

/// Format the boundary notice for `primary_tag` (`custom_notice` may be null for the default).
///
/// # Safety
///
/// - `primary_tag` must be a valid NUL-terminated UTF-8 C string.
/// - If non-null, `custom_notice` must be a valid NUL-terminated UTF-8 C string.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_format_sanitize_notice(
    primary_tag: *const c_char,
    custom_notice: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let tag = match unsafe { crate::cstr_to_str(primary_tag) } {
        Ok(t) => t,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    let notice = match unsafe { opt_cstr_to_str(custom_notice) } {
        Ok(n) => n,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    unsafe { *out_err = ptr::null_mut() };
    into_cstring_raw(md_tmpl::format_sanitize_notice(tag, notice).into_owned())
}

/// Strip outer `<tag_name>` / `</tag_name>` tags (and one envelope newline per side), or return
/// null when `s` is not wrapped or an argument is invalid.
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - `tag_name` must be a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_strip_outer_quarantine_tag(
    data: *const u8,
    len: usize,
    tag_name: *const c_char,
) -> *mut c_char {
    let (Ok(s), Ok(tag)) = (unsafe { bytes_to_str_lossy(data, len) }, unsafe {
        crate::cstr_to_str(tag_name)
    }) else {
        return ptr::null_mut();
    };
    md_tmpl::strip_outer_quarantine_tag(&s, tag).map_or(ptr::null_mut(), into_cstring_raw)
}

/// Extract the inner payload of a verified quarantine envelope, or return null if invalid.
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - If non-null, `tag_spec` must be a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_unquarantine(
    data: *const u8,
    len: usize,
    tag_spec: *const c_char,
) -> *mut c_char {
    let (Ok(s), Ok(spec)) = (unsafe { bytes_to_str_lossy(data, len) }, unsafe {
        opt_cstr_to_str(tag_spec)
    }) else {
        return ptr::null_mut();
    };
    md_tmpl::unquarantine_str(&s, spec).map_or(ptr::null_mut(), into_cstring_raw)
}

/// Idempotently sanitize and wrap the input in a quarantine envelope.
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - If non-null, `tag_spec` must be a valid NUL-terminated UTF-8 C string.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_quarantine_untrusted_idempotent(
    data: *const u8,
    len: usize,
    tag_spec: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let s = match unsafe { bytes_to_str_lossy(data, len) } {
        Ok(s) => s,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    let spec = match unsafe { opt_cstr_to_str(tag_spec) } {
        Ok(t) => t,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    match md_tmpl::quarantine_untrusted_idempotent_str(&s, spec) {
        Ok(out) => {
            unsafe { *out_err = ptr::null_mut() };
            into_cstring_raw(out)
        }
        Err(e) => {
            unsafe { *out_err = terr_to_cstring(&e) };
            ptr::null_mut()
        }
    }
}

/// Idempotently sanitize and wrap the input in a `sanitize_block` envelope.
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - If non-null, `tag_spec` and `custom_notice` must be valid NUL-terminated UTF-8 C strings.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_sanitize_block_idempotent(
    data: *const u8,
    len: usize,
    tag_spec: *const c_char,
    custom_notice: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let s = match unsafe { bytes_to_str_lossy(data, len) } {
        Ok(s) => s,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    let spec = match unsafe { opt_cstr_to_str(tag_spec) } {
        Ok(t) => t,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    let notice = match unsafe { opt_cstr_to_str(custom_notice) } {
        Ok(n) => n,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    match md_tmpl::sanitize_block_idempotent_str(&s, spec, notice) {
        Ok(out) => {
            unsafe { *out_err = ptr::null_mut() };
            into_cstring_raw(out)
        }
        Err(e) => {
            unsafe { *out_err = terr_to_cstring(&e) };
            ptr::null_mut()
        }
    }
}

/// Truncate the input in the middle, keeping at most `limit` UTF-8 bytes (`marker` may be null
/// for the default marker and may use `{skipped}`).
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - If non-null, `marker` must be a valid NUL-terminated UTF-8 C string.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_truncate_middle(
    data: *const u8,
    len: usize,
    limit: usize,
    marker: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let marker = match unsafe { opt_cstr_to_str(marker) } {
        Ok(m) => m,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    unsafe {
        map_str(data, len, out_err, |s| {
            md_tmpl::truncate_middle_str(s, limit, marker)
        })
    }
}

/// Truncate a JSON array of items in the middle, keeping at most `limit` items around one marker
/// item (`marker` null = default, `""` = no marker). Returns the resulting JSON array.
///
/// # Safety
///
/// - `items_json` must be a valid NUL-terminated UTF-8 C string.
/// - If non-null, `marker` must be a valid NUL-terminated UTF-8 C string.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_truncate_middle_list_json(
    items_json: *const c_char,
    limit: usize,
    marker: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let json = match unsafe { crate::cstr_to_str(items_json) } {
        Ok(j) => j,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    let marker = match unsafe { opt_cstr_to_str(marker) } {
        Ok(m) => m,
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    let items = match json_to_value(json) {
        Ok(Value::List(items)) => items,
        Ok(_) => return unsafe { fail(out_err, ERR_EXPECTED_JSON_ARRAY) },
        Err(e) => return unsafe { fail(out_err, &e) },
    };
    let truncated = md_tmpl::truncate_middle_list(&items, limit, marker);
    unsafe { *out_err = ptr::null_mut() };
    into_cstring_raw(value_to_json(&Value::List(std::sync::Arc::new(truncated))))
}
