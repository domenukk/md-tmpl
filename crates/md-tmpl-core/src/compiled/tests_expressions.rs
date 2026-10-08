//! `in` / `not in` operators, `panic` statements, and literal expression tests.

use std::sync::Arc;

use super::{
    test_helpers::{compile, compiled_render},
    *,
};
use crate::{compat::HashMap, context::Context, value::Value};

// -- M2: in and not in operators --

#[test]
fn in_operator_string_substring() {
    let mut ctx = Context::new();
    ctx.set("text", "foobar");
    let result =
        compiled_render("> {% if \"bar\" in text %}yes{% else %}no{% /if %}", &ctx).unwrap();
    assert_eq!(result, "yes");
    let result2 =
        compiled_render("> {% if \"baz\" in text %}yes{% else %}no{% /if %}", &ctx).unwrap();
    assert_eq!(result2, "no");
}

#[test]
fn not_in_operator_string_substring() {
    let mut ctx = Context::new();
    ctx.set("text", "foobar");
    let result = compiled_render(
        "> {% if !(\"baz\" in text) %}yes{% else %}no{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "yes");
    let result2 = compiled_render(
        "> {% if !(\"bar\" in text) %}yes{% else %}no{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result2, "no");
}

#[test]
fn in_operator_list_element() {
    let mut ctx = Context::new();
    ctx.set(
        "roles",
        Value::List(Arc::new(vec![
            Value::Str("admin".into()),
            Value::Str("user".into()),
        ])),
    );
    let result = compiled_render(
        "> {% if \"admin\" in roles %}yes{% else %}no{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "yes");
    let result2 = compiled_render(
        "> {% if \"guest\" in roles %}yes{% else %}no{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result2, "no");
}

#[test]
fn not_in_operator_list_element() {
    let mut ctx = Context::new();
    ctx.set(
        "roles",
        Value::List(Arc::new(vec![
            Value::Str("admin".into()),
            Value::Str("user".into()),
        ])),
    );
    let result = compiled_render(
        "> {% if !(\"guest\" in roles) %}yes{% else %}no{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "yes");
    let result2 = compiled_render(
        "> {% if !(\"admin\" in roles) %}yes{% else %}no{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result2, "no");
}

#[test]
fn in_and_not_in_enum_variants_kinds() {
    let mut ctx = Context::new();
    let mut variant_map = HashMap::new();
    variant_map.insert(
        crate::consts::ENUM_VARIANTS_KEY.into(),
        Value::List(Arc::new(vec![
            Value::Str("Design".into()),
            Value::Str("Review".into()),
        ])),
    );
    ctx.set("Stage", Value::Struct(Arc::new(variant_map)));

    let res_in = compiled_render(
        "> {% if \"Design\" in kinds(Stage) %}valid{% else %}invalid{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(res_in, "valid");

    let res_not_in = compiled_render(
        "> {% if !(\"Invalid\" in kinds(Stage)) %}absent{% else %}present{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(res_not_in, "absent");
}

#[test]
fn in_operator_type_mismatch_evaluates_to_false() {
    let mut ctx = Context::new();
    ctx.set("num", Value::Int(123));
    let result =
        compiled_render("> {% if \"foo\" in num %}yes{% else %}no{% /if %}", &ctx).unwrap();
    assert_eq!(result, "no");

    let result_not_in =
        compiled_render("> {% if !(\"foo\" in num) %}yes{% else %}no{% /if %}", &ctx).unwrap();
    assert_eq!(result_not_in, "yes");
}

// -- M2: panic statements --

#[test]
fn panic_statement_literal_string_halts_rendering() {
    let ctx = Context::new();
    let err = compiled_render("> {% panic(\"fatal error\") %}", &ctx)
        .expect_err("panic should halt rendering");
    assert!(
        matches!(err, TemplateError::Panic(ref s) if s == "fatal error"),
        "expected TemplateError::Panic(\"fatal error\"), got {err:?}"
    );
}

#[test]
fn panic_statement_variable_interpolation() {
    let mut ctx = Context::new();
    ctx.set("err_msg", "unsupported operation occurred");
    let err =
        compiled_render("> {% panic(err_msg) %}", &ctx).expect_err("panic should halt rendering");
    assert!(
        matches!(err, TemplateError::Panic(ref s) if s == "unsupported operation occurred"),
        "expected TemplateError::Panic(\"unsupported operation occurred\"), got {err:?}"
    );
}

// -- D5: literal expressions in general positions ----------------------------
//
// A literal must be valid wherever a variable of the same type is valid
// (output, filter input, ...), and must parse identically in every position.
// `CompiledExpr::compile` and `ConditionOperand::compile` share
// `parse_number_literal`, so these tests lock the behavior against drift.

#[test]
fn literal_string_renders_in_output_position() {
    let ctx = Context::new();
    assert_eq!(compiled_render("{{ \"hi\" }}", &ctx).unwrap(), "hi");
}

#[test]
fn literal_bool_renders_in_output_position() {
    let ctx = Context::new();
    assert_eq!(compiled_render("{{ true }}", &ctx).unwrap(), "true");
    assert_eq!(compiled_render("{{ false }}", &ctx).unwrap(), "false");
}

#[test]
fn literal_int_renders_in_output_position() {
    let ctx = Context::new();
    assert_eq!(compiled_render("{{ 42 }}", &ctx).unwrap(), "42");
    assert_eq!(compiled_render("{{ -7 }}", &ctx).unwrap(), "-7");
}

#[test]
fn literal_float_renders_in_output_position() {
    let ctx = Context::new();
    assert_eq!(compiled_render("{{ 3.14 }}", &ctx).unwrap(), "3.14");
    // Whole-valued floats render without a trailing `.0`, matching the TS backend.
    assert_eq!(compiled_render("{{ 3.0 }}", &ctx).unwrap(), "3");
}

#[test]
fn literal_compiles_to_literal_expr() {
    let (segs, _) = compile("{{ 42 }}").unwrap();
    assert!(
        matches!(
            &segs[0],
            Segment::Expr {
                expr: CompiledExpr::Literal(Value::Int(42)),
                filters,
            } if filters.is_empty()
        ),
        "expected a literal int expr, got {:?}",
        segs[0]
    );
}

#[test]
fn literal_string_is_valid_filter_input() {
    let ctx = Context::new();
    assert_eq!(compiled_render("{{ \"hi\" | upper }}", &ctx).unwrap(), "HI");
}

#[test]
fn literal_parses_consistently_in_condition_and_output() {
    // The same integer literal drives a comparison (ConditionOperand) and an
    // output position (CompiledExpr); both must accept it identically.
    let ctx = Context::new();
    assert_eq!(
        compiled_render("> {% if 1 == 1 %}{{ 1 }}{% /if %}", &ctx).unwrap(),
        "1"
    );
}

// -- D5: numeric-literal normalization (leading / negative zero) -------------

#[test]
fn numeric_literal_leading_zeros_normalized() {
    let ctx = Context::new();
    assert_eq!(compiled_render("{{ 007 }}", &ctx).unwrap(), "7");
    assert_eq!(compiled_render("{{ 00 }}", &ctx).unwrap(), "0");
}

#[test]
fn numeric_literal_negative_zero_normalized() {
    let ctx = Context::new();
    assert_eq!(compiled_render("{{ -0 }}", &ctx).unwrap(), "0");
    assert_eq!(compiled_render("{{ -0.0 }}", &ctx).unwrap(), "0");
}

// -- D5: strict numeric grammar rejects malformed numerics -------------------
//
// Tokens outside the shared grammar `^-?[0-9]+(?:\.[0-9]+)?$` are not literals:
// they fall through to a path lookup and error (undefined variable) rather than
// being silently accepted. This matches the TS backend's error-vs-ok
// classification (the exact phase — compile vs render — may differ, which is
// acceptable; the requirement is that they never succeed and never panic).

#[test]
fn malformed_numeric_literals_error() {
    let ctx = Context::new();
    for garbage in [
        "{{ 3. }}",
        "{{ .5 }}",
        "{{ 3.1.4 }}",
        "{{ 1e3 }}",
        "{{ 0x10 }}",
        "{{ +5 }}",
    ] {
        // Each malformed literal must error (at compile or render); `expect_err`
        // both asserts that and consumes the error value, so nothing is silently
        // discarded while the per-input diagnostic is preserved.
        compiled_render(garbage, &ctx).expect_err(&format!(
            "expected `{garbage}` to error (not a valid literal), but it rendered"
        ));
    }
}
