//! Blockquote prefix enforcement and tag stripping tests.

use std::sync::Arc;

use super::{
    test_helpers::{compile, compiled_render},
    *,
};
use crate::{compat::HashMap, context::Context, value::Value};

// -- blockquote prefix enforcement tests (validate_blockquote_prefix) ----
// Rule: `{% %}` at line start → MUST have `> ` prefix (compile error otherwise).
// `{{ }}` at line start → no prefix needed.
// `{% %}` mid-line → no prefix needed.
// Content text between `> {% %}` tags → no prefix needed.

#[test]
fn bare_if_at_line_start_rejected() {
    let err = compile("{% if show %}yes{% /if %}")
        .expect_err("bare {% if %} at line start must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote") || msg.contains("> "),
        "should mention blockquote prefix: {msg}"
    );
}

#[test]
fn bare_for_at_line_start_rejected() {
    let err = compile("{% for x in items %}{{ x }}{% /for %}")
        .expect_err("bare {% for %} at line start must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote") || msg.contains("> "),
        "should mention blockquote prefix: {msg}"
    );
}

#[test]
fn bare_raw_at_line_start_rejected() {
    let err = compile("{% raw %}literal{% /raw %}")
        .expect_err("bare {% raw %} at line start must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote") || msg.contains("> "),
        "should mention blockquote prefix: {msg}"
    );
}

#[test]
fn bare_match_at_line_start_rejected() {
    let err = compile("{% match x %}{% case A %}a{% /match %}")
        .expect_err("bare {% match %} at line start must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote") || msg.contains("> "),
        "should mention blockquote prefix: {msg}"
    );
}

#[test]
fn bare_stmt_on_second_line_rejected() {
    // First line is content (OK), second line is bare {% %} (rejected).
    let err = compile(
        r"hello
{% if show %}yes{% /if %}",
    )
    .expect_err("bare {% %} on any line must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote") || msg.contains("> "),
        "should mention blockquote prefix: {msg}"
    );
}

#[test]
fn expression_at_line_start_accepted() {
    // {{ }} at line start does NOT need > prefix.
    let mut ctx = Context::new();
    ctx.set("name", "world");
    let result = compiled_render("{{ name }}", &ctx).unwrap();
    assert_eq!(result, "world");
}

#[test]
fn expression_on_own_line_no_prefix_needed() {
    // Multiple lines with {{ }} at line start — all OK without >.
    let mut ctx = Context::new();
    ctx.set("a", "one");
    ctx.set("b", "two");
    let result = compiled_render(
        r"{{ a }}
{{ b }}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        r"one
two"
    );
}

#[test]
fn midline_stmt_no_prefix_needed() {
    // {% %} in the middle of a line (after text) does NOT need >.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("text{% if show %} yes{% /if %}", &ctx).unwrap();
    assert_eq!(result, "text yes");
}

#[test]
fn content_between_blockquoted_tags_no_prefix_needed() {
    // Content lines between > {% %} tags do NOT need > prefix.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"> {% if show %}

plain content

> {% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        "plain content
"
    );
}

#[test]
fn mixed_content_and_expressions_between_tags() {
    // Content and {{ }} between > {% %} tags — no > needed on content lines.
    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"> {% if show %}

Hello {{ name }}!
Goodbye.

> {% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        r"Hello Alice!
Goodbye.
"
    );
}

#[test]
fn multiline_for_body_no_prefix_on_content() {
    // For-loop body: content and {{ }} lines don't need >.
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Struct(Arc::new(HashMap::from([
                ("name".into(), Value::Str("a".into())),
                ("score".into(), Value::Int(10)),
            ]))),
            Value::Struct(Arc::new(HashMap::from([
                ("name".into(), Value::Str("b".into())),
                ("score".into(), Value::Int(20)),
            ]))),
        ])),
    );
    let result = compiled_render(
        r"> {% for item in items %}

- {{ item.name }}: {{ item.score }}

> {% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        r"- a: 10
- b: 20
"
    );
}

#[test]
fn nested_blocks_content_no_prefix() {
    // Nested > {% if %} inside > {% for %} — content lines still no prefix.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("x".into()),
            Value::Str("y".into()),
        ])),
    );
    let result = compiled_render(
        r"> {% for item in items %}

> {% if show %}

{{ item }}

> {% /if %}

> {% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        r"x
y
"
    );
}

#[test]
fn blockquote_on_content_line_preserved_in_output() {
    // A `> ` on a content line (not a {% %} line) is NOT stripped —
    // it's a real markdown blockquote and appears in the output.
    let ctx = Context::new();
    let result = compiled_render("> This is a quote.", &ctx).unwrap();
    assert_eq!(result, "> This is a quote.");
}

#[test]
fn indented_bare_stmt_rejected() {
    // Even with leading whitespace, a line whose first non-space char is `{%`
    // must be rejected.
    let err =
        compile("   {% if x %}y{% /if %}").expect_err("indented bare {% %} should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote") || msg.contains("> "),
        "should mention blockquote prefix: {msg}"
    );
}

// -- blockquote tag stripping tests --------------------------------------

#[test]
fn blockquote_if_compact() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"> {% if show %}

yes

> {% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        "yes
"
    );
}

#[test]
fn blockquote_if_spaced() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"> {% if show %}

yes

> {% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        "yes
"
    );
}

#[test]
fn blockquote_if_else() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(false));
    let result = compiled_render(
        r"> {% if show %}

yes

> {% else %}

no

> {% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        "no
"
    );
}

#[test]
fn blockquote_for_loop() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])),
    );
    let result = compiled_render(
        r"> {% for x in items %}

- {{ x }}

> {% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        r"- a
- b
"
    );
}

#[test]
fn blockquote_preserves_regular_blockquotes() {
    let ctx = Context::new();
    // A `>` line without `{%` should be preserved as-is.
    let result = compiled_render("> This is a regular blockquote.", &ctx).unwrap();
    assert_eq!(result, "> This is a regular blockquote.");
}

#[test]
fn blockquote_elif() {
    let mut ctx = Context::new();
    ctx.set("status", "paused");
    let result = compiled_render(
        r#"> {% if status == "active" %}

running

> {% elif status == "paused" %}

paused

> {% else %}

stopped

> {% /if %}"#,
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        "paused
"
    );
}

// -- additional blockquote stripping tests --------------------------------

#[test]
fn blockquote_if_false_renders_empty() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(false));
    let result = compiled_render(
        r"> {% if show %}

yes

> {% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "");
}

#[test]
fn blockquote_nested_if_in_for() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Struct(Arc::new(HashMap::from([
                ("name".into(), Value::Str("alpha".into())),
                ("show".into(), Value::Bool(true)),
            ]))),
            Value::Struct(Arc::new(HashMap::from([
                ("name".into(), Value::Str("beta".into())),
                ("show".into(), Value::Bool(false)),
            ]))),
        ])),
    );
    let result = compiled_render(
        r"> {% for item in items %}

> {% if item.show %}

{{ item.name }}

> {% /if %}

> {% /for %}",
        &ctx,
    )
    .unwrap();
    // Only "alpha" is shown (beta has show=false). The for-loop
    // body for beta emits nothing (if-false), so we get one item
    // plus a trailing newline from the body text.
    assert_eq!(
        result,
        "alpha
"
    );
}

#[test]
fn blockquote_stripping_is_idempotent() {
    // Running the blockquote-stripped form through compile should give
    // the same output as the blockquoted form.
    let blockquoted = r"> {% if show %}

yes

> {% /if %}";
    let stripped = r"> {% if show %}yes

> {% /if %}";
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result_bq = compiled_render(blockquoted, &ctx).unwrap();
    let result_plain = compiled_render(stripped, &ctx).unwrap();
    assert_eq!(result_bq, result_plain);
    assert_eq!(
        result_bq,
        "yes
"
    );
}

#[test]
fn blockquote_mixed_with_expressions() {
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"Hello {{ name }}!

> {% if show %}

Visible.

> {% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        r"Hello world!
Visible.
"
    );
}

#[test]
fn blockquote_with_trim_markers() {
    // Whitespace-control `{%-` / `-%}` should still work after stripping.
    // After blockquote stripping: `{%- if show -%}content\n{%- /if -%}`
    // `-%}` on the if-tag strips the leading whitespace/newline after it,
    // and `{%-` on the /if trims trailing whitespace before it.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"> {%- if show -%}

content

> {%- /if -%}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        "content
"
    );
}

#[test]
fn blockquote_for_single_item() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![Value::Str("only".into())])),
    );
    let result = compiled_render(
        r"> {% for x in items %}

{{ x }}

> {% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        "only
"
    );
}

#[test]
fn blockquote_for_empty_list() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    let result = compiled_render(
        r"> {% for x in items %}

{{ x }}

> {% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "");
}
