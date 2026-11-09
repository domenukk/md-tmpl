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

/// Return `true` if `data[..len]` contains any stripped invisible Unicode, Bidi control, Plane 14 Tag,
/// or C0/DEL/C1 control character. Null `data` with `len > 0` fails closed (`true`).
///
/// # Safety
///
/// If `len > 0`, `data` must point to `len` valid initialized bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_has_invisible_chars(data: *const u8, len: usize) -> bool {
    let Ok(s) = (unsafe { bytes_to_str_lossy(data, len) }) else {
        return true;
    };
    md_tmpl::has_invisible_chars(&s)
}

/// Strip invisible Unicode smuggling characters, Bidi overrides/isolates, Plane 14 Tags, and
/// dangerous C0/DEL/C1 control characters in a single pass.
///
/// # Safety
///
/// - If `len > 0`, `data` must point to `len` valid initialized bytes.
/// - `out_err` must be a valid pointer to a `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_strip_invisible(
    data: *const u8,
    len: usize,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    unsafe { map_str(data, len, out_err, md_tmpl::strip_invisible_str) }
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

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;
    use crate::pt_free_string;

    unsafe fn take_cstr(ptr: *mut c_char) -> String {
        assert!(!ptr.is_null());
        let s = unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .expect("valid UTF-8 C string")
            .to_owned();
        unsafe { pt_free_string(ptr) };
        s
    }

    #[test]
    fn ffi_host_constants_and_invisible_chars() {
        unsafe {
            let constants_json = take_cstr(pt_security_constants_json());
            assert!(constants_json.contains("\"DEFAULT_SANITIZE_TAG\":\"untrusted_content\""));
            assert!(constants_json.contains("\"XML_DECODE_PAIRS\":"));

            let clean = "Hello 👨‍👩‍👧‍👦 ❤️ world";
            assert!(!pt_has_invisible_chars(clean.as_ptr(), clean.len()));
            // Null pointer with non-zero length fails closed (true).
            assert!(pt_has_invisible_chars(ptr::null(), 4));

            let mut err: *mut c_char = ptr::null_mut();
            let stripped_clean = take_cstr(pt_strip_invisible(
                clean.as_ptr(),
                clean.len(),
                &raw mut err,
            ));
            assert!(err.is_null());
            assert_eq!(stripped_clean, clean);

            let smuggled = "a\u{200B}b\u{202E}c\u{E0020}d\0e";
            assert!(pt_has_invisible_chars(smuggled.as_ptr(), smuggled.len()));
            let stripped = take_cstr(pt_strip_invisible(
                smuggled.as_ptr(),
                smuggled.len(),
                &raw mut err,
            ));
            assert!(err.is_null());
            assert_eq!(stripped, "abcde");
        }
    }

    #[test]
    fn ffi_host_xml_codec_envelopes_and_truncation() {
        unsafe {
            let mut err: *mut c_char = ptr::null_mut();
            let decoded = take_cstr(pt_decode_xml(b"a &lt;b&gt;".as_ptr(), 11, &raw mut err));
            assert!(err.is_null());
            assert_eq!(decoded, "a <b>");

            let attr = take_cstr(pt_escape_xml_attr(b"a\"b\nc".as_ptr(), 5, &raw mut err));
            assert!(err.is_null());
            assert_eq!(attr, "a&quot;b\\nc");

            let body = take_cstr(pt_escape_xml_body(b"a<b\0c".as_ptr(), 5, &raw mut err));
            assert!(err.is_null());
            assert_eq!(body, "a&lt;b\\0c");

            let tag_doc = CString::new("doc").unwrap();
            let custom_notice = CString::new("see {tag}").unwrap();
            let notice = take_cstr(pt_format_sanitize_notice(
                tag_doc.as_ptr(),
                custom_notice.as_ptr(),
                &raw mut err,
            ));
            assert!(err.is_null());
            assert_eq!(notice, "see doc");

            let wrapped = "<doc>\n\nhello\n\n</doc>";
            let inner = take_cstr(pt_strip_outer_quarantine_tag(
                wrapped.as_ptr(),
                wrapped.len(),
                tag_doc.as_ptr(),
            ));
            assert_eq!(inner, "\nhello\n");

            let clean_env = "<doc>\nok\n</doc>";
            let unq = take_cstr(pt_unquarantine(
                clean_env.as_ptr(),
                clean_env.len(),
                tag_doc.as_ptr(),
            ));
            assert_eq!(unq, "ok");

            let dirty_env = "<doc>\n<|im_start|>\n</doc>";
            let rewrapped = take_cstr(pt_quarantine_untrusted_idempotent(
                dirty_env.as_ptr(),
                dirty_env.len(),
                tag_doc.as_ptr(),
                &raw mut err,
            ));
            assert!(err.is_null());
            assert_eq!(rewrapped, "<doc>\n&lt;|im_start|&gt;\n</doc>");

            let san_block = take_cstr(pt_sanitize_block_idempotent(
                b"payload".as_ptr(),
                7,
                tag_doc.as_ptr(),
                ptr::null(),
                &raw mut err,
            ));
            assert!(err.is_null());
            let san_again = take_cstr(pt_sanitize_block_idempotent(
                san_block.as_ptr(),
                san_block.len(),
                tag_doc.as_ptr(),
                ptr::null(),
                &raw mut err,
            ));
            assert!(err.is_null());
            assert_eq!(san_again, san_block);

            let trunc_str = take_cstr(pt_truncate_middle(
                b"abcdef".as_ptr(),
                6,
                100,
                ptr::null(),
                &raw mut err,
            ));
            assert!(err.is_null());
            assert_eq!(trunc_str, "abcdef");

            let items_json = CString::new(r#"["a","b","c","d","e"]"#).unwrap();
            let marker = CString::new("..{skipped}..").unwrap();
            let trunc_list = take_cstr(pt_truncate_middle_list_json(
                items_json.as_ptr(),
                3,
                marker.as_ptr(),
                &raw mut err,
            ));
            assert!(err.is_null());
            assert_eq!(trunc_list, r#"["a", "..2..", "d", "e"]"#);
        }
    }
}
