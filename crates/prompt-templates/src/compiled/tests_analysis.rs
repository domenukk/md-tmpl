//! Analysis, `eval_condition`, whitespace control, and blockquote tests
//! for the compiled template engine.

use std::collections::{HashMap, HashSet};

use super::{
    analysis::{collect_referenced_params, parse_condition},
    render::eval_condition,
    *,
};
use crate::{context::Context, value::Value};

// -- collect_referenced_params ------------------------------------------

/// Compile a template body and return the set of referenced parameter names.
fn refs(src: &str) -> HashSet<String> {
    collect_referenced_params(&compile(src).unwrap().0)
}

/// Helper: compile + render.
fn compiled_render(template: &str, ctx: &Context) -> Result<String, TemplateError> {
    let (segments, _) = compile(template)?;
    let mut scope = crate::scope::Scope::new(ctx);
    super::render::render_segments(&segments, &mut scope, None)
}

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
    let r = refs("{% for item in items %}{{ item }}{% /for %}");
    assert!(r.contains("items"), "iterable should be referenced");
    assert!(!r.contains("item"), "loop binding should be excluded");
}

#[test]
fn refs_nested_for_loop_scopes() {
    let r = refs("{% for row in rows %}{% for col in row.cols %}{{ col }}{% /for %}{% /for %}");
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
    let r = refs("{% for x in items %}{{ x }}{% /for %}{{ x }}");
    assert!(r.contains("items"));
    assert!(r.contains("x"), "x should be found after loop scope ends");
}

// Conditions

#[test]
fn refs_simple_condition() {
    assert!(refs("{% if show %}yes{% /if %}").contains("show"));
}

#[test]
fn refs_condition_comparison_left() {
    let r = refs("{% if count > 0 %}yes{% /if %}");
    assert!(r.contains("count"));
    assert!(!r.contains("0"), "literal should not be a var");
}

#[test]
fn refs_condition_comparison_both_sides() {
    let r = refs("{% if left == right %}match{% /if %}");
    assert!(r.contains("left"));
    assert!(r.contains("right"));
}

#[test]
fn refs_condition_else_branch() {
    let r = refs("{% if flag %}{{ a }}{% else %}{{ b }}{% /if %}");
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
    let r = refs("{% for item in items %}{{ idx(item) }}{% /for %}");
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
    let r = refs("{% if x == 'hello' %}yes{% /if %}");
    assert!(r.contains("x"));
    // 'hello' is a string literal.
}

#[test]
fn refs_bool_literal_excluded() {
    let r = refs("{% if x == true %}yes{% /if %}");
    assert!(r.contains("x"));
}

#[test]
fn refs_float_literal_excluded() {
    let r = refs("{% if x > 3.14 %}yes{% /if %}");
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
        refs("{% raw %}{{ not_a_var }}{% /raw %}").is_empty(),
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

// -- elif chain tests -------------------------------------------------------

#[test]
fn elif_first_branch_matches() {
    let mut ctx = Context::new();
    ctx.set("val", "A");
    let result = compiled_render(
        "{% if val == 'A' %}first{% elif val == 'B' %}second{% else %}other{% /if %}",
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
        "{% if val == 'A' %}first{% elif val == 'B' %}second{% elif val == 'C' %}third{% /if %}",
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
        "{% if val == 'A' %}first{% elif val == 'B' %}second{% else %}fallback{% /if %}",
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
        "{% if val == 'A' %}first{% elif val == 'B' %}second{% /if %}",
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
        "{% if val == 'A' %}a{% elif val == 'B' %}{% if nested %}inner{% /if %}{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "inner");
}

#[test]
fn elif_refs_all_branches() {
    let r = refs("{% if a == 'x' %}{{ b }}{% elif c == 'y' %}{{ d }}{% else %}{{ e }}{% /if %}");
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

// -- whitespace control tests -------------------------------------------

#[test]
fn trim_before_strips_preceding_whitespace() {
    // `{%-` should strip whitespace between previous content and the tag,
    // back to the previous newline (keeping the newline).
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("hello\n   {%- if show %}yes{%- /if %}", &ctx).unwrap();
    assert_eq!(result, "hello\nyes");
}

#[test]
fn trim_after_strips_following_whitespace() {
    // `-%}` should strip whitespace after the tag up to and including
    // the next newline.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("> {% if show -%}\n  content\n> {% /if %}", &ctx).unwrap();
    assert_eq!(result, "content\n");
}

#[test]
fn trim_expr_both_sides() {
    // `{{-` and `-}}` trim whitespace around expression tags.
    let mut ctx = Context::new();
    ctx.set("name", "world");
    let result = compiled_render("hello  {{- name -}}  \nbye", &ctx).unwrap();
    assert_eq!(result, "helloworldbye");
}

#[test]
fn trim_combined_both_sides() {
    // `{%- -%}` trims whitespace on both sides of the opening tag.
    // Note: trim markers on the *closing* tag are handled by
    // find_closing_block which doesn't apply trim logic.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("begin\n> {%- if show -%}\ncontent{% /if %}", &ctx).unwrap();
    assert_eq!(result, "begin\ncontent");
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
        Value::List(vec![Value::Str("a".into()), Value::Str("b".into())]),
    );
    // `{%-` before for trims the preceding newline from "list:\n".
    // Inside the loop body, `{{- item -}}` trims around the expression.
    let result = compiled_render(
        "list:{%- for item in items %}\n- {{- item -}}\n> {%- /for %}",
        &ctx,
    )
    .unwrap();
    // After the opening for tag (no -%}), \n is kept.
    // `{{- item -}}` strips surrounding whitespace around item value.
    // `{%- /for %}` — the `{%-` trims trailing whitespace before the close tag.
    assert_eq!(result, "list:\n-a\n-b");
}

#[test]
fn trim_before_at_start_of_input() {
    // `{%-` at the very beginning with no preceding text should not panic.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("{%- if show %}yes{% /if %}", &ctx).unwrap();
    assert_eq!(result, "yes");
}

#[test]
fn trim_after_at_end_of_input() {
    // `-%}` at the very end with no following text should not panic.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("{% if show %}yes{% /if -%}", &ctx).unwrap();
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

// -- eval_condition comprehensive tests (ported from scope.rs) --------

/// Helper: evaluate a condition string through the production path
/// (`parse_condition` → `eval_condition`).
fn eval_cond(condition: &str, ctx: &Context) -> Result<bool, crate::error::TemplateError> {
    let cond = parse_condition(condition);
    let scope = crate::scope::Scope::new(ctx);
    eval_condition(&cond, &scope)
}

#[test]
fn eval_condition_comprehensive() {
    let mut ctx = Context::new();
    ctx.set("outcome", "Confirmed");
    ctx.set("count", 5_i64);
    ctx.set("ratio", 1.23_f64);
    ctx.set("items", Value::List(vec![Value::Int(10), Value::Int(20)]));

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
fn condition_in_template_with_len_function() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(vec![Value::Int(10), Value::Int(20)]));

    // len() in conditions — tested via compiled_render (full pipeline)
    assert_eq!(
        compiled_render("{% if len(items) == 2 %}yes{% /if %}", &ctx).unwrap(),
        "yes"
    );
    assert_eq!(
        compiled_render("{% if len(items) > 0 %}yes{% /if %}", &ctx).unwrap(),
        "yes"
    );
    assert_eq!(
        compiled_render("{% if len(items) == 0 %}yes{% /if %}", &ctx).unwrap(),
        ""
    );
}

// -- blockquote tag stripping tests --------------------------------------

#[test]
fn blockquote_if_compact() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(">{% if show %}\nyes\n>{% /if %}", &ctx).unwrap();
    assert_eq!(result, "yes\n");
}

#[test]
fn blockquote_if_spaced() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("> {% if show %}\nyes\n> {% /if %}", &ctx).unwrap();
    assert_eq!(result, "yes\n");
}

#[test]
fn blockquote_if_else() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(false));
    let result = compiled_render(">{% if show %}\nyes\n>{% else %}\nno\n>{% /if %}", &ctx).unwrap();
    assert_eq!(result, "no\n");
}

#[test]
fn blockquote_for_loop() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(vec![Value::Str("a".into()), Value::Str("b".into())]),
    );
    let result = compiled_render(">{% for x in items %}\n- {{ x }}\n>{% /for %}", &ctx).unwrap();
    assert_eq!(result, "- a\n- b\n");
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
        ">{% if status == \"active\" %}\nrunning\n>{% elif status == \"paused\" %}\npaused\n>{% else %}\nstopped\n>{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "paused\n");
}

// -- additional blockquote stripping tests --------------------------------

#[test]
fn blockquote_if_false_renders_empty() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(false));
    let result = compiled_render(">{% if show %}\nyes\n>{% /if %}", &ctx).unwrap();
    assert_eq!(result, "");
}

#[test]
fn blockquote_nested_if_in_for() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(vec![
            Value::Dict(HashMap::from([
                ("name".into(), Value::Str("alpha".into())),
                ("show".into(), Value::Bool(true)),
            ])),
            Value::Dict(HashMap::from([
                ("name".into(), Value::Str("beta".into())),
                ("show".into(), Value::Bool(false)),
            ])),
        ]),
    );
    let result = compiled_render(
        "> {% for item in items %}\n> {% if item.show %}\n{{ item.name }}\n> {% /if %}\n> {% /for %}",
        &ctx,
    )
    .unwrap();
    // Only "alpha" is shown (beta has show=false). The for-loop
    // body for beta emits nothing (if-false), so we get one item
    // plus a trailing newline from the body text.
    assert_eq!(result, "alpha\n");
}

#[test]
fn blockquote_stripping_is_idempotent() {
    // Running the blockquote-stripped form through compile should give
    // the same output as the blockquoted form.
    let blockquoted = "> {% if show %}\nyes\n> {% /if %}";
    let stripped = "> {% if show %}yes\n> {% /if %}";
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result_bq = compiled_render(blockquoted, &ctx).unwrap();
    let result_plain = compiled_render(stripped, &ctx).unwrap();
    assert_eq!(result_bq, result_plain);
    assert_eq!(result_bq, "yes\n");
}

#[test]
fn blockquote_mixed_with_expressions() {
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("show", Value::Bool(true));
    let result = compiled_render(
        "Hello {{ name }}!\n>{% if show %}\nVisible.\n>{% /if %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "Hello world!\nVisible.\n");
}

#[test]
fn blockquote_with_trim_markers() {
    // Whitespace-control `{%-` / `-%}` should still work after stripping.
    // After blockquote stripping: `{%- if show -%}content\n{%- /if -%}`
    // `-%}` on the if-tag strips the leading whitespace/newline after it,
    // and `{%-` on the /if trims trailing whitespace before it.
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = compiled_render("> {%- if show -%}\ncontent\n> {%- /if -%}", &ctx).unwrap();
    assert_eq!(result, "content\n");
}

#[test]
fn blockquote_for_single_item() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(vec![Value::Str("only".into())]));
    let result = compiled_render(">{% for x in items %}\n{{ x }}\n>{% /for %}", &ctx).unwrap();
    assert_eq!(result, "only\n");
}

#[test]
fn blockquote_for_empty_list() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(vec![]));
    let result = compiled_render(">{% for x in items %}\n{{ x }}\n>{% /for %}", &ctx).unwrap();
    assert_eq!(result, "");
}

// -- markdown link include E2E pipeline tests -----------------------------

#[test]
fn markdown_link_include_simple() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("greeting.tmpl.md"),
        "---\nname: greeting\nparams: []\n---\nHello!",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("main.tmpl.md"),
        "---\nname: main\nparams: []\n---\n> {% include [greeting](greeting.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("main.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(result, "Hello!");
}

#[test]
fn markdown_link_include_with_vars() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "---\nname: child\nparams: [msg = str]\n---\nGot: {{ msg }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "---\nname: parent\nparams: []\n---\n> {% include [child](child.tmpl.md) with msg=\"hi\" %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(result, "Got: hi");
}

#[test]
fn markdown_link_include_for_each() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("row.tmpl.md"),
        "---\nname: row\nparams: [item = str]\n---\n- {{ item }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("list.tmpl.md"),
        "---\nname: list\nparams: [items = list<>]\n---\n> {% include [row](row.tmpl.md) for item in items %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("list.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(vec![Value::Str("one".into()), Value::Str("two".into())]),
    );
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(result, "- one- two");
}
