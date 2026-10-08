//! Whitespace control (`-` trim marker) tests.

use std::sync::Arc;

use super::test_helpers::compiled_render;
use crate::{context::Context, value::Value};

// -- whitespace control tests -------------------------------------------

#[test]
fn trim_before_strips_preceding_whitespace() {
    // `{%-` should strip whitespace between previous content and the tag,
    // back to the previous newline (keeping the newline).
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"hello
> {%- if show %}yes{%- /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        r"hello
yes"
    );
}

#[test]
fn trim_after_strips_following_whitespace() {
    // `-%}` should strip whitespace after the tag up to and including
    // the next newline.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"> {% if show -%}

  content

> {% /if %}",
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
fn trim_expr_both_sides() {
    // `{{-` and `-}}` trim whitespace around expression tags.
    let mut ctx = Context::new();
    ctx.set("name", "world");
    let result = compiled_render(
        r"hello  {{- name -}}  
bye",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "helloworldbye");
}

#[test]
fn trim_combined_both_sides() {
    // `{%- -%}` trims whitespace on both sides of the opening tag.
    // Note: trim markers on the *closing* tag are handled by
    // find_closing_block which doesn't apply trim logic.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        r"begin

> {%- if show -%}

content{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(
        result,
        r"begin
content"
    );
}

#[test]
fn no_trim_without_dash_preserves_whitespace() {
    // Without `-`, whitespace is kept as-is (existing behavior).
    let mut ctx = Context::new();
    ctx.set("name", "world");
    let result = compiled_render("hello {{ name }} bye", &ctx).unwrap();
    assert_eq!(result, "hello world bye");
}

#[test]
fn trim_for_loop_with_whitespace_control() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])),
    );
    // `{%-` before for trims the preceding newline from "list:\n".
    // Inside the loop body, `{{- item -}}` trims around the expression.
    let result = compiled_render(
        r"list:{%- for item in items %}
- {{- item -}}

> {%- /for %}",
        &ctx,
    )
    .unwrap();
    // After the opening for tag (no -%}), \n is kept.
    // `{{- item -}}` strips surrounding whitespace around item value.
    // `{%- /for %}` — the `{%-` trims trailing whitespace before the close tag.
    assert_eq!(
        result,
        r"list:
-a
-b"
    );
}

#[test]
fn trim_before_at_start_of_input() {
    // `{%-` at the very beginning with no preceding text should not panic.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("> {%- if show %}yes{% /if %}", &ctx).unwrap();
    assert_eq!(result, "yes");
}

#[test]
fn trim_after_at_end_of_input() {
    // `-%}` at the very end with no following text should not panic.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("> {% if show %}yes{% /if -%}", &ctx).unwrap();
    assert_eq!(result, "yes");
}

#[test]
fn trim_expr_only_left() {
    // `{{-` trims only the left side.
    let mut ctx = Context::new();
    ctx.set("name", "world");
    let result = compiled_render("hello   {{- name }} bye", &ctx).unwrap();
    assert_eq!(result, "helloworld bye");
}

#[test]
fn trim_expr_only_right() {
    // `-}}` trims only the right side.
    let mut ctx = Context::new();
    ctx.set("name", "world");
    let result = compiled_render("hello {{ name -}}   bye", &ctx).unwrap();
    assert_eq!(result, "hello worldbye");
}
