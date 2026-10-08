//! Template compilation, rendering entry points, caching, and error kinds.

use std::ptr;

use crate::*;

#[test]
fn test_from_source_and_render() {
    let source = CString::new(
        "\
---
params:
  - name = str
---
Hello {{ name }}!",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null(), "expected no error");
    assert!(!tmpl.is_null());

    let ctx = pt_context_new();
    let key = CString::new("name").unwrap();
    let val = CString::new("world").unwrap();
    let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    assert!(!result.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Hello world!");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_syntax_error() {
    let source = CString::new("no frontmatter").unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(!err.is_null(), "expected syntax error");
    assert!(tmpl.is_null());
    unsafe { pt_free_string(err) };
}

#[test]
fn test_source_hash() {
    let source = CString::new(
        "\
---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();
    let mut t1: *mut PtTemplate = ptr::null_mut();
    let mut t2: *mut PtTemplate = ptr::null_mut();
    unsafe {
        pt_template_from_source(source.as_ptr(), &raw mut t1);
        pt_template_from_source(source.as_ptr(), &raw mut t2);
    }
    let h1 = unsafe { pt_template_source_hash(t1) };
    let h2 = unsafe { pt_template_source_hash(t2) };
    assert_eq!(h1, h2);
    unsafe {
        pt_template_free(t1);
        pt_template_free(t2);
    }
}

#[test]
fn test_render_missing_param_error() {
    let source = CString::new(
        "\
---
params: [name = str, age = int]
---
{{ name }} {{ age }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

    let ctx = pt_context_new();
    let key = CString::new("name").unwrap();
    let val = CString::new("Alice").unwrap();
    unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(result.is_null(), "expected render to fail");
    assert!(!render_err.is_null());
    let err_str = unsafe { CStr::from_ptr(render_err) }.to_str().unwrap();
    assert!(
        err_str.contains("missing"),
        "error should mention 'missing': {err_str}"
    );

    unsafe {
        pt_free_string(render_err);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_allowing_extra() {
    let source = CString::new(
        "\
---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

    let ctx = pt_context_new();
    let key = CString::new("name").unwrap();
    let val = CString::new("world").unwrap();
    unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
    let extra_key = CString::new("bogus").unwrap();
    let extra_val = CString::new("ignored").unwrap();
    unsafe { pt_context_set_str(ctx, extra_key.as_ptr(), extra_val.as_ptr()) };

    // Strict mode should fail
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(
        result.is_null(),
        "strict render should fail with extra params"
    );
    unsafe { pt_free_string(render_err) };

    // Allow-extra mode should succeed
    let mut render_err2: *mut c_char = ptr::null_mut();
    let result2 = unsafe { pt_template_render_allowing_extra(tmpl, ctx, &raw mut render_err2) };
    assert!(render_err2.is_null());
    assert!(!result2.is_null());
    let result_str = unsafe { CStr::from_ptr(result2) }.to_str().unwrap();
    assert_eq!(result_str, "Hello world!");

    unsafe {
        pt_free_string(result2);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_cache_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tmpl.md");
    std::fs::write(
        &path,
        "\
---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();

    let cache = pt_cache_new();
    let path_c = CString::new(path.to_str().unwrap()).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_cache_load(cache, path_c.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());
    assert!(!tmpl.is_null());

    let count = unsafe { pt_cache_template_count(cache) };
    assert_eq!(count, 1);

    unsafe { pt_cache_clear(cache) };
    let count_after = unsafe { pt_cache_template_count(cache) };
    assert_eq!(count_after, 0);

    unsafe {
        pt_template_free(tmpl);
        pt_cache_free(cache);
    }
}

#[test]
fn test_template_body() {
    let source = CString::new(
        "\
---
params: [x = str]
---
Body: {{ x }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    let body = unsafe { pt_template_body(tmpl) };
    let body_str = unsafe { CStr::from_ptr(body) }.to_str().unwrap();
    assert!(body_str.contains("Body:"));
    unsafe {
        pt_free_string(body);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_json_single_shot() {
    let source = CString::new(
        "\
---
params: [name = str, count = int]
---
{{ name }}: {{ count }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let json = CString::new(r#"{"name":"Alice","count":42}"#).unwrap();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result =
        unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    assert!(!result.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Alice: 42");

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_json_allow_extra() {
    let source = CString::new(
        "\
---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

    let json = CString::new(r#"{"name":"world","extra":"ignored"}"#).unwrap();

    // Strict mode should fail with extra key.
    let mut render_err: *mut c_char = ptr::null_mut();
    let result =
        unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
    assert!(
        result.is_null(),
        "strict render should fail with extra params"
    );
    unsafe { pt_free_string(render_err) };

    // Allow-extra mode should succeed.
    let mut render_err2: *mut c_char = ptr::null_mut();
    let result2 =
        unsafe { pt_template_render_json(tmpl, json.as_ptr(), true, &raw mut render_err2) };
    assert!(render_err2.is_null());
    assert!(!result2.is_null());
    let result_str = unsafe { CStr::from_ptr(result2) }.to_str().unwrap();
    assert_eq!(result_str, "Hello world!");

    unsafe {
        pt_free_string(result2);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_json_non_object_error() {
    let source = CString::new(
        "\
---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

    let json = CString::new(r"[1, 2, 3]").unwrap();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result =
        unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
    assert!(result.is_null());
    assert!(!render_err.is_null());
    let err_str = unsafe { CStr::from_ptr(render_err) }.to_str().unwrap();
    assert!(
        err_str.contains("object"),
        "expected 'object' in error: {err_str}"
    );

    unsafe {
        pt_free_string(render_err);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_flexbuffers_single_shot() {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Params {
        name: String,
        count: i64,
    }

    let source = CString::new(
        "\
---
params: [name = str, count = int]
---
{{ name }}: {{ count }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let params = Params {
        name: "Alice".to_string(),
        count: 42,
    };
    let data = flexbuffers::to_vec(&params).unwrap();

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe {
        pt_template_render_flexbuffers(tmpl, data.as_ptr(), data.len(), false, &raw mut render_err)
    };
    assert!(render_err.is_null(), "expected no render error");
    assert!(!result.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Alice: 42");

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

/// Split a transported error string into its `(kind, message)` parts, mirroring
/// what a language binding does.
fn split_kind(err: *const c_char) -> (String, String) {
    let s = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
    match s.split_once(ERR_KIND_SEP) {
        Some((kind, msg)) => (kind.to_string(), msg.to_string()),
        None => (String::new(), s.to_string()),
    }
}

#[test]
fn test_render_error_carries_kind_prefix() {
    let source = CString::new(
        "\
---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    // Render with an empty context — the required `name` param is missing.
    let ctx = pt_context_new();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(result.is_null());
    assert!(!render_err.is_null());

    let (kind, msg) = split_kind(render_err);
    assert_eq!(kind, "missing_params", "unexpected kind in {msg:?}");
    assert!(!msg.is_empty(), "message should be present");

    unsafe {
        pt_free_string(render_err);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_compile_error_carries_syntax_kind() {
    // Missing frontmatter is a syntax error.
    let source = CString::new("no frontmatter here").unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(!err.is_null());
    assert!(tmpl.is_null());

    let (kind, _msg) = split_kind(err);
    assert_eq!(kind, "syntax");
    unsafe { pt_free_string(err) };
}

#[test]
fn test_render_empty() {
    let source = CString::new(
        "\
---
params: []
---
static output",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_empty(tmpl, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "static output");

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_unchecked() {
    let source = CString::new(
        "\
---
params: [name = str]
---
Hi {{ name }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let key = CString::new("name").unwrap();
    let val = CString::new("Ada").unwrap();
    let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_unchecked(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Hi Ada");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_cached() {
    let source = CString::new(
        "\
---
params: [x = str]
---
value={{ x }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let cache = pt_cache_new();
    let ctx = pt_context_new();
    let key = CString::new("x").unwrap();
    let val = CString::new("42").unwrap();
    let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_cached(tmpl, ctx, cache, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "value=42");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_cache_free(cache);
        pt_template_free(tmpl);
    }
}
