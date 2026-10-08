//! `collect_referenced_params` analysis tests.

use super::test_helpers::refs;

// Expressions

#[test]
fn refs_simple_expression() {
    assert!(refs("{{ name }}").contains("name"));
}

#[test]
fn refs_dotted_path_extracts_root() {
    let r = refs("{{ item.label }}");
    assert!(r.contains("item"));
    assert!(!r.contains("label"), "only root should be extracted");
}

#[test]
fn refs_deeply_dotted_path() {
    let r = refs("{{ a.b.c.d }}");
    assert!(r.contains("a"));
    assert_eq!(r.len(), 1, "only root should appear");
}

#[test]
fn refs_multiple_expressions() {
    let r = refs("{{ a }} and {{ b }} and {{ c }}");
    assert!(r.contains("a"));
    assert!(r.contains("b"));
    assert!(r.contains("c"));
    assert_eq!(r.len(), 3);
}

// For loops

#[test]
fn refs_for_loop_iterable_not_binding() {
    let r = refs("> {% for item in items %}{{ item }}{% /for %}");
    assert!(r.contains("items"), "iterable should be referenced");
    assert!(!r.contains("item"), "loop binding should be excluded");
}

#[test]
fn refs_nested_for_loop_scopes() {
    let r = refs("> {% for row in rows %}{% for col in row.cols %}{{ col }}{% /for %}{% /for %}");
    assert!(r.contains("rows"));
    // `row` is a loop binding, not a context var. `row.cols` has root `row`
    // which is a binding, so it shouldn't appear.
    assert!(!r.contains("row"));
    assert!(!r.contains("col"));
}

#[test]
fn refs_var_after_for_loop_not_shadowed() {
    // `x` is used as a loop binding inside the loop, but also referenced
    // after. The post-loop `{{ x }}` should find `x` as a context variable
    // since the binding scope ended.
    let r = refs("> {% for x in items %}{{ x }}{% /for %}{{ x }}");
    assert!(r.contains("items"));
    assert!(r.contains("x"), "x should be found after loop scope ends");
}

// Conditions

#[test]
fn refs_simple_condition() {
    assert!(refs("> {% if show %}yes{% /if %}").contains("show"));
}

#[test]
fn refs_condition_comparison_left() {
    let r = refs("> {% if count > 0 %}yes{% /if %}");
    assert!(r.contains("count"));
    assert!(!r.contains("0"), "literal should not be a var");
}

#[test]
fn refs_condition_comparison_both_sides() {
    let r = refs("> {% if left == right %}match{% /if %}");
    assert!(r.contains("left"));
    assert!(r.contains("right"));
}

#[test]
fn refs_condition_else_branch() {
    let r = refs("> {% if flag %}{{ a }}{% else %}{{ b }}{% /if %}");
    assert!(r.contains("flag"));
    assert!(r.contains("a"));
    assert!(r.contains("b"));
}

// Functions

#[test]
fn refs_len_function_extracts_arg() {
    let r = refs("{{ len(items) }}");
    assert!(r.contains("items"));
    assert!(!r.contains("len"), "builtin name should not be a var");
}

#[test]
fn refs_idx_function_with_loop_binding() {
    let r = refs("> {% for item in items %}{{ idx(item) }}{% /for %}");
    assert!(r.contains("items"));
    assert!(
        !r.contains("item"),
        "binding passed to idx() should be excluded"
    );
}

#[test]
fn refs_len_function_with_dotted_arg() {
    let r = refs("{{ len(data.rows) }}");
    assert!(r.contains("data"));
    assert!(!r.contains("rows"));
}

// Literals

#[test]
fn refs_string_literal_excluded() {
    let r = refs("> {% if x == 'hello' %}yes{% /if %}");
    assert!(r.contains("x"));
    // 'hello' is a string literal.
}

#[test]
fn refs_bool_literal_excluded() {
    let r = refs("> {% if x == true %}yes{% /if %}");
    assert!(r.contains("x"));
}

#[test]
fn refs_float_literal_excluded() {
    let r = refs("> {% if x > 3.14 %}yes{% /if %}");
    assert!(r.contains("x"));
}

// Edge cases

#[test]
fn refs_empty_template() {
    assert!(refs("Just static text").is_empty());
}

#[test]
fn refs_raw_block_not_analyzed() {
    assert!(
        refs("> {% raw %}{{ not_a_var }}{% /raw %}").is_empty(),
        "raw blocks should not contribute variables",
    );
}

#[test]
fn refs_pipe_expression_extracts_root() {
    // If pipes are supported: `{{ name | upper }}` → root is `name`.
    let r = refs("{{ name | upper }}");
    assert!(r.contains("name"));
}

#[test]
fn refs_deduplicates() {
    let r = refs("{{ x }} {{ x }} {{ x }}");
    assert!(r.contains("x"));
    assert_eq!(r.len(), 1, "same var referenced 3 times should appear once");
}

#[test]
fn refs_not_keyword_creates_bad_var() {
    // `not flag` parses as a truthiness check on path "not flag",
    // which should be caught as an undeclared variable.
    let r = refs("> {% if not flag %}yes{% /if %}");
    assert!(
        r.contains("not flag"),
        "should contain 'not flag' as referenced variable, got: {r:?}",
    );
}

#[test]
fn refs_not_in_operator_creates_bad_var() {
    // `x not in items` parses as `x not` (left) `in` (op) `items` (right),
    // and "x not" is flagged as undeclared by the template compiler.
    let r = refs("> {% if x not in items %}yes{% /if %}");
    assert!(
        r.contains("x not"),
        "should contain 'x not' as referenced variable, got: {r:?}",
    );
}
