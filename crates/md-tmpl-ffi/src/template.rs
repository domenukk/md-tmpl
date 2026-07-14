//! Template lifecycle FFI: constructors, destructor, and declaration
//! validation.

use std::{
    ffi::{CString, c_char},
    path::Path,
    ptr,
};

use md_tmpl::{CompileOptions, Template, Value};

use crate::{
    PtTemplate, cstr_to_str, err_to_cstring,
    json::{parse_json_env_object, parse_json_string_pairs},
    terr_to_cstring,
};

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
        Err(e) => terr_to_cstring(&e),
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
        Err(e) => terr_to_cstring(&e),
    }
}

/// Parse a template with compile-time environment variables.
///
/// `env_json` must be a JSON object string mapping env names to typed values,
/// e.g. `{"PROMPTS_DIR": "/path", "MAX_RETRIES": 5}`.
///
/// # Safety
///
/// - `source` must be a valid NUL-terminated UTF-8 string.
/// - `env_json` must be a valid NUL-terminated UTF-8 string containing a JSON object.
/// - `out` must be a valid pointer to a `*mut PtTemplate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_from_source_with_env(
    source: *const c_char,
    env_json: *const c_char,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    let source = match unsafe { cstr_to_str(source) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let env_json_str = match unsafe { cstr_to_str(env_json) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let env_pairs = match parse_json_env_object(env_json_str) {
        Ok(pairs) => pairs,
        Err(e) => return err_to_cstring(&e),
    };
    let env_refs: Vec<(&str, Value)> = env_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    match Template::compile(source, CompileOptions::default().env(&env_refs)) {
        Ok((tmpl, _fm)) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out = Box::into_raw(handle) };
            ptr::null_mut()
        }
        Err(e) => terr_to_cstring(&e),
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
        Err(e) => terr_to_cstring(&e),
    }
}

/// Parsed compile-option inputs shared by the `*_with_options` constructors:
/// an optional base directory and the owned `(name, value)` env pairs.
type CompileOptionInputs<'a> = (Option<&'a str>, Vec<(String, Value)>);

/// Parse the optional `base_dir`/`env_json` C strings shared by the
/// `*_with_options` constructors into owned env pairs and an optional base dir.
///
/// Returns `Err` with an allocated C error string when a pointer holds invalid
/// UTF-8 or the env JSON cannot be parsed.
unsafe fn parse_compile_option_inputs<'a>(
    base_dir: *const c_char,
    env_json: *const c_char,
) -> Result<CompileOptionInputs<'a>, *mut c_char> {
    let dir_str = if base_dir.is_null() {
        None
    } else {
        match unsafe { cstr_to_str(base_dir) } {
            Ok(s) => Some(s),
            Err(e) => return Err(err_to_cstring(&e)),
        }
    };

    let env_pairs = if env_json.is_null() {
        Vec::new()
    } else {
        let env_json_str = match unsafe { cstr_to_str(env_json) } {
            Ok(s) => s,
            Err(e) => return Err(err_to_cstring(&e)),
        };
        match parse_json_env_object(env_json_str) {
            Ok(pairs) => pairs,
            Err(e) => return Err(err_to_cstring(&e)),
        }
    };
    Ok((dir_str, env_pairs))
}

/// Parse a template from source with any combination of compile options.
///
/// This is the unified constructor: `base_dir` and `env_json` are optional
/// (pass null to omit), and `allow_unused` toggles the unused-parameter check.
/// It replaces the family of single-purpose `pt_template_from_source_*`
/// constructors and, unlike them, allows all options to be combined.
///
/// `env_json`, when non-null, must be a JSON object string mapping env names to
/// typed values, e.g. `{"PROMPTS_DIR": "/path", "MAX_RETRIES": 5}`.
///
/// # Safety
///
/// - `source` must be a valid NUL-terminated UTF-8 string.
/// - `base_dir`, if non-null, must be a valid NUL-terminated UTF-8 file path.
/// - `env_json`, if non-null, must be a valid NUL-terminated UTF-8 JSON object.
/// - `out` must be a valid pointer to a `*mut PtTemplate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_from_source_with_options(
    source: *const c_char,
    base_dir: *const c_char,
    env_json: *const c_char,
    allow_unused: bool,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    let src = match unsafe { cstr_to_str(source) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let (dir_str, env_pairs) = match unsafe { parse_compile_option_inputs(base_dir, env_json) } {
        Ok(parsed) => parsed,
        Err(err) => return err,
    };
    let env_refs: Vec<(&str, Value)> = env_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();

    let mut options = CompileOptions::default()
        .allow_unused(allow_unused)
        .env(&env_refs);
    if let Some(dir) = dir_str {
        options = options.base_dir(Path::new(dir));
    }

    match Template::compile(src, options) {
        Ok((tmpl, _fm)) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out = Box::into_raw(handle) };
            ptr::null_mut()
        }
        Err(e) => terr_to_cstring(&e),
    }
}

/// Load a template from a file with any combination of compile options.
///
/// Like [`pt_template_from_source_with_options`], but reads the template from
/// `path`. When `base_dir` is null, the file's parent directory is used for
/// include resolution.
///
/// # Safety
///
/// - `path` must be a valid NUL-terminated UTF-8 file path.
/// - `base_dir`, if non-null, must be a valid NUL-terminated UTF-8 file path.
/// - `env_json`, if non-null, must be a valid NUL-terminated UTF-8 JSON object.
/// - `out` must be a valid pointer to a `*mut PtTemplate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_template_from_file_with_options(
    path: *const c_char,
    base_dir: *const c_char,
    env_json: *const c_char,
    allow_unused: bool,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    let path_str = match unsafe { cstr_to_str(path) } {
        Ok(s) => s,
        Err(e) => return err_to_cstring(&e),
    };
    let (dir_str, env_pairs) = match unsafe { parse_compile_option_inputs(base_dir, env_json) } {
        Ok(parsed) => parsed,
        Err(err) => return err,
    };
    let env_refs: Vec<(&str, Value)> = env_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();

    let mut options = CompileOptions::default()
        .allow_unused(allow_unused)
        .env(&env_refs);
    if let Some(dir) = dir_str {
        options = options.base_dir(Path::new(dir));
    }

    match Template::compile_file(Path::new(path_str), options) {
        Ok((tmpl, _fm)) => {
            let handle = Box::new(PtTemplate { inner: tmpl });
            unsafe { *out = Box::into_raw(handle) };
            ptr::null_mut()
        }
        Err(e) => terr_to_cstring(&e),
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
        Err(e) => terr_to_cstring(&e),
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

            let name_escaped = fm
                .name
                // NOLINT: missing name defaults to empty string for JSON output
                .unwrap_or_default()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            let desc_escaped = fm
                .description
                // NOLINT: missing description defaults to empty string for JSON output
                .unwrap_or_default()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            let params: Vec<String> = fm.params.iter().map(|p| format!("\"{p}\"")).collect();

            let json = format!(
                "{{\"name\":\"{name_escaped}\",\"description\":\"{desc_escaped}\",\"has_params\":{},\"allow_unused\":{},\"params\":[{}]}}",
                fm.has_params,
                fm.allow_unused,
                params.join(",")
            );
            unsafe {
                *out_fm = CString::new(json)
                    .unwrap_or_else(|_| CString::new("<NUL byte in output>").unwrap())
                    .into_raw();
            }
            ptr::null_mut()
        }
        Err(e) => {
            unsafe { *out_tmpl = ptr::null_mut() };
            terr_to_cstring(&e)
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
