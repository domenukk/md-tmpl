//! Context setters, JSON value conversion, and option values.

use std::ptr;

use md_tmpl::Value;

use crate::{json::json_to_value, metadata::value_to_json, *};

#[test]
fn test_context_set_int() {
    let source = CString::new(
        "\
---
params: [count = int]
---
Count: {{ count }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let key = CString::new("count").unwrap();
    let err = unsafe { pt_context_set_int(ctx, key.as_ptr(), 42) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Count: 42");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_context_set_bool() {
    let source = CString::new(
        "\
---
params: [flag = bool]
---
> {% if flag %}

yes

> {% else %}

no

> {% /if %}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let key = CString::new("flag").unwrap();
    let err = unsafe { pt_context_set_bool(ctx, key.as_ptr(), true) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "yes\n");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_context_set_json_list() {
    let source = CString::new(
        "\
---
params:
  - items = list(label = str)
---
> {% for item in items %}

{{ item.label }}

> {% /for %}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let key = CString::new("items").unwrap();
    let json = CString::new(r#"[{"label":"alpha"},{"label":"beta"}]"#).unwrap();
    let err = unsafe { pt_context_set_json(ctx, key.as_ptr(), json.as_ptr()) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "alpha\nbeta\n");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_json_to_value_dict() {
    let val = json_to_value(r#"{"name":"Alice","score":42}"#).unwrap();
    assert!(val.is_struct());
    assert_eq!(val.get_field("name").unwrap().as_str(), Some("Alice"));
    assert_eq!(val.get_field("score").unwrap().as_int(), Some(42));
}

#[test]
fn test_json_to_value_nested() {
    let val = json_to_value(r#"{"items":[{"label":"a"},{"label":"b"}]}"#).unwrap();
    let items = val.get_field("items").unwrap().as_list().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].get_field("label").unwrap().as_str(), Some("a"));
}

#[test]
fn test_context_set_float_render() {
    let source = CString::new(
        "\
---
params: [score = float]
---
{{ score }}",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let key = CString::new("score").unwrap();
    let err = unsafe { pt_context_set_float(ctx, key.as_ptr(), 3.25) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "3.25");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_context_set_tmpl_render() {
    // Template that takes a tmpl() param: card = tmpl(title = str)
    // and iterates over items, including card for each
    let card_source = CString::new(
        "\
---
name: card
params: [title = str]
---
* {{ title }}",
    )
    .unwrap();
    let mut card_tmpl: *mut PtTemplate = ptr::null_mut();
    unsafe { pt_template_from_source_allowing_unused(card_source.as_ptr(), &raw mut card_tmpl) };
    assert!(!card_tmpl.is_null());

    let main_source = CString::new(
        "\
---
params:
  - card = tmpl(title = str)
  - items = list(name = str)
---
> {% for item in items %}
> {% include card with title=item.name %}
> {% /for %}",
    )
    .unwrap();
    let mut main_tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(main_source.as_ptr(), &raw mut main_tmpl) };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let card_key = CString::new("card").unwrap();
    let err = unsafe { pt_context_set_tmpl(ctx, card_key.as_ptr(), card_tmpl) };
    assert!(err.is_null());

    let items_key = CString::new("items").unwrap();
    let items_json = CString::new(r#"[{"name":"Alpha"},{"name":"Beta"}]"#).unwrap();
    let err = unsafe { pt_context_set_json(ctx, items_key.as_ptr(), items_json.as_ptr()) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(main_tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null(), "render error: {:?}", unsafe {
        render_err.as_ref().map(|p| CStr::from_ptr(p))
    });
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert!(
        result_str.contains("Alpha"),
        "expected Alpha in output, got: {result_str}"
    );
    assert!(
        result_str.contains("Beta"),
        "expected Beta in output, got: {result_str}"
    );

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(main_tmpl);
        pt_template_free(card_tmpl);
    }
}

#[test]
fn test_value_to_json_roundtrip() {
    assert_eq!(value_to_json(&Value::Int(42)), "42");
    assert_eq!(value_to_json(&Value::Bool(true)), "true");
    assert_eq!(value_to_json(&Value::Bool(false)), "false");
    assert_eq!(value_to_json(&Value::Str("hello".into())), "\"hello\"");
    assert_eq!(
        value_to_json(&Value::Str("say \"hi\"\n".into())),
        "\"say \\\"hi\\\"\\n\""
    );
    assert_eq!(value_to_json(&Value::Float(3.5)), "3.5");
}

// -- option(T) tests --

#[test]
fn test_option_json_null_renders_none_via_match() {
    let source = CString::new(concat!(
        "\
---
params:
  - label = option(str)
---
",
        "> {% match label %}\n",
        "> {% case Some %}\n\n",
        "got:{{ label }}\n\n",
        "> {% case None %}\n\n",
        "empty\n\n",
        "> {% /match %}"
    ))
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let json = CString::new(r#"{"label":null}"#).unwrap();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result =
        unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str.trim(), "empty");

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_option_json_some_renders_value_via_match() {
    let source = CString::new(concat!(
        "\
---
params:
  - label = option(str)
---
",
        "> {% match label %}\n",
        "> {% case Some %}\n\n",
        "got:{{ label }}\n\n",
        "> {% case None %}\n\n",
        "empty\n\n",
        "> {% /match %}"
    ))
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let json = CString::new(r#"{"label":"hello"}"#).unwrap();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result =
        unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert!(
        result_str.contains("got:hello"),
        "expected 'got:hello', got '{result_str}'"
    );

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_option_json_null_via_has() {
    let source = CString::new(concat!(
        "\
---
params:
  - label = option(str)
---
",
        "> {% if has(label) %}\n\n",
        "got:{{ label }}\n\n",
        "> {% else %}\n\n",
        "empty\n\n",
        "> {% /if %}"
    ))
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let json = CString::new(r#"{"label":null}"#).unwrap();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result =
        unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str.trim(), "empty");

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_option_json_some_via_has() {
    let source = CString::new(concat!(
        "\
---
params:
  - label = option(str)
---
",
        "> {% if has(label) %}\n\n",
        "got:{{ label }}\n\n",
        "> {% else %}\n\n",
        "empty\n\n",
        "> {% /if %}"
    ))
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    let json = CString::new(r#"{"label":"world"}"#).unwrap();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result =
        unsafe { pt_template_render_json(tmpl, json.as_ptr(), false, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert!(
        result_str.contains("got:world"),
        "expected 'got:world', got '{result_str}'"
    );

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_option_set_none_direct() {
    let source = CString::new(concat!(
        "\
---
params:
  - label = option(str)
---
",
        "> {% if has(label) %}\n\n",
        "got:{{ label }}\n\n",
        "> {% else %}\n\n",
        "empty\n\n",
        "> {% /if %}"
    ))
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    // Use the direct pt_context_set_none API
    let ctx = pt_context_new();
    let key = CString::new("label").unwrap();
    let err = unsafe { pt_context_set_none(ctx, key.as_ptr()) };
    assert!(err.is_null(), "pt_context_set_none should succeed");

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str.trim(), "empty");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_option_set_none_then_some() {
    // Verify setting a value after None properly overrides
    let source = CString::new(concat!(
        "\
---
params:
  - label = option(str)
---
",
        "> {% if has(label) %}\n\n",
        "got:{{ label }}\n\n",
        "> {% else %}\n\n",
        "empty\n\n",
        "> {% /if %}"
    ))
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe { pt_template_from_source(source.as_ptr(), &raw mut tmpl) };
    assert!(err.is_null());

    // Set to None first, then override with a value
    let ctx = pt_context_new();
    let key = CString::new("label").unwrap();
    let err = unsafe { pt_context_set_none(ctx, key.as_ptr()) };
    assert!(err.is_null(), "pt_context_set_none should succeed");
    let val = CString::new("override").unwrap();
    let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
    assert!(err.is_null(), "pt_context_set_str should succeed");

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null(), "expected no render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert!(
        result_str.contains("got:override"),
        "expected 'got:override', got '{result_str}'"
    );

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}
