use std::ptr;

use md_tmpl::Value;

use super::*;
use crate::{json::json_to_value, metadata::value_to_json};

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
fn test_from_source_with_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("header.tmpl.md"),
        "\
---
name: header
params: [title = str]
---
# {{ title }}",
    )
    .unwrap();

    let source = CString::new(
        "\
---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body",
    )
    .unwrap();
    let base_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_base_dir(source.as_ptr(), base_dir.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null(), "error: {:?}", unsafe {
        err.as_ref().map(|p| CStr::from_ptr(p))
    });
    assert!(!tmpl.is_null());

    let ctx = pt_context_new();
    let key = CString::new("title").unwrap();
    let val = CString::new("Test").unwrap();
    unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert!(
        result_str.contains("Test"),
        "expected Test in: {result_str}"
    );

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_frontmatter() {
    let source = CString::new(
        "\
---
name: greeting
description: A greeting template
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let mut fm: *mut c_char = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
    };
    assert!(err.is_null(), "expected no error");
    assert!(!tmpl.is_null());
    assert!(!fm.is_null());

    let fm_str = unsafe { CStr::from_ptr(fm) }.to_str().unwrap();
    assert!(
        fm_str.contains("\"name\":\"greeting\""),
        "expected name in fm: {fm_str}"
    );
    assert!(
        fm_str.contains("\"description\":\"A greeting template\""),
        "expected desc in fm: {fm_str}"
    );
    assert!(
        fm_str.contains("\"has_params\":true"),
        "expected has_params in fm: {fm_str}"
    );
    assert!(
        fm_str.contains("\"allow_unused\":false"),
        "expected allow_unused in fm: {fm_str}"
    );

    // Verify template still works
    let ctx = pt_context_new();
    let key = CString::new("name").unwrap();
    let val = CString::new("World").unwrap();
    unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Hello World!");

    unsafe {
        pt_free_string(fm);
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_frontmatter_no_params() {
    let source = CString::new(
        "\
---
name: static
description: No params
params: []
---
Hello!",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let mut fm: *mut c_char = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
    };
    assert!(err.is_null());
    assert!(!tmpl.is_null());

    let fm_str = unsafe { CStr::from_ptr(fm) }.to_str().unwrap();
    assert!(
        fm_str.contains("\"name\":\"static\""),
        "expected name in fm: {fm_str}"
    );
    assert!(
        fm_str.contains("\"params\":[]"),
        "expected empty params in fm: {fm_str}"
    );

    unsafe {
        pt_free_string(fm);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_frontmatter_error() {
    let source = CString::new("no frontmatter at all").unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let mut fm: *mut c_char = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
    };
    assert!(!err.is_null(), "expected error for invalid source");
    assert!(tmpl.is_null(), "template should be null on error");
    unsafe { pt_free_string(err) };
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

// ─── env: support tests ──────────────────────────────────────────────────────

#[test]
fn test_from_source_with_env_basic() {
    let source = CString::new(
        "\
---
env:
  - API_KEY = str

params: [name = str]
---
key={{ API_KEY }} name={{ name }}",
    )
    .unwrap();
    let env_json = CString::new(r#"{"API_KEY":"secret123"}"#).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null(), "expected no error, got: {:?}", unsafe {
        err.as_ref().map(|p| CStr::from_ptr(p))
    });
    assert!(!tmpl.is_null());

    let ctx = pt_context_new();
    let key = CString::new("name").unwrap();
    let val = CString::new("Alice").unwrap();
    let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null(), "render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "key=secret123 name=Alice");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_env_int() {
    let source = CString::new(
        "\
---
env:
  - MAX_RETRIES = int

params: []
---
max={{ MAX_RETRIES }}",
    )
    .unwrap();
    let env_json = CString::new(r#"{"MAX_RETRIES":5}"#).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "max=5");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_env_default_override() {
    let source = CString::new(
        "\
---
env:
  - REGION = str := \"us-east-1\"

params: []
---
region={{ REGION }}",
    )
    .unwrap();
    let env_json = CString::new(r#"{"REGION":"eu-west-1"}"#).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "region=eu-west-1");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_env_default_fallback() {
    let source = CString::new(
        "\
---
env:
  - REGION = str := \"us-east-1\"

params: []
---
region={{ REGION }}",
    )
    .unwrap();
    // Empty env — should use default.
    let env_json = CString::new(r"{}").unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "region=us-east-1");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_env_missing_required() {
    let source = CString::new(
        "\
---
env:
  - REQUIRED_KEY = str

params: []
---
{{ REQUIRED_KEY }}",
    )
    .unwrap();
    // Missing required env var — should error at compile time.
    let env_json = CString::new(r"{}").unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(!err.is_null(), "expected compile error for missing env");
    assert!(tmpl.is_null());
    let err_str = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
    assert!(
        err_str.contains("REQUIRED_KEY"),
        "error should mention missing key: {err_str}"
    );
    unsafe { pt_free_string(err) };
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

#[test]
fn test_from_source_with_options_combines_env_and_allow_unused() {
    // Declares an unused param and a required env var — both options combined.
    let source = CString::new(
        "\
---
env:
  - REGION = str

params:
  - unused = str := \"x\"
---
region={{ REGION }}",
    )
    .unwrap();
    let env_json = CString::new(r#"{"REGION": "eu-west-1"}"#).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_options(
            source.as_ptr(),
            ptr::null(),
            env_json.as_ptr(),
            true,
            &raw mut tmpl,
        )
    };
    assert!(err.is_null(), "expected compile to succeed");
    assert!(!tmpl.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_empty(tmpl, &raw mut render_err) };
    assert!(render_err.is_null());
    assert_eq!(
        unsafe { CStr::from_ptr(result) }.to_str().unwrap(),
        "region=eu-west-1"
    );

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_options_unused_error_without_allow() {
    // Same template, but allow_unused=false — should fail to compile.
    let source = CString::new(
        "\
---
params: [unused = str]
---
static",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_options(
            source.as_ptr(),
            ptr::null(),
            ptr::null(),
            false,
            &raw mut tmpl,
        )
    };
    assert!(!err.is_null(), "expected unused-param error");
    assert!(tmpl.is_null());
    unsafe { pt_free_string(err) };
}

#[test]
fn test_from_file_with_options() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opts.tmpl.md");
    std::fs::write(
        &path,
        "\
---
params:
  - unused = str := \"x\"
---
from file",
    )
    .unwrap();

    let path_c = CString::new(path.to_str().unwrap()).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_file_with_options(
            path_c.as_ptr(),
            ptr::null(),
            ptr::null(),
            true,
            &raw mut tmpl,
        )
    };
    assert!(err.is_null(), "expected compile to succeed");
    assert!(!tmpl.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_empty(tmpl, &raw mut render_err) };
    assert!(render_err.is_null());
    assert_eq!(
        unsafe { CStr::from_ptr(result) }.to_str().unwrap(),
        "from file"
    );

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

// ─── cached include + compile-env regression tests ───────────────────────────

/// Write a parent template that includes a `PROMPTS_DIR`-driven child into
/// `dir`, returning the parent source. The child's `env:` var has no default,
/// so it must resolve from the parent's compile-time env on the cached path.
fn write_env_include_fixture(dir: &std::path::Path) -> CString {
    std::fs::write(
        dir.join("child.tmpl.md"),
        "\
---
name: child
env: [PROMPTS_DIR = str]
params: []
---
dir={{ PROMPTS_DIR }}",
    )
    .unwrap();
    CString::new(
        "\
---
params: []
---
> {% include [child](./child.tmpl.md) %}",
    )
    .unwrap()
}

#[test]
fn test_render_cached_include_resolves_compile_env() {
    // Regression: on the cached include path the child's env-only var must
    // resolve from the parent's compile env (mirrors render_ctx_cached). Before
    // the core fix this raised "no value provided and no default".
    let dir = tempfile::tempdir().unwrap();
    let source = write_env_include_fixture(dir.path());
    let base_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
    let env_json = CString::new(r#"{"PROMPTS_DIR":"/prompts"}"#).unwrap();

    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_options(
            source.as_ptr(),
            base_dir.as_ptr(),
            env_json.as_ptr(),
            false,
            &raw mut tmpl,
        )
    };
    assert!(err.is_null(), "compile error: {:?}", unsafe {
        err.as_ref().map(|p| CStr::from_ptr(p))
    });

    let cache = pt_cache_new();
    let ctx = pt_context_new();

    // First render compiles the include from disk with env threaded in.
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_cached(tmpl, ctx, cache, &raw mut render_err) };
    assert!(render_err.is_null(), "render error: {:?}", unsafe {
        render_err.as_ref().map(|p| CStr::from_ptr(p))
    });
    let first = unsafe { CStr::from_ptr(result) }
        .to_str()
        .unwrap()
        .to_string();
    assert!(first.contains("dir=/prompts"), "got: {first}");

    // Second render is served from cache; the env-injected const persists.
    let result2 = unsafe { pt_template_render_cached(tmpl, ctx, cache, &raw mut render_err) };
    assert!(render_err.is_null());
    assert_eq!(unsafe { CStr::from_ptr(result2) }.to_str().unwrap(), first);

    unsafe {
        pt_free_string(result);
        pt_free_string(result2);
        pt_context_free(ctx);
        pt_cache_free(cache);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_cached_include_invalidated_on_env_change() {
    // Regression: a shared cache must not serve a stale include when only the
    // compile env changed (file mtime + content are identical, which would
    // otherwise hit the fast path).
    let dir = tempfile::tempdir().unwrap();
    let source = write_env_include_fixture(dir.path());
    let base_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
    let cache = pt_cache_new();
    let ctx = pt_context_new();

    let render_with = |env_json: &str| -> String {
        let env = CString::new(env_json).unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe {
            pt_template_from_source_with_options(
                source.as_ptr(),
                base_dir.as_ptr(),
                env.as_ptr(),
                false,
                &raw mut tmpl,
            )
        };
        assert!(err.is_null(), "compile error: {:?}", unsafe {
            err.as_ref().map(|p| CStr::from_ptr(p))
        });
        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render_cached(tmpl, ctx, cache, &raw mut render_err) };
        assert!(render_err.is_null(), "render error: {:?}", unsafe {
            render_err.as_ref().map(|p| CStr::from_ptr(p))
        });
        let out = unsafe { CStr::from_ptr(result) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe {
            pt_free_string(result);
            pt_template_free(tmpl);
        }
        out
    };

    assert!(render_with(r#"{"PROMPTS_DIR":"/alpha"}"#).contains("dir=/alpha"));
    // Same file + cache, different env — must recompute, not serve /alpha.
    assert!(render_with(r#"{"PROMPTS_DIR":"/beta"}"#).contains("dir=/beta"));

    unsafe {
        pt_context_free(ctx);
        pt_cache_free(cache);
    }
}
