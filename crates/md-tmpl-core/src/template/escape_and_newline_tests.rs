//! Escape-sequence render matrix and CRLF normalization tests.

use std::sync::Arc;

use super::*;
use crate::value::Value;

// =========================================================================
// Escape-sequence render matrix (S6–S7) and interpolation-composition (I1–I5).
// These lock in that `{{ }}` interpolation still composes with backslash
// escapes inside statement/expression string literals after unescaping lands.
// Frontmatter default strings remain literal (no interpolation) — that is
// covered by the frontmatter suite; here we only exercise `{% %}`/`{{ }}`.
// See scratch/md_tmpl_escapes_spec.md and the maintainer follow-up (I1–I5).
// =========================================================================

/// Compile a parent template whose body is `parent_body`, alongside a
/// `child.tmpl.md` file with the given contents, sharing a temp base dir.
fn compile_parent_with_child(
    parent_params: &str,
    parent_body: &str,
    child: &str,
) -> (Template, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("child.tmpl.md"), child).unwrap();
    let src = format!(
        "---
params: {parent_params}
---
{parent_body}"
    );
    let (tmpl, _) =
        Template::compile(&src, CompileOptions::default().base_dir(dir.path())).unwrap();
    (tmpl, dir)
}

#[test]
fn s6_escaped_quote_in_condition_literal() {
    // S6: a statement string literal `"a\"b"` parses; its value is `a"b`.
    let tmpl = Template::from_source(
        r#"---
params: [x = str]
---
> {% if x == "a\"b" %}yes{% /if %}"#,
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "a\"b");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "yes");
}

#[test]
fn s7_escape_and_interpolation_compose_in_condition() {
    // S7: `"x \" {{ v }}"` unescapes to `x " ` then interpolates `v`.
    let tmpl = Template::from_source(
        r#"---
params:
  - v = str
  - expected = str
---
> {% if "x \" {{ v }}" == expected %}ok{% /if %}"#,
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("v", "Z");
    ctx.set("expected", "x \" Z");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "ok");
}

#[test]
fn i1_escaped_quote_plus_interpolation_in_if() {
    // I1: escaped quotes AND an interpolation together in an `if` literal.
    let tmpl = Template::from_source(
        r#"---
params:
  - a = str
  - b = str
---
> {% if a == "say \"hi\" to {{ b }}" %}match{% /if %}"#,
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("a", "say \"hi\" to Z");
    ctx.set("b", "Z");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "match");
}

#[test]
fn i2_include_with_escaped_quote_and_interpolation() {
    // I2: an include `with` override that mixes interpolation and escaped
    // quotes renders the composed string.
    let (tmpl, _dir) = compile_parent_with_child(
        "[name = str]",
        r#"> {% include [child](./child.tmpl.md) with title="{{ name }}: \"quoted\"" %}"#,
        "---
params:
  - title = str
---
{{ title }}",
    );
    let mut ctx = Context::new();
    ctx.set("name", "Ada");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), r#"Ada: "quoted""#);
}

#[test]
fn i3_in_operator_with_escape_and_interpolation() {
    // I3: the `in` operator's left literal composes an escaped quote with an
    // interpolation, matching an element whose value contains a quote.
    let tmpl = Template::from_source(
        r#"---
params:
  - key = str
  - items = list(str)
---
> {% if "k_\"{{ key }}\"" in items %}found{% /if %}"#,
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("key", "a");
    ctx.set(
        "items",
        Value::List(Arc::new(vec![Value::Str("k_\"a\"".into())])),
    );
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "found");
}

#[test]
fn i4_interpolation_boundary_after_escaped_quote() {
    // I4: a leading escaped quote must not swallow the following `{{ x }}`
    // interpolation boundary — the literal is `"` + value of `x`.
    let tmpl = Template::from_source(
        r#"---
params:
  - x = str
  - expected = str
---
> {% if expected == "\"{{ x }}" %}ok{% /if %}"#,
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "Z");
    ctx.set("expected", "\"Z");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "ok");
}

#[test]
fn i5_plain_interpolation_still_works_without_escapes() {
    // I5: guard against scanner regressions — plain interpolation (no escapes)
    // must still work in `if`, `in`, and include `with`.

    // if
    let if_tmpl = Template::from_source(
        r#"---
params:
  - s = str
  - n = str
---
> {% if s == "hi {{ n }}" %}ok{% /if %}"#,
    )
    .unwrap();
    let mut if_ctx = Context::new();
    if_ctx.set("s", "hi Bob");
    if_ctx.set("n", "Bob");
    assert_eq!(if_tmpl.render_ctx(&if_ctx).unwrap(), "ok");

    // in
    let membership_tmpl = Template::from_source(
        r#"---
params:
  - k = str
  - items = list(str)
---
> {% if "v{{ k }}" in items %}found{% /if %}"#,
    )
    .unwrap();
    let mut membership_ctx = Context::new();
    membership_ctx.set("k", "A");
    membership_ctx.set(
        "items",
        Value::List(Arc::new(vec![Value::Str("vA".into())])),
    );
    assert_eq!(
        membership_tmpl.render_ctx(&membership_ctx).unwrap(),
        "found"
    );

    // include with
    let (with_tmpl, _dir) = compile_parent_with_child(
        "[name = str]",
        r#"> {% include [child](./child.tmpl.md) with title="hello {{ name }}" %}"#,
        "---
params:
  - title = str
---
{{ title }}",
    );
    let mut with_ctx = Context::new();
    with_ctx.set("name", "Ada");
    assert_eq!(with_tmpl.render_ctx(&with_ctx).unwrap(), "hello Ada");
}

// -- CRLF normalization ────────────────────────────────────────────────────

#[test]
fn crlf_from_source_produces_lf_output() {
    // Simulate a template read on Windows — the body has \r\n line endings.
    let source = "---\r\nparams: [name = str]\r\n---\r\nHello {{ name }}!\r\nGoodbye\r\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        !output.contains('\r'),
        "output must not contain \\r: {output:?}"
    );
    assert_eq!(output, "Hello world!\nGoodbye\n");
}

#[test]
fn crlf_compile_produces_lf_output() {
    let source = "---\r\nparams: [x = str]\r\n---\r\nLine1 {{ x }}\r\nLine2\r\n";
    let (tmpl, _fm) = Template::compile(source, CompileOptions::default()).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "val");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        !output.contains('\r'),
        "output must not contain \\r: {output:?}"
    );
    assert_eq!(output, "Line1 val\nLine2\n");
}

#[test]
fn crlf_from_file_produces_lf_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crlf.tmpl.md");
    // Write raw CRLF bytes.
    std::fs::write(
        &path,
        b"---\r\nparams: [msg = str]\r\n---\r\n{{ msg }}\r\nend\r\n",
    )
    .unwrap();
    let tmpl = Template::from_file(&path).unwrap();
    let mut ctx = Context::new();
    ctx.set("msg", "hi");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        !output.contains('\r'),
        "output must not contain \\r: {output:?}"
    );
    assert_eq!(output, "hi\nend\n");
}
