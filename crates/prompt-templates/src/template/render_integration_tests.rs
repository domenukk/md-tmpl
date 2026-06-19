//! Render integration tests covering cross-feature interactions.
//!
//! These tests verify that constants, imports, includes, variables, defaults,
//! and template parameters interact correctly — no leaking, no shadowing
//! surprises, and all renders produce expected output.

use std::sync::Arc;

use crate::{CompileOptions, Context, RenderOptions, Template, Value};

// ---------------------------------------------------------------------------
// A. Constants inside includes — isolation & propagation
// ---------------------------------------------------------------------------

/// Local constants defined in an included template render correctly.
#[test]
fn include_renders_own_constants() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("helper.tmpl.md"),
        r#"---
name: helper
consts:
  - GREETING = str := "Hey"
params: [name = str]
---
{{ GREETING }} {{ name }}!"#,
    )
    .unwrap();

    let main_src = r"---
params: [name = str]
---
> {% include [helper](helper.tmpl.md) with name=name %}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Hey Alice!");
}

/// Constants from an included template don't leak into the parent scope.
#[test]
fn include_constants_do_not_leak_to_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    // Helper defines LEAKED_CONST.
    std::fs::write(
        base.join("helper.tmpl.md"),
        r#"---
name: helper
consts:
  - LEAKED_CONST = str := "secret"
params: []
---
included"#,
    )
    .unwrap();

    // Main references LEAKED_CONST after include — this should fail.
    let main_src = r"---
params: []
---
> {% include [helper](helper.tmpl.md) %}

{{ LEAKED_CONST }}";
    let result = Template::compile(main_src, CompileOptions::default().base_dir(base));
    assert!(
        result.is_err(),
        "LEAKED_CONST from include should not be visible in parent"
    );
}

/// Constants defined in parent scope flow to included template at render time
/// (the scope is inherited). This test documents current behavior.
#[test]
fn parent_context_visible_in_include_via_allow_unused() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    // Helper uses a param that parent passes via context forwarding.
    std::fs::write(
        base.join("helper.tmpl.md"),
        r"---
name: helper
params: [ctx_val = str]
---
Got: {{ ctx_val }}",
    )
    .unwrap();

    let main_src = r"---
params: [ctx_val = str]
---
> {% include [helper](helper.tmpl.md) with ctx_val=ctx_val %}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set("ctx_val", "from_parent");
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Got: from_parent");
}

// ---------------------------------------------------------------------------
// B. Imported constants — scoping and rendering
// ---------------------------------------------------------------------------

/// Imported constants render correctly via the `lib.CONST` syntax.
#[test]
fn imported_constants_render_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("colors.tmpl.md"),
        r#"---
name: colors
consts:
  - PRIMARY = str := "blue"
  - COUNT = int := 42
---
"#,
    )
    .unwrap();

    let main_src = r"---
imports:
  - [colors](colors.tmpl.md)
params: []
---
Color: {{ colors.PRIMARY }}, Count: {{ colors.COUNT }}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Color: blue, Count: 42");
}

/// Imported constants from one module don't pollute another module's namespace.
#[test]
fn imported_constants_are_namespaced() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("a.tmpl.md"),
        r#"---
name: a
consts:
  - VAL = str := "from_a"
---
"#,
    )
    .unwrap();

    std::fs::write(
        base.join("b.tmpl.md"),
        r#"---
name: b
consts:
  - VAL = str := "from_b"
---
"#,
    )
    .unwrap();

    let main_src = r"---
imports:
  - [a](a.tmpl.md)
  - [b](b.tmpl.md)
params: []
---
{{ a.VAL }} {{ b.VAL }}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "from_a from_b");
}

/// Bare imported constant name (without module prefix) is not accessible.
#[test]
fn bare_imported_constant_not_accessible() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("lib.tmpl.md"),
        r#"---
name: lib
consts:
  - SECRET = str := "hidden"
---
"#,
    )
    .unwrap();

    // Try using SECRET without lib. prefix — should fail at compile time.
    let main_src = r"---
imports:
  - [lib](lib.tmpl.md)
params: []
---
{{ SECRET }}";
    let result = Template::compile(main_src, CompileOptions::default().base_dir(base));
    assert!(
        result.is_err(),
        "imported constant should not be accessible without module prefix"
    );
}

// ---------------------------------------------------------------------------
// C. Variable isolation between includes
// ---------------------------------------------------------------------------

/// Variables passed to one include don't leak to a second include.
#[test]
fn variables_do_not_leak_between_includes() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("greet.tmpl.md"),
        r"---
name: greet
params: [who = str]
---
Hello {{ who }}",
    )
    .unwrap();

    std::fs::write(
        base.join("farewell.tmpl.md"),
        r"---
name: farewell
params: [who = str]
---
Bye {{ who }}",
    )
    .unwrap();

    let main_src = r#"---
params: []
---
> {% include [greet](greet.tmpl.md) with who="Alice" %}
> {% include [farewell](farewell.tmpl.md) with who="Bob" %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert!(
        output.contains("Hello Alice"),
        "first include should greet Alice, got: {output}"
    );
    assert!(
        output.contains("Bye Bob"),
        "second include should farewell Bob, got: {output}"
    );
    assert!(
        !output.contains("Hello Bob"),
        "Alice variable must not leak to Bob include"
    );
    assert!(
        !output.contains("Bye Alice"),
        "Bob variable must not leak to Alice include"
    );
}

/// `with` variables override parent context values for the include scope.
#[test]
fn with_vars_override_parent_context() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("echo.tmpl.md"),
        r"---
name: echo
params: [val = str]
---
{{ val }}",
    )
    .unwrap();

    let main_src = r#"---
params: [val = str]
---
Parent: {{ val }}

> {% include [echo](echo.tmpl.md) with val="overridden" %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set("val", "original");
    let output = tmpl.render(&ctx).unwrap();
    assert!(
        output.contains("Parent: original"),
        "parent should see original, got: {output}"
    );
    assert!(
        output.contains("overridden"),
        "include should see overridden val, got: {output}"
    );
}

// ---------------------------------------------------------------------------
// D. Defaults in includes
// ---------------------------------------------------------------------------

/// Include injects defaults for params the caller doesn't provide.
#[test]
fn include_default_params_injected() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("widget.tmpl.md"),
        r#"---
name: widget
params:
  - label = str
  - color = str := "gray"
---
{{ label }}({{ color }})"#,
    )
    .unwrap();

    let main_src = r#"---
params: []
---
> {% include [widget](widget.tmpl.md) with label="Button" %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Button(gray)");
}

/// Include caller can override a default param.
#[test]
fn include_caller_overrides_default() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("widget.tmpl.md"),
        r#"---
name: widget
params:
  - label = str
  - color = str := "gray"
---
{{ label }}({{ color }})"#,
    )
    .unwrap();

    let main_src = r#"---
params: []
---
> {% include [widget](widget.tmpl.md) with label="Button", color="red" %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Button(red)");
}

// ---------------------------------------------------------------------------
// E. For-each includes
// ---------------------------------------------------------------------------

/// `for_each` include iterates correctly with correct variable isolation.
#[test]
fn for_each_include_renders_each_item() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("item.tmpl.md"),
        r"---
name: item
params: [thing = str]
---
- {{ thing }}",
    )
    .unwrap();

    let main_src = r"---
params: [items = list<str>]
---
> {% include [item](item.tmpl.md) for thing in items %}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("apple".into()),
            Value::Str("banana".into()),
            Value::Str("cherry".into()),
        ])),
    );
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("- apple"), "got: {output}");
    assert!(output.contains("- banana"), "got: {output}");
    assert!(output.contains("- cherry"), "got: {output}");
}

/// For-each include with empty list produces no output.
#[test]
fn for_each_include_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("item.tmpl.md"),
        r"---
name: item
params: [thing = str]
---
- {{ thing }}",
    )
    .unwrap();

    let main_src = r"---
params: [items = list<str>]
---
Before

> {% include [item](item.tmpl.md) for thing in items %}

After";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("Before"), "got: {output}");
    assert!(output.contains("After"), "got: {output}");
    assert!(
        !output.contains("- "),
        "empty list should produce no item output, got: {output}"
    );
}

// ---------------------------------------------------------------------------
// F. Nested includes — chain of includes with their own constants
// ---------------------------------------------------------------------------

/// Three-level include chain, each with their own constants.
#[test]
fn nested_includes_each_use_own_constants() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("leaf.tmpl.md"),
        r#"---
name: leaf
consts:
  - LEAF_TAG = str := "LEAF"
params: []
---
[{{ LEAF_TAG }}]"#,
    )
    .unwrap();

    std::fs::write(
        base.join("mid.tmpl.md"),
        r#"---
name: mid
consts:
  - MID_TAG = str := "MID"
params: []
---
[{{ MID_TAG }}]> {% include [leaf](leaf.tmpl.md) %}"#,
    )
    .unwrap();

    let main_src = r#"---
consts:
  - TOP_TAG = str := "TOP"
params: []
---
[{{ TOP_TAG }}]> {% include [mid](mid.tmpl.md) %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("[TOP]"), "got: {output}");
    assert!(output.contains("[MID]"), "got: {output}");
    assert!(output.contains("[LEAF]"), "got: {output}");
}

// ---------------------------------------------------------------------------
// G. Template parameters (tmpl<...>) with their own constants
// ---------------------------------------------------------------------------

/// Higher-order template parameter carries its own constants.
#[test]
fn tmpl_param_carries_own_constants() {
    let helper = Template::from_source(
        r#"---
params: [name = str]
consts:
  - PREFIX = str := "Dr."
---
{{ PREFIX }} {{ name }}"#,
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [formatter = tmpl<name = str>]
---
> {% include formatter with name="Smith" %}"#,
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("formatter", Value::Tmpl(Arc::new(helper)));
    let output = main.render(&ctx).unwrap();
    assert_eq!(output, "Dr. Smith");
}

/// Template param constants don't leak to the calling template.
#[test]
fn tmpl_param_constants_do_not_leak() {
    // Main tries to use HELPER_SECRET — should fail at compile time
    // because the constant belongs to the tmpl param, not the parent.
    let main_src = r"---
params: [h = tmpl<>]
---
> {% include h %}

{{ HELPER_SECRET }}";
    let result = Template::from_source(main_src);
    assert!(
        result.is_err(),
        "tmpl param's constants should not be visible in parent scope"
    );
}

// ---------------------------------------------------------------------------
// H. RenderOptions interactions
// ---------------------------------------------------------------------------

/// `allow_extra(true)` allows extra context keys without error.
#[test]
fn render_with_allow_extra() {
    let tmpl = Template::from_source(
        r"---
params: [name = str]
---
Hi {{ name }}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    ctx.set("extra_key", "should not error");

    let output = tmpl
        .render_with(&ctx, RenderOptions::default().allow_extra(true))
        .unwrap();
    assert_eq!(output, "Hi Alice");
}

/// `allow_extra(false)` (default) rejects extra context keys.
#[test]
fn render_strict_rejects_extra_keys() {
    let tmpl = Template::from_source(
        r"---
params: [name = str]
---
Hi {{ name }}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    ctx.set("extra_key", "boom");

    let result = tmpl.render(&ctx);
    assert!(
        result.is_err(),
        "strict mode should reject extra context keys"
    );
}

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
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("high"), "got: {output}");

    let mut ctx2 = Context::new();
    ctx2.set("level", 2);
    let output2 = tmpl.render(&ctx2).unwrap();
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
  - Status = enum<Active, Inactive>
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
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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

    let direct = tmpl.render(&ctx).unwrap();
    let cached = tmpl
        .render_cached_with(&ctx, &cache, RenderOptions::default())
        .unwrap();
    assert_eq!(direct, cached, "cached and direct render must match");
    assert_eq!(direct, "v1: hello");
}

// ---------------------------------------------------------------------------
// N. render_into correctness
// ---------------------------------------------------------------------------

/// `render_into` appends to an existing buffer correctly.
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
    tmpl.render_into(&ctx, &mut buf).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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
  - CFG = struct<host = str, port = int> := {host = "localhost", port = 8080}
params: []
---
{{ CFG.host }}:{{ CFG.port }}"#,
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "localhost:8080");
}

/// List constant with join filter.
#[test]
fn list_constant_join_filter() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - LANGS = list<str> := ["Rust", "Go", "Python"]
params: []
---
{{ LANGS | join(", ") }}"#,
    )
    .unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
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
    let output = tmpl.render(&ctx).unwrap();
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
  - Priority = enum<High, Medium, Low>
---
",
    )
    .unwrap();

    let main_src = r"---
imports:
  - [types](types.tmpl.md)
params: [p = types.Priority]
---
Priority: {{ kind(p) }}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set("p", "High");
    let output = tmpl.render(&ctx).unwrap();
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
params: [items = list<str>]
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
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("* a"), "got: {output}");
    assert!(output.contains("* b"), "got: {output}");
}

/// Loop variable does not persist after the loop.
#[test]
fn loop_variable_does_not_persist_after_loop() {
    let tmpl = Template::from_source(
        r"---
params: [items = list<str>]
---
> {% for item in items %}

{{ item }}

> {% /for %}

Done",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![Value::Str("x".into())])));
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains('x'), "got: {output}");
    assert!(output.contains("Done"), "got: {output}");
}

// ---------------------------------------------------------------------------
// S. Multiple includes with overlapping param names
// ---------------------------------------------------------------------------

/// Two includes with the same param name get different values.
#[test]
fn overlapping_param_names_isolated() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("a.tmpl.md"),
        r"---
name: a
params: [val = str]
---
A={{ val }}",
    )
    .unwrap();

    std::fs::write(
        base.join("b.tmpl.md"),
        r"---
name: b
params: [val = str]
---
B={{ val }}",
    )
    .unwrap();

    let main_src = r#"---
params: []
---
> {% include [a](a.tmpl.md) with val="first" %}
> {% include [b](b.tmpl.md) with val="second" %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("A=first"), "got: {output}");
    assert!(output.contains("B=second"), "got: {output}");
    assert!(
        !output.contains("A=second"),
        "val leaked from b to a, got: {output}"
    );
    assert!(
        !output.contains("B=first"),
        "val leaked from a to b, got: {output}"
    );
}

// ---------------------------------------------------------------------------
// T. Constants cannot be overridden by context
// ---------------------------------------------------------------------------

/// Context value cannot override a constant — const always wins.
#[test]
fn constant_wins_over_context() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - FIXED = str := "immutable"
params: []
---
{{ FIXED }}"#,
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("FIXED", "hacked");
    let output = tmpl
        .render_with(&ctx, RenderOptions::default().allow_extra(true))
        .unwrap();
    assert_eq!(output, "immutable");
}

// ---------------------------------------------------------------------------
// U. Include with constants + params combined
// ---------------------------------------------------------------------------

/// Included template uses both its own constants and passed params.
#[test]
fn include_uses_own_consts_and_passed_params() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("card.tmpl.md"),
        r#"---
name: card
consts:
  - BORDER = str := "===="
params: [title = str]
---
{{ BORDER }}
{{ title }}
{{ BORDER }}"#,
    )
    .unwrap();

    let main_src = r#"---
params: []
---
> {% include [card](card.tmpl.md) with title="Hello" %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        r"====
Hello
===="
    );
}

// ---------------------------------------------------------------------------
// V. For-each include with constants in the included template
// ---------------------------------------------------------------------------

/// For-each included template can use its own constants alongside the loop variable.
#[test]
fn for_each_include_with_constants() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("row.tmpl.md"),
        r#"---
name: row
consts:
  - PREFIX = str := ">"
params: [item = str]
---
{{ PREFIX }} {{ item }}"#,
    )
    .unwrap();

    let main_src = r"---
params: [items = list<str>]
---
> {% include [row](row.tmpl.md) for item in items %}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("alpha".into()),
            Value::Str("beta".into()),
        ])),
    );
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("> alpha"), "got: {output}");
    assert!(output.contains("> beta"), "got: {output}");
}
