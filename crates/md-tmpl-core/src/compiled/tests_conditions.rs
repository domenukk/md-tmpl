//! `elif` chain and `eval_condition` tests.

use std::sync::Arc;

use super::{
    test_helpers::{compile, compiled_render, eval_cond, refs},
    *,
};
use crate::{context::Context, value::Value};

// -- elif chain tests -------------------------------------------------------

#[test]
fn elif_first_branch_matches() {
    let mut ctx = Context::new();
    ctx.set("val", "A");
    let result = compiled_render(
        "> {% if val == 'A' %}first{% elif val == 'B' %}second{% else %}other{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "first");
}

#[test]
fn elif_middle_branch_matches() {
    let mut ctx = Context::new();
    ctx.set("val", "B");
    let result = compiled_render(
        "> {% if val == 'A' %}first{% elif val == 'B' %}second{% elif val == 'C' %}third{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "second");
}

#[test]
fn elif_falls_through_to_else() {
    let mut ctx = Context::new();
    ctx.set("val", "Z");
    let result = compiled_render(
        "> {% if val == 'A' %}first{% elif val == 'B' %}second{% else %}fallback{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "fallback");
}

#[test]
fn elif_all_false_no_else_renders_nothing() {
    let mut ctx = Context::new();
    ctx.set("val", "Z");
    let result = compiled_render(
        "> {% if val == 'A' %}first{% elif val == 'B' %}second{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "");
}

#[test]
fn elif_chain_with_nested_if() {
    // Ensure nested if blocks inside elif branches work correctly.
    let mut ctx = Context::new();
    ctx.set("val", "B");
    ctx.set("nested", Value::Bool(true));
    let result = compiled_render(
        "> {% if val == 'A' %}a{% elif val == 'B' %}{% if nested %}inner{% /if %}{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "inner");
}

#[test]
fn elif_refs_all_branches() {
    let r = refs("> {% if a == 'x' %}{{ b }}{% elif c == 'y' %}{{ d }}{% else %}{{ e }}{% /if %}");
    assert!(r.contains("a"));
    assert!(r.contains("b"));
    assert!(r.contains("c"));
    assert!(r.contains("d"));
    assert!(r.contains("e"));
}

#[test]
fn standalone_elif_rejected() {
    let err = compile("> {% elif x == 1 %}")
        .expect_err("standalone elif without preceding if should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("unexpected"),
        "standalone elif should be rejected: {msg}"
    );
}

// -- eval_condition comprehensive tests (ported from scope.rs) --------

#[test]
fn eval_condition_comprehensive() {
    let mut ctx = Context::new();
    ctx.set("outcome", "Confirmed");
    ctx.set("count", 5_i64);
    ctx.set("ratio", 1.23_f64);
    ctx.set(
        "items",
        Value::List(Arc::new(vec![Value::Int(10), Value::Int(20)])),
    );

    // String comparisons
    assert!(eval_cond("outcome == 'Confirmed'", &ctx).unwrap());
    assert!(eval_cond("outcome == \"Confirmed\"", &ctx).unwrap());
    assert!(!eval_cond("outcome == \"Other\"", &ctx).unwrap());
    assert!(eval_cond("outcome != \"Other\"", &ctx).unwrap());

    // Numeric inequalities
    assert!(eval_cond("count > 2", &ctx).unwrap());
    assert!(eval_cond("count >= 5", &ctx).unwrap());
    assert!(eval_cond("count < 10", &ctx).unwrap());
    assert!(eval_cond("count <= 5", &ctx).unwrap());
    assert!(!eval_cond("count > 5", &ctx).unwrap());

    // Float / Int mixed comparisons
    assert!(eval_cond("ratio > 1", &ctx).unwrap());
    assert!(eval_cond("ratio < 2", &ctx).unwrap());
    assert!(!eval_cond("ratio == 1", &ctx).unwrap());
}

#[test]
fn eval_condition_truthy_missing_variable() {
    let ctx = Context::new();
    let err = eval_cond("missing", &ctx).expect_err("condition on missing variable should fail");
    assert!(
        err.to_string().contains("missing") || err.to_string().contains("undefined"),
        "should mention the missing variable: {err}"
    );
}

#[test]
fn eval_condition_truthy_bool_values() {
    let mut ctx = Context::new();
    ctx.set("flag", true);
    ctx.set("off", false);
    assert!(eval_cond("flag", &ctx).unwrap());
    assert!(!eval_cond("off", &ctx).unwrap());
}

#[test]
fn condition_operators_inside_string_literal_not_split() {
    // REGRESSION: `||`, `&&`, and comparison operators embedded inside a quoted
    // string literal must be treated as literal characters by split_top_level /
    // find_top_level_op, not as boolean/comparison operators. Otherwise a
    // comparison against a string containing these tokens would be mis-parsed.
    let mut ctx = Context::new();
    ctx.set("label", "a || b && c == d");
    assert!(
        eval_cond("label == \"a || b && c == d\"", &ctx).unwrap(),
        "operators inside the quoted RHS must not fragment the condition"
    );
    assert!(
        !eval_cond("label == \"a || b\"", &ctx).unwrap(),
        "only the top-level comparison should be evaluated"
    );
    // Single-quoted literals must behave identically.
    assert!(eval_cond("label == 'a || b && c == d'", &ctx).unwrap());
}

#[test]
fn condition_escaped_quote_in_string_literal_not_split() {
    // REGRESSION: a backslash-escaped quote (`\"` / `\'`) must NOT terminate a
    // string literal during condition parsing. Otherwise `split_top_level` /
    // `find_top_level_op` would close the literal early and treat operators that
    // appear *after* the escaped quote (but still inside the string) as
    // top-level, fragmenting the condition. This mirrors `split_at_depth_zero`.
    let mut ctx = Context::new();
    ctx.set("label", "a\" && b || c");
    assert!(
        eval_cond("label == \"a\\\" && b || c\"", &ctx).unwrap(),
        "escaped quote must not close the literal early; trailing operators stay literal"
    );
    // A different RHS (literal ends right after the escaped quote's segment)
    // must compare unequal — proving the full literal was compared, not a
    // prematurely-closed prefix.
    assert!(!eval_cond("label == \"a\\\" && b\"", &ctx).unwrap());
    // Single-quoted literals with an escaped single quote behave identically.
    ctx.set("note", "x' || y");
    assert!(eval_cond("note == 'x\\' || y'", &ctx).unwrap());
}

#[test]
fn condition_in_template_with_len_function() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![Value::Int(10), Value::Int(20)])),
    );

    // len() in conditions — tested via compiled_render (full pipeline)
    assert_eq!(
        compiled_render("> {% if len(items) == 2 %}yes{% /if %}", &ctx).unwrap(),
        "yes"
    );
    assert_eq!(
        compiled_render("> {% if len(items) > 0 %}yes{% /if %}", &ctx).unwrap(),
        "yes"
    );
    assert_eq!(
        compiled_render("> {% if len(items) == 0 %}yes{% /if %}", &ctx).unwrap(),
        ""
    );
}
