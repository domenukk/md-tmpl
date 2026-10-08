//! `for ... else` tests.

use std::sync::Arc;

use super::{
    render::estimate_output_capacity,
    test_helpers::{compile, compiled_render},
    *,
};
use crate::{context::Context, value::Value};

// -- for...else tests --

#[test]
fn for_else_empty_list() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    let result = compiled_render(
        "> {% for item in items %}{{ item }}{% else %}No items{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "No items");
}

#[test]
fn for_else_non_empty_list() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("Alice".into()),
            Value::Str("Bob".into()),
        ])),
    );
    let result = compiled_render(
        "> {% for item in items %}{{ item }}{% else %}No items{% /for %}",
        &ctx,
    )
    .unwrap();
    assert!(result.contains("Alice"));
    assert!(result.contains("Bob"));
    assert!(!result.contains("No items"));
}

#[test]
fn for_else_nested_outer_empty() {
    let mut ctx = Context::new();
    ctx.set("outer", Value::List(Arc::new(vec![])));
    ctx.set("inner", Value::List(Arc::new(vec![Value::Str("x".into())])));
    let result = compiled_render(
        "> {% for o in outer %}{% for i in inner %}{{ i }}{% else %}no inner{% /for %}{% else %}no outer{% /for %}",
        &ctx,
    )
    .unwrap();
    assert!(result.contains("no outer"));
    assert!(!result.contains("no inner"));
}

#[test]
fn for_else_nested_inner_empty() {
    let mut ctx = Context::new();
    ctx.set("outer", Value::List(Arc::new(vec![Value::Str("A".into())])));
    ctx.set("inner", Value::List(Arc::new(vec![])));
    let result = compiled_render(
        "> {% for o in outer %}{% for i in inner %}{{ i }}{% else %}no inner{% /for %}{% else %}no outer{% /for %}",
        &ctx,
    )
    .unwrap();
    assert!(result.contains("no inner"));
    assert!(!result.contains("no outer"));
}

#[test]
fn for_else_with_if_nesting() {
    // {% else %} inside {% if %} should NOT be confused with for-else.
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        "> {% for item in items %}{% if show %}{{ item }}{% else %}hidden{% /if %}{% else %}No items{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "No items");
}

#[test]
fn for_without_else_still_works() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])),
    );
    let result = compiled_render("> {% for item in items %}[{{ item }}]{% /for %}", &ctx).unwrap();
    assert_eq!(result, "[a][b]");
}

// -- for...else tests ------------------------------------------------

#[test]
fn compile_for_else_parses_else_body() {
    let (segs, _) =
        compile("> {% for item in items %}{{ item }}{% else %}empty{% /for %}").unwrap();
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Segment::ForLoop {
            binding,
            list_expr,
            filters,
            body,
            else_body,
        } => {
            assert_eq!(binding, "item");
            assert!(matches!(list_expr, CompiledExpr::Path(p) if p.as_str() == "items"));
            assert!(filters.is_empty());
            assert_eq!(body.len(), 1, "body should have expr segment");
            assert_eq!(else_body.len(), 1, "else_body should have static segment");
            assert!(
                matches!(&else_body[0], Segment::Static(s) if s == "empty"),
                "else_body should be 'empty', got {else_body:?}"
            );
        }
        other => panic!("expected ForLoop, got {other:?}"),
    }
}

#[test]
fn for_else_renders_body_when_list_non_empty() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])),
    );
    let result = compiled_render(
        "> {% for item in items %}[{{ item }}]{% else %}empty{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "[a][b]");
}

#[test]
fn for_else_renders_else_when_list_empty() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    let result = compiled_render(
        "> {% for item in items %}[{{ item }}]{% else %}No items found.{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "No items found.");
}

#[test]
fn for_else_renders_nothing_when_list_empty_and_no_else() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    let result = compiled_render("> {% for item in items %}[{{ item }}]{% /for %}", &ctx).unwrap();
    assert_eq!(result, "");
}

#[test]
fn for_else_with_expressions_in_else_body() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    ctx.set("fallback", "nothing here");
    let result = compiled_render(
        "> {% for item in items %}{{ item }}{% else %}{{ fallback }}{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "nothing here");
}

#[test]
fn for_else_nested_else_only_triggers_outer() {
    // Ensure {% else %} inside a nested {% for %} doesn't affect the outer loop.
    let mut ctx = Context::new();
    ctx.set(
        "outer",
        Value::List(Arc::new(vec![
            Value::Str("x".into()),
            Value::Str("y".into()),
        ])),
    );
    ctx.set("inner", Value::List(Arc::new(vec![])));
    let result = compiled_render(
        "> {% for o in outer %}[{{ o }}{% for i in inner %}{{ i }}{% else %}empty{% /for %}]{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "[xempty][yempty]");
}

#[test]
fn for_else_nested_inner_non_empty() {
    // Both loops have items — no else body should render.
    let mut ctx = Context::new();
    ctx.set("outer", Value::List(Arc::new(vec![Value::Str("A".into())])));
    ctx.set(
        "inner",
        Value::List(Arc::new(vec![
            Value::Str("1".into()),
            Value::Str("2".into()),
        ])),
    );
    let result = compiled_render(
        "> {% for o in outer %}{{ o }}:{% for i in inner %}{{ i }}{% else %}none{% /for %};{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "A:12;");
}

#[test]
fn for_else_with_nested_if_in_else() {
    // The else body can contain conditionals.
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    ctx.set("show_hint", Value::Bool(true));
    let result = compiled_render(
        "> {% for item in items %}{{ item }}{% else %}{% if show_hint %}Hint: list is empty{% /if %}{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "Hint: list is empty");
}

#[test]
fn for_else_empty_else_body_ignored_for_non_empty_list() {
    // When the list has items, the else body (even if present) is not rendered.
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![Value::Str("only".into())])),
    );
    let result = compiled_render(
        "> {% for item in items %}{{ item }}{% else %}SHOULD NOT APPEAR{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "only");
}

#[test]
fn for_else_idx_works_in_body_not_in_else() {
    // idx() should work in the loop body but not in the else body.
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])),
    );
    let result = compiled_render(
        "> {% for item in items %}{{ idx(item) }}:{{ item }} {% else %}no items{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "0:a 1:b ");
}

#[test]
fn for_else_estimate_capacity_includes_else() {
    let (segs, _) =
        compile("> {% for item in items %}{{ item }}{% else %}fallback text here{% /for %}")
            .unwrap();
    let cap = estimate_output_capacity(&segs);
    // Should include some capacity for the else_body.
    assert!(
        cap >= 18,
        "capacity should account for else_body, got {cap}"
    );
}
