//! `render_ctx_into`, `render_into`, and `render_empty` tests.

use super::*;

// -- render_ctx_into --

#[test]
fn render_ctx_into_matches_render() {
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
Hello {{ name }}! Count: {{ count }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("count", 42_i64);

    let expected = tmpl.render_ctx(&ctx).unwrap();

    let mut output = String::new();
    tmpl.render_ctx_into(&ctx, &mut output).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn render_ctx_into_appends_to_existing() {
    let tmpl = Template::from_source(
        "---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "world");

    let mut output = String::from("Hello, ");
    tmpl.render_ctx_into(&ctx, &mut output).unwrap();
    assert_eq!(output, "Hello, world");
}

#[test]
fn render_ctx_into_allowing_extra_works() {
    let tmpl = Template::from_source(
        "---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "hello");
    ctx.set("extra", "ignored");

    let mut output = String::new();
    tmpl.render_ctx_into_allowing_extra(&ctx, &mut output)
        .unwrap();
    assert_eq!(output, "hello");
}

#[test]
fn render_ctx_into_validates_context() {
    let tmpl = Template::from_source(
        "---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();
    let ctx = Context::new(); // missing x

    let mut output = String::new();
    assert!(tmpl.render_ctx_into(&ctx, &mut output).is_err());
}

#[cfg(feature = "serde")]
#[test]
fn render_into_works() {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Params {
        name: String,
    }
    let tmpl = Template::from_source(
        "---
params: [name = str]
---
Hi {{ name }}",
    )
    .unwrap();
    let params = Params {
        name: "Charlie".into(),
    };
    let mut buf = String::new();
    tmpl.render_into(&params, &mut buf).unwrap();
    assert_eq!(buf, "Hi Charlie");
}

// -- render_empty ----------------------------------------------------------

#[test]
fn render_empty_no_params() {
    let tmpl = Template::from_source(
        r"---
params: []
---
Hello world!",
    )
    .unwrap();
    assert_eq!(tmpl.render_empty().unwrap(), "Hello world!");
}

#[test]
fn render_empty_all_defaults() {
    let tmpl = Template::from_source(
        r#"---
params:
  - greeting = str := "Hi"
  - count = int := 5
---
{{ greeting }} {{ count }}"#,
    )
    .unwrap();
    assert_eq!(tmpl.render_empty().unwrap(), "Hi 5");
}

#[test]
fn render_empty_with_consts() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - VERSION = str := "1.0"

params: []
---
v{{ VERSION }}"#,
    )
    .unwrap();
    assert_eq!(tmpl.render_empty().unwrap(), "v1.0");
}

#[test]
fn render_empty_required_params_fails() {
    let tmpl = Template::from_source(
        r"---
params:
  - name = str
---
Hello {{ name }}!",
    )
    .unwrap();
    let err = tmpl.render_empty().unwrap_err();
    assert!(
        err.to_string().contains("name"),
        "error should mention the missing param: {err}"
    );
}

#[test]
fn render_empty_mixed_defaults_and_required_fails() {
    let tmpl = Template::from_source(
        r#"---
params:
  - greeting = str := "Hi"
  - name = str
---
{{ greeting }} {{ name }}!"#,
    )
    .unwrap();
    let err = tmpl.render_empty().unwrap_err();
    assert!(
        err.to_string().contains("name"),
        "error should mention the missing param: {err}"
    );
}

#[test]
fn render_empty_into_works() {
    let tmpl = Template::from_source(
        r"---
params: []
---
Hello!",
    )
    .unwrap();
    let mut buf = String::from("prefix: ");
    tmpl.render_empty_into(&mut buf).unwrap();
    assert_eq!(buf, "prefix: Hello!");
}
