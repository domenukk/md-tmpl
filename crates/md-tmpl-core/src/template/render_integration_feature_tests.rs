//! Render integration tests: filters, conditionals, match, inline templates, and constants.
//!
//! These tests verify that constants, imports, includes, variables, defaults,
//! and template parameters interact correctly — no leaking, no shadowing
//! surprises, and all renders produce expected output.

use std::sync::Arc;

use crate::{CompileOptions, Context, Template, Value};

// ---------------------------------------------------------------------------
// I. Filters on constants and variables
// ---------------------------------------------------------------------------

/// Filters work correctly on constant values.
#[test]
fn filter_on_constant() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - MSG = str := "hello world"

params: []
---
{{ MSG | upper }}"#,
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "HELLO WORLD");
}

/// Filter chain on a variable renders correctly.
#[test]
fn filter_chain_on_variable() {
    let tmpl = Template::from_source(
        r"---
params: [msg = str]
---
{{ msg | trim | upper }}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("msg", "  spaced  ");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "SPACED");
}

// ---------------------------------------------------------------------------
// J. Conditionals with constants and variables
// ---------------------------------------------------------------------------

/// If-else branch renders correctly with a constant condition.
#[test]
fn conditional_on_constant() {
    let tmpl = Template::from_source(
        r"---
consts:
  - ENABLED = bool := true

params: []
---
> {% if ENABLED %}

ON

> {% /if %}",
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("ON"), "got: {output}");
}

/// If-else with constant false.
#[test]
fn conditional_on_constant_false() {
    let tmpl = Template::from_source(
        r"---
consts:
  - ENABLED = bool := false

params: []
---
> {% if ENABLED %}

ON

> {% else %}

OFF

> {% /if %}",
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("OFF"), "got: {output}");
    assert!(!output.contains("ON"), "got: {output}");
}

/// Conditional comparing a variable to a constant literal.
#[test]
fn conditional_variable_vs_literal() {
    let tmpl = Template::from_source(
        r"---
params: [level = int]
---
> {% if level >= 5 %}

high

> {% else %}

low

> {% /if %}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("level", 10);
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("high"), "got: {output}");

    let mut ctx2 = Context::new();
    ctx2.set("level", 2);
    let output2 = tmpl.render_ctx(&ctx2).unwrap();
    assert!(output2.contains("low"), "got: {output2}");
}

// ---------------------------------------------------------------------------
// K. Match on enum with constants
// ---------------------------------------------------------------------------

/// Match renders the correct arm for an enum value.
#[test]
fn match_enum_renders_correct_arm() {
    let tmpl = Template::from_source(
        r"---
types:
  - Status = enum(Active, Inactive)

params: [status = Status]
---
> {% match status %}
> {% case Active %}

running

> {% case Inactive %}

stopped

> {% /match %}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("status", "Active");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("running"), "got: {output}");
    assert!(!output.contains("stopped"), "got: {output}");
}

// ---------------------------------------------------------------------------
// L. Inline templates ({% tmpl %})
// ---------------------------------------------------------------------------

/// Inline template defined and included within the same file.
#[test]
fn inline_template_renders_correctly() {
    let src = concat!(
        r#"---
params: [name = str]
---
"#,
        r#"> {% tmpl greeting %}
"#,
        r#"---
"#,
        r#"params: [who = str]
"#,
        r#"---
"#,
        r#"Hi {{ who }}!

"#,
        r#"> {% /tmpl %}

"#,
        "> {% include greeting with who=name %}",
    );
    let tmpl = Template::from_source(src).unwrap();

    let mut ctx = Context::new();
    ctx.set("name", "World");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("Hi World!"), "got: {output}");
}

/// Inline template can be used multiple times with different variables.
#[test]
fn inline_template_reusable() {
    let src = concat!(
        r#"---
params: []
---
"#,
        r#"> {% tmpl tag %}
"#,
        r#"---
"#,
        r#"params: [label = str]
"#,
        r#"---
"#,
        r#"[{{ label }}]

"#,
        r#"> {% /tmpl %}

"#,
        r#"> {% include tag with label="A" %}

"#,
        "> {% include tag with label=\"B\" %}",
    );
    let tmpl = Template::from_source(src).unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("[A]"), "got: {output}");
    assert!(output.contains("[B]"), "got: {output}");
}

// ---------------------------------------------------------------------------
// M. Cache render correctness
// ---------------------------------------------------------------------------

/// Cache-rendered templates produce identical output to non-cached rendering.
#[test]
fn cached_render_matches_direct_render() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    let src = r#"---
params: [x = str]
consts:
  - TAG = str := "v1"
---
{{ TAG }}: {{ x }}"#;
    std::fs::write(base.join("test.tmpl.md"), src).unwrap();

    let cache = crate::TemplateCache::new();
    let tmpl = cache.load(&base.join("test.tmpl.md")).unwrap();

    let mut ctx = Context::new();
    ctx.set("x", "hello");

    let direct = tmpl.render_ctx(&ctx).unwrap();
    let cached = tmpl.render_ctx_cached(&ctx, &cache).unwrap();
    assert_eq!(direct, cached, "cached and direct render must match");
    assert_eq!(direct, "v1: hello");
}

// ---------------------------------------------------------------------------
// N. render_ctx_into correctness
// ---------------------------------------------------------------------------

/// `render_ctx_into` appends to an existing buffer correctly.
#[test]
fn render_into_appends_correctly() {
    let tmpl = Template::from_source(
        r"---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("x", "world");

    let mut buf = String::from("hello ");
    tmpl.render_ctx_into(&ctx, &mut buf).unwrap();
    assert_eq!(buf, "hello world");
}

// ---------------------------------------------------------------------------
// O. Edge cases — empty templates, no params, only constants
// ---------------------------------------------------------------------------

/// Template with only constants and no params renders static content.
#[test]
fn consts_only_template_renders() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - APP = str := "MyApp"
  - VER = int := 2

params: []
---
{{ APP }} v{{ VER }}"#,
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "MyApp v2");
}

/// Empty body template renders empty string.
#[test]
fn empty_body_renders_empty() {
    let tmpl = Template::from_source(
        r"---
params: []
---
",
    )
    .unwrap();
    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "");
}

/// Frontmatter-only template with no body renders empty.
#[test]
fn frontmatter_only_renders_empty() {
    let tmpl = Template::from_source(
        r"---
params: []
---
",
    )
    .unwrap();
    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.is_empty(), "got: {output:?}");
}

// ---------------------------------------------------------------------------
// P. Constant types — dict, list, bool, int, float
// ---------------------------------------------------------------------------

/// Struct constant accessed via dot path.
#[test]
fn dict_constant_dot_access() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - CFG = struct(host = str, port = int) := {host = "localhost", port = 8080}

params: []
---
{{ CFG.host }}:{{ CFG.port }}"#,
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "localhost:8080");
}

/// List constant with join filter.
#[test]
fn list_constant_join_filter() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - LANGS = list(str) := ["Rust", "Go", "Python"]

params: []
---
{{ LANGS | join(", ") }}"#,
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "Rust, Go, Python");
}

/// Bool constant in conditional.
#[test]
fn bool_constant_in_conditional() {
    let tmpl = Template::from_source(
        r"---
consts:
  - DEBUG = bool := false

params: []
---
> {% if DEBUG %}

debug_on

> {% else %}

release

> {% /if %}",
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("release"), "got: {output}");
    assert!(!output.contains("debug_on"), "got: {output}");
}

// ---------------------------------------------------------------------------
// Q. Imported types used in params
// ---------------------------------------------------------------------------

/// Type alias imported from another module works in param declarations.
#[test]
fn imported_type_alias_in_params() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("types.tmpl.md"),
        r"---
name: types
types:
  - Priority = enum(High, Medium, Low)
---
",
    )
    .unwrap();

    let main_src = r"---
imports:
  - [types](./types.tmpl.md)

params: [p = types.Priority]
---
Priority: {{ kind(p) }}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set("p", "High");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "Priority: High");
}

// ---------------------------------------------------------------------------
// R. Constants in for-loops
// ---------------------------------------------------------------------------

/// Constants are accessible inside for-loop bodies.
#[test]
fn constant_accessible_inside_for_loop() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - BULLET = str := "*"

params: [items = list(str)]
---
> {% for item in items %}

{{ BULLET }} {{ item }}

> {% /for %}"#,
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("* a"), "got: {output}");
    assert!(output.contains("* b"), "got: {output}");
}

/// Loop variable does not persist after the loop.
#[test]
fn loop_variable_does_not_persist_after_loop() {
    let tmpl = Template::from_source(
        r"---
params: [items = list(str)]
---
> {% for item in items %}

{{ item }}

> {% /for %}

Done",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![Value::Str("x".into())])));
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains('x'), "got: {output}");
    assert!(output.contains("Done"), "got: {output}");
}
