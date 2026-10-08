//! Declaration, default, const, and name/description metadata accessors.

use std::ptr;

use crate::*;

#[test]
fn test_declarations() {
    let source = CString::new(
        "\
---
params: [name = str, count = int]
---
{{ name }} {{ count }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    let declarations_raw = unsafe { pt_template_declarations(tmpl) };
    let declarations_text = unsafe { CStr::from_ptr(declarations_raw) }
        .to_str()
        .unwrap();
    assert!(declarations_text.contains("name"));
    assert!(declarations_text.contains("str"));
    assert!(declarations_text.contains("count"));
    assert!(declarations_text.contains("int"));
    unsafe {
        pt_free_string(declarations_raw);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_defaults_json() {
    let source = CString::new(
        "\
---
params:
  - name = str := \"World\"
  - count = int := 5
  - flag = bool
---
{{ name }} {{ count }} {{ flag }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(!tmpl.is_null());

    let raw_cstr = unsafe { pt_template_defaults_json(tmpl) };
    let text = unsafe { CStr::from_ptr(raw_cstr) }.to_str().unwrap();
    assert!(
        text.contains("\"name\": \"World\""),
        "expected name default in JSON: {text}"
    );
    assert!(
        text.contains("\"count\": 5"),
        "expected count default in JSON: {text}"
    );
    assert!(
        !text.contains("flag"),
        "flag should not appear in defaults: {text}"
    );

    unsafe {
        pt_free_string(raw_cstr);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_defaults_json_empty() {
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

    let raw_cstr = unsafe { pt_template_defaults_json(tmpl) };
    let text = unsafe { CStr::from_ptr(raw_cstr) }.to_str().unwrap();
    assert_eq!(text, "{}");

    unsafe {
        pt_free_string(raw_cstr);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_consts_json() {
    let source = CString::new(
        "\
---
consts:
  - MAX = int := 100
  - GREETING = str := \"hello\"

params: []
---
{{ MAX }} {{ GREETING }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(!tmpl.is_null());

    let raw_cstr = unsafe { pt_template_consts_json(tmpl) };
    let text = unsafe { CStr::from_ptr(raw_cstr) }.to_str().unwrap();
    assert!(
        text.contains("\"MAX\": 100"),
        "expected MAX const in JSON: {text}"
    );
    assert!(
        text.contains("\"GREETING\": \"hello\""),
        "expected GREETING const in JSON: {text}"
    );

    unsafe {
        pt_free_string(raw_cstr);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_consts_json_empty() {
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

    let raw_cstr = unsafe { pt_template_consts_json(tmpl) };
    let text = unsafe { CStr::from_ptr(raw_cstr) }.to_str().unwrap();
    assert_eq!(text, "{}");

    unsafe {
        pt_free_string(raw_cstr);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_defaults_context() {
    let source = CString::new(
        "\
---
params:
  - name = str := \"World\"
  - greeting = str
---
{{ greeting }} {{ name }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(!tmpl.is_null());

    let ctx = unsafe { pt_template_defaults_context(tmpl) };
    assert!(!ctx.is_null());

    // Set the non-default param
    let key = CString::new("greeting").unwrap();
    let val = CString::new("Hello").unwrap();
    let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null(), "render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Hello World");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_defaults_context_override() {
    let source = CString::new(
        "\
---
params:
  - name = str := \"World\"
---
{{ name }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

    let ctx = unsafe { pt_template_defaults_context(tmpl) };
    let key = CString::new("name").unwrap();
    let val = CString::new("Alice").unwrap();
    unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Alice");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_validate_declarations_match() {
    let source = CString::new(
        "\
---
params: [name = str, count = int]
---
{{ name }} {{ count }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(!tmpl.is_null());

    // Same declarations should validate
    let expected = CString::new(r#"[["name","str"],["count","int"]]"#).unwrap();
    let err = unsafe { pt_template_validate_declarations(tmpl, expected.as_ptr()) };
    assert!(err.is_null(), "expected matching declarations");

    unsafe { pt_template_free(tmpl) };
}

#[test]
fn test_validate_declarations_mismatch_retyped() {
    let source = CString::new(
        "\
---
params: [name = str]
---
{{ name }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

    // Different type: expect "retyped"
    let expected = CString::new(r#"[["name","int"]]"#).unwrap();
    let err = unsafe { pt_template_validate_declarations(tmpl, expected.as_ptr()) };
    assert!(!err.is_null(), "expected mismatch error");
    let err_str = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
    assert!(
        err_str.contains("retyped"),
        "expected retyped in error: {err_str}"
    );

    unsafe {
        pt_free_string(err);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_validate_declarations_mismatch_added() {
    let source = CString::new(
        "\
---
params: [name = str, count = int]
---
{{ name }} {{ count }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };

    // Expected has fewer params — template has "added" count
    let expected = CString::new(r#"[["name","str"]]"#).unwrap();
    let err = unsafe { pt_template_validate_declarations(tmpl, expected.as_ptr()) };
    assert!(!err.is_null(), "expected mismatch error for added param");
    let err_str = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
    assert!(
        err_str.contains("added"),
        "expected 'added' in error: {err_str}"
    );

    unsafe {
        pt_free_string(err);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_name_and_description_present() {
    let source = CString::new(
        "\
---
name: greeting
description: A greeting template
params: []
---
hi",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let name = unsafe { pt_template_name(tmpl) };
    assert!(!name.is_null());
    assert_eq!(
        unsafe { CStr::from_ptr(name) }.to_str().unwrap(),
        "greeting"
    );

    let desc = unsafe { pt_template_description(tmpl) };
    assert!(!desc.is_null());
    assert_eq!(
        unsafe { CStr::from_ptr(desc) }.to_str().unwrap(),
        "A greeting template"
    );

    unsafe {
        pt_free_string(name);
        pt_free_string(desc);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_name_and_description_absent_return_null() {
    let source = CString::new(
        "\
---
params: []
---
hi",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    assert!(unsafe { pt_template_name(tmpl) }.is_null());
    assert!(unsafe { pt_template_description(tmpl) }.is_null());

    unsafe { pt_template_free(tmpl) };
}
