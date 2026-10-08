//! Rendering parity, nesting, and loop-function tests for the compiled engine.

use std::sync::Arc;

use super::{
    render::estimate_output_capacity,
    test_helpers::{assert_same, compile, compiled_render},
    *,
};
use crate::{compat::HashMap, context::Context, value::Value};

// -- parity tests: compiled vs legacy --

#[test]
fn parity_plain_text() {
    assert_same("Hello world!", &Context::new());
}

#[test]
fn parity_expression() {
    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    assert_same("Hello {{ name }}!", &ctx);
}

#[test]
fn parity_for_loop() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Struct(Arc::new(HashMap::from([(
                "label".into(),
                Value::Str("alpha".into()),
            )]))),
            Value::Struct(Arc::new(HashMap::from([(
                "label".into(),
                Value::Str("beta".into()),
            )]))),
        ])),
    );
    assert_same(
        r"> {% for item in items %}{{ idx(item) }}: {{ item.label }}

> {% /for %}",
        &ctx,
    );
}

#[test]
fn parity_nested_for_loops() {
    let mut ctx = Context::new();
    ctx.set(
        "groups",
        Value::List(Arc::new(vec![
            Value::Struct(Arc::new(HashMap::from([(
                "name".into(),
                Value::Str("G1".into()),
            )]))),
            Value::Struct(Arc::new(HashMap::from([(
                "name".into(),
                Value::Str("G2".into()),
            )]))),
        ])),
    );
    ctx.set(
        "tags",
        Value::List(Arc::new(vec![Value::Struct(Arc::new(HashMap::from([(
            "t".into(),
            Value::Str("A".into()),
        )])))])),
    );
    assert_same(
        r"> {% for g in groups %}[{{ g.name }}{% for t in tags %}:{{ t.t }}{% /for %}]

> {% /for %}",
        &ctx,
    );
}

#[test]
fn parity_if_true() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    assert_same("> {% if show %}visible{% /if %}", &ctx);
}

#[test]
fn parity_if_false() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(false));
    assert_same("> {% if show %}visible{% /if %}", &ctx);
}

#[test]
fn parity_if_else() {
    let mut ctx = Context::new();
    ctx.set("active", Value::Bool(false));
    assert_same("> {% if active %}Running{% else %}Stopped{% /if %}", &ctx);
}

#[test]
fn parity_nested_if() {
    let mut ctx = Context::new();
    ctx.set("a", Value::Bool(true));
    ctx.set("b", Value::Bool(false));
    assert_same(
        "> {% if a %}A{% if b %}B{% else %}notB{% /if %}{% /if %}",
        &ctx,
    );
}

#[test]
fn parity_raw_block() {
    assert_same("> {% raw %}{{ not_a_variable }}{% /raw %}", &Context::new());
}

#[test]
fn parity_mixed_content() {
    let mut ctx = Context::new();
    ctx.set("title", "Report");
    ctx.set("show_footer", Value::Bool(true));
    ctx.set(
        "items",
        Value::List(Arc::new(vec![Value::Struct(Arc::new(HashMap::from([(
            "name".into(),
            Value::Str("Item 1".into()),
        )])))])),
    );
    assert_same(
        r"# {{ title }}
> {% for item in items %}- {{ item.name }}

> {% /for %}{% if show_footer %}---
Footer{% /if %}",
        &ctx,
    );
}

#[test]
fn parity_filter_chain() {
    let mut ctx = Context::new();
    ctx.set("name", "  hello  ");
    assert_same("{{ name | trim | upper }}", &ctx);
}

#[test]
fn parity_filter_with_args() {
    let mut ctx = Context::new();
    ctx.set("score", Value::Float(1.2345));
    assert_same("{{ score | fixed(2) }}", &ctx);
}

// -- compiled-path-specific tests --

#[test]
fn for_inside_if() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ])),
    );
    let result = compiled_render(
        "> {% if show %}{% for item in items %}[{{ item }}]{% /for %}{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "[a][b]");
}

#[test]
fn if_inside_for() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Struct(Arc::new(HashMap::from([
                ("name".into(), Value::Str("visible".into())),
                ("show".into(), Value::Bool(true)),
            ]))),
            Value::Struct(Arc::new(HashMap::from([
                ("name".into(), Value::Str("hidden".into())),
                ("show".into(), Value::Bool(false)),
            ]))),
        ])),
    );
    let result = compiled_render(
        "> {% for item in items %}{% if item.show %}{{ item.name }}{% /if %}{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "visible");
}

#[test]
fn deeply_nested_for_loops() {
    let mut ctx = Context::new();
    ctx.set(
        "a",
        Value::List(Arc::new(vec![
            Value::Str("x".into()),
            Value::Str("y".into()),
        ])),
    );
    ctx.set(
        "b",
        Value::List(Arc::new(vec![
            Value::Str("1".into()),
            Value::Str("2".into()),
        ])),
    );
    ctx.set("c", Value::List(Arc::new(vec![Value::Str("!".into())])));
    let result = compiled_render(
        "> {% for ai in a %}{% for bi in b %}{% for ci in c %}{{ ai }}{{ bi }}{{ ci }}{% /for %}{% /for %}{% /for %}",
        &ctx,
    ).unwrap();
    assert_eq!(result, "x1!x2!y1!y2!");
}

#[test]
fn empty_template() {
    let result = compiled_render("", &Context::new()).unwrap();
    assert_eq!(result, "");
}

#[test]
fn for_non_list_errors() {
    let mut ctx = Context::new();
    ctx.set("items", "not a list");
    let err = compiled_render("> {% for item in items %}x{% /for %}", &ctx)
        .expect_err("iterating over non-list should fail");
    assert!(
        err.to_string().contains("not a list"),
        "should mention non-list type: {err}"
    );
}

#[test]
fn unexpected_closing_tag_errors() {
    let err = compile("> {% /for %}").expect_err("unexpected closing tag should fail");
    assert!(
        err.to_string().contains("unexpected"),
        "should mention unexpected closing tag: {err}"
    );
}

#[test]
fn legacy_endfor_rejected() {
    let err = compile("> {% endfor %}").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown statement"),
        "expected unknown statement error, got: {msg}"
    );
}

#[test]
fn legacy_endif_rejected() {
    let err = compile("> {% endif %}").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown statement"),
        "expected unknown statement error, got: {msg}"
    );
}

#[test]
fn legacy_endraw_rejected() {
    let err = compile("> {% endraw %}").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown statement"),
        "expected unknown statement error, got: {msg}"
    );
}

#[test]
fn estimate_capacity_returns_nonzero_for_static() {
    let (segs, _) = compile("Hello world!").unwrap();
    assert!(estimate_output_capacity(&segs) >= 12);
}

// -- loop functions: idx(binding), len(list) --

#[test]
fn idx_gives_zero_based_index() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
            Value::Str("c".into()),
        ])),
    );
    let result = compiled_render(
        "> {% for item in items %}{{ idx(item) }}:{{ item }} {% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "0:a 1:b 2:c ");
}

#[test]
fn idx_not_available_outside_loop() {
    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    // idx() on a non-loop variable returns None, falling through
    // to resolve_path which will fail on "idx(name)".
    let err = compiled_render("{{ idx(name) }}", &ctx).expect_err("idx() outside loop should fail");
    assert!(
        err.to_string()
            .contains("requires active loop binding 'name'"),
        "should mention active loop binding error: {err}"
    );
}

#[test]
fn nested_loops_independent_idx() {
    let mut ctx = Context::new();
    ctx.set(
        "outer",
        Value::List(Arc::new(vec![
            Value::Str("A".into()),
            Value::Str("B".into()),
        ])),
    );
    ctx.set(
        "inner",
        Value::List(Arc::new(vec![
            Value::Str("x".into()),
            Value::Str("y".into()),
        ])),
    );
    let result = compiled_render(
        "> {% for o in outer %}{% for i in inner %}{{ idx(i) }}{% /for %},{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "01,01,");
}

#[test]
fn nested_loops_outer_idx_accessible_from_inner() {
    let mut ctx = Context::new();
    ctx.set(
        "tasks",
        Value::List(Arc::new(vec![
            Value::Str("task1".into()),
            Value::Str("task2".into()),
        ])),
    );
    ctx.set(
        "tags",
        Value::List(Arc::new(vec![
            Value::Str("t1".into()),
            Value::Str("t2".into()),
        ])),
    );
    // From inside the inner loop, idx(task) should still resolve
    // the OUTER loop's index — that's the whole point.
    let result = compiled_render(
        "> {% for task in tasks %}{% for tag in tags %}{{ idx(task) }}.{{ idx(tag) }} {% /for %}{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "0.0 0.1 1.0 1.1 ");
}

#[test]
fn len_function_on_list() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
            Value::Str("c".into()),
        ])),
    );
    let result = compiled_render("{{ len(items) }}", &ctx).unwrap();
    assert_eq!(result, "3");
}

#[test]
fn bare_index_not_available() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![Value::Str("a".into())])));
    // Bare `index` should not resolve — use `{{ idx(item) }}`.
    let err = compiled_render("> {% for item in items %}{{ index }}{% /for %}", &ctx)
        .expect_err("bare 'index' should not resolve");
    assert!(
        err.to_string().contains("index") || err.to_string().contains("undefined"),
        "should mention unresolvable 'index': {err}"
    );
}

#[test]
fn len_function_on_string() {
    let mut ctx = Context::new();
    ctx.set("greeting", Value::Str("hello".into()));
    let result = compiled_render("{{ len(greeting) }}", &ctx).unwrap();
    assert_eq!(result, "5");
}

#[test]
fn len_function_on_struct_rejected() {
    let mut ctx = Context::new();
    let mut map = HashMap::new();
    map.insert("a".into(), Value::Int(1));
    map.insert("b".into(), Value::Int(2));
    ctx.set("data", Value::Struct(Arc::new(map)));
    let err =
        compiled_render("{{ len(data) }}", &ctx).expect_err("len() on struct should be rejected");
    assert!(
        err.to_string().contains("len() requires a list or string"),
        "error should mention correct types: {err}"
    );
}

#[test]
fn kinds_function_enum_variants() {
    let mut ctx = Context::new();
    let mut variant_map = HashMap::new();
    variant_map.insert(
        crate::consts::ENUM_VARIANTS_KEY.into(),
        Value::List(Arc::new(vec![
            Value::Str("Finding".into()),
            Value::Str("Hypothesis".into()),
            Value::Str("Question".into()),
        ])),
    );
    ctx.set("Tag", Value::Struct(Arc::new(variant_map)));
    let result = compiled_render("{{ kinds(Tag) | join(\", \") }}", &ctx).unwrap();
    assert_eq!(result, "Finding, Hypothesis, Question");
}

#[test]
fn for_loop_over_kinds() {
    let mut ctx = Context::new();
    let mut variant_map = HashMap::new();
    variant_map.insert(
        crate::consts::ENUM_VARIANTS_KEY.into(),
        Value::List(Arc::new(vec![
            Value::Str("Alpha".into()),
            Value::Str("Beta".into()),
        ])),
    );
    ctx.set("Stage", Value::Struct(Arc::new(variant_map)));
    let result = compiled_render("> {% for s in kinds(Stage) %}[{{ s }}]{% /for %}", &ctx).unwrap();
    assert_eq!(result, "[Alpha][Beta]");
}

#[test]
fn comment_stripped_from_output() {
    let ctx = Context::new();
    let result = compiled_render("before{# this is a comment #}after", &ctx).unwrap();
    assert_eq!(result, "beforeafter");
}

#[test]
fn struct_display_rejected_with_correct_type_name() {
    let mut ctx = Context::new();
    let mut map = HashMap::new();
    map.insert("x".into(), Value::Int(1));
    map.insert("y".into(), Value::Str("hello".into()));
    ctx.set("obj", Value::Struct(Arc::new(map)));
    let err = compiled_render("{{ obj }}", &ctx).expect_err("struct display should error");
    // Verify the error message says 'struct' (user-facing name), not 'dict' (internal name).
    assert!(
        err.to_string().contains("struct"),
        "error should mention 'struct': {err}"
    );
    assert!(
        !err.to_string().contains("dict"),
        "error should NOT mention 'dict': {err}"
    );
}
