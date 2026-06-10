//! Tests for the compiled template engine.

use std::collections::HashMap;

use super::{
    inline::extract_inline_templates,
    render::{estimate_output_capacity, render_segments},
    *,
};
use crate::{context::Context, scope::Scope, value::Value};

/// Helper: compile + render.
fn compiled_render(template: &str, ctx: &Context) -> Result<String, TemplateError> {
    let (segments, _) = compile(template)?;
    let mut scope = Scope::new(ctx);
    render_segments(&segments, &mut scope, None)
}

/// Assert the compiled path produces a result without error.
fn assert_same(template: &str, ctx: &Context) {
    compiled_render(template, ctx).expect("compiled render failed");
}

// -- basic compilation --

#[test]
fn compile_empty() {
    let (segs, _) = compile("").unwrap();
    assert!(segs.is_empty());
}

#[test]
fn compile_static_only() {
    let (segs, _) = compile("Hello world!").unwrap();
    assert_eq!(segs.len(), 1);
    assert!(matches!(&segs[0], Segment::Static(s) if s == "Hello world!"));
}

#[test]
fn compile_expr() {
    let (segs, _) = compile("Hello {{ name }}!").unwrap();
    assert_eq!(segs.len(), 3);
    assert!(matches!(&segs[0], Segment::Static(s) if s == "Hello "));
    assert!(
        matches!(&segs[1], Segment::Expr { path, filters } if path == "name" && filters.is_empty())
    );
    assert!(matches!(&segs[2], Segment::Static(s) if s == "!"));
}

#[test]
fn compile_expr_with_filters() {
    let (segs, _) = compile("{{ name | upper | trim }}").unwrap();
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Segment::Expr { path, filters } => {
            assert_eq!(path, "name");
            assert_eq!(filters.len(), 2);
            assert_eq!(filters[0].kind, FilterKind::Upper);
            assert_eq!(filters[1].kind, FilterKind::Trim);
        }
        other => panic!("expected Expr, got {other:?}"),
    }
}

#[test]
fn compile_for_loop() {
    let (segs, _) = compile("{% for item in items %}{{ item }}{% /for %}").unwrap();
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Segment::ForLoop {
            binding,
            list_path,
            body,
        } => {
            assert_eq!(binding, "item");
            assert_eq!(list_path, "items");
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected ForLoop, got {other:?}"),
    }
}

#[test]
fn compile_if_else() {
    let (segs, _) = compile("{% if show %}yes{% else %}no{% /if %}").unwrap();
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Segment::If {
            branches,
            else_body,
        } => {
            assert_eq!(branches.len(), 1);
            assert!(matches!(&branches[0].0, Condition::Truthy(v) if v == "show"));
            assert_eq!(branches[0].1.len(), 1);
            assert_eq!(else_body.len(), 1);
        }
        other => panic!("expected If, got {other:?}"),
    }
}

#[test]
fn compile_raw() {
    let (segs, _) = compile("{% raw %}{{ not_a_var }}{% /raw %}").unwrap();
    assert_eq!(segs.len(), 1);
    assert!(matches!(&segs[0], Segment::Raw(s) if s == "{{ not_a_var }}"));
}

#[test]
fn compile_raw_custom_delimiter() {
    let (segs, _) = compile("{% raw=# %}stuff{% /# %}").unwrap();
    assert_eq!(segs.len(), 1);
    assert!(matches!(&segs[0], Segment::Raw(s) if s == "stuff"));
}

#[test]
fn compile_raw_custom_delimiter_contains_raw_close() {
    // The whole point: output literal {% /raw %} by using a different closer.
    let (segs, _) = compile("{% raw=DELIM %}{% raw %}{{ x }}{% /raw %}{% /DELIM %}").unwrap();
    assert_eq!(segs.len(), 1);
    assert!(
        matches!(&segs[0], Segment::Raw(s) if s == "{% raw %}{{ x }}{% /raw %}"),
        "should preserve literal raw tags in output"
    );
}

#[test]
fn compile_raw_empty_delimiter_errors() {
    let err = compile("{% raw= %}oops{% /raw %}").unwrap_err();
    assert!(
        err.to_string().contains("delimiter"),
        "should mention missing delimiter: {err}"
    );
}
// -- inline template extraction tests --

#[test]
fn extract_basic_inline_template() {
    let input = "before\n{% tmpl greeting %}\n---\nparams: []\n---\nHello!\n{% /tmpl %}\nafter";
    let (cleaned, templates) = extract_inline_templates(input).unwrap();
    assert_eq!(cleaned.trim(), "before\nafter");
    assert_eq!(templates.len(), 1);
    let tmpl = templates.get("greeting").expect("missing 'greeting'");
    assert!(tmpl.declarations.is_empty());
    assert!(!tmpl.segments.is_empty(), "segments should be pre-compiled");
}

#[test]
fn extract_inline_template_with_frontmatter() {
    let input = concat!(
        "{% tmpl row %}\n",
        "---\n",
        "params: [label = str]\n",
        "---\n",
        "- {{ label }}\n",
        "{% /tmpl %}\n",
    );
    let (_, templates) = extract_inline_templates(input).unwrap();
    let tmpl = templates.get("row").unwrap();
    assert_eq!(tmpl.declarations.len(), 1);
    assert_eq!(tmpl.declarations[0].name, "label");
}

#[test]
fn extract_duplicate_inline_template_errors() {
    let input = "{% tmpl a %}\n---\nparams: []\n---\nfoo\n{% /tmpl %}\n{% tmpl a %}\n---\nparams: []\n---\nbar\n{% /tmpl %}";
    let err = extract_inline_templates(input).unwrap_err();
    assert!(
        err.to_string().contains("duplicate"),
        "expected duplicate error, got: {err}"
    );
}

#[test]
fn extract_empty_name_errors() {
    let input = "{% tmpl %}\n---\nparams: []\n---\nfoo\n{% /tmpl %}";
    let err = extract_inline_templates(input).unwrap_err();
    assert!(
        err.to_string().contains("tmpl NAME"),
        "expected name error, got: {err}"
    );
}

#[test]
fn extract_skips_raw_block() {
    // {% tmpl %} inside {% raw %} should NOT be treated as an inline template.
    let input = "{% raw %}\n{% tmpl fake %}\nnot a template\n{% /tmpl %}\n{% /raw %}";
    let (cleaned, templates) = extract_inline_templates(input).unwrap();
    assert!(
        templates.is_empty(),
        "should not find templates inside raw blocks"
    );
    assert!(
        cleaned.contains("{% tmpl fake %}"),
        "raw content should be preserved literally"
    );
}

#[test]
fn extract_multiple_inline_templates() {
    let input = concat!(
        "{% tmpl alpha %}\n---\nparams: []\n---\nA\n{% /tmpl %}\n",
        "middle\n",
        "{% tmpl beta %}\n---\nparams: []\n---\nB\n{% /tmpl %}\n",
    );
    let (cleaned, templates) = extract_inline_templates(input).unwrap();
    assert_eq!(templates.len(), 2);
    assert!(templates.contains_key("alpha"));
    assert!(templates.contains_key("beta"));
    assert_eq!(cleaned.trim(), "middle");
}

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
        Value::List(vec![
            Value::Dict(HashMap::from([(
                "label".into(),
                Value::Str("alpha".into()),
            )])),
            Value::Dict(HashMap::from([("label".into(), Value::Str("beta".into()))])),
        ]),
    );
    assert_same(
        "{% for item in items %}{{ idx(item) }}: {{ item.label }}\n> {% /for %}",
        &ctx,
    );
}

#[test]
fn parity_nested_for_loops() {
    let mut ctx = Context::new();
    ctx.set(
        "groups",
        Value::List(vec![
            Value::Dict(HashMap::from([("name".into(), Value::Str("G1".into()))])),
            Value::Dict(HashMap::from([("name".into(), Value::Str("G2".into()))])),
        ]),
    );
    ctx.set(
        "tags",
        Value::List(vec![Value::Dict(HashMap::from([(
            "t".into(),
            Value::Str("A".into()),
        )]))]),
    );
    assert_same(
        "{% for g in groups %}[{{ g.name }}{% for t in tags %}:{{ t.t }}{% /for %}]\n> {% /for %}",
        &ctx,
    );
}

#[test]
fn parity_if_true() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    assert_same("{% if show %}visible{% /if %}", &ctx);
}

#[test]
fn parity_if_false() {
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(false));
    assert_same("{% if show %}visible{% /if %}", &ctx);
}

#[test]
fn parity_if_else() {
    let mut ctx = Context::new();
    ctx.set("active", Value::Bool(false));
    assert_same("{% if active %}Running{% else %}Stopped{% /if %}", &ctx);
}

#[test]
fn parity_nested_if() {
    let mut ctx = Context::new();
    ctx.set("a", Value::Bool(true));
    ctx.set("b", Value::Bool(false));
    assert_same(
        "{% if a %}A{% if b %}B{% else %}notB{% /if %}{% /if %}",
        &ctx,
    );
}

#[test]
fn parity_raw_block() {
    assert_same("{% raw %}{{ not_a_variable }}{% /raw %}", &Context::new());
}

#[test]
fn parity_mixed_content() {
    let mut ctx = Context::new();
    ctx.set("title", "Report");
    ctx.set("show_footer", Value::Bool(true));
    ctx.set(
        "items",
        Value::List(vec![Value::Dict(HashMap::from([(
            "name".into(),
            Value::Str("Item 1".into()),
        )]))]),
    );
    assert_same(
        "# {{ title }}\n{% for item in items %}- {{ item.name }}\n{% /for %}{% if show_footer %}---\nFooter{% /if %}",
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
        Value::List(vec![Value::Str("a".into()), Value::Str("b".into())]),
    );
    let result = compiled_render(
        "{% if show %}{% for item in items %}[{{ item }}]{% /for %}{% /if %}",
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
        Value::List(vec![
            Value::Dict(HashMap::from([
                ("name".into(), Value::Str("visible".into())),
                ("show".into(), Value::Bool(true)),
            ])),
            Value::Dict(HashMap::from([
                ("name".into(), Value::Str("hidden".into())),
                ("show".into(), Value::Bool(false)),
            ])),
        ]),
    );
    let result = compiled_render(
        "{% for item in items %}{% if item.show %}{{ item.name }}{% /if %}{% /for %}",
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
        Value::List(vec![Value::Str("x".into()), Value::Str("y".into())]),
    );
    ctx.set(
        "b",
        Value::List(vec![Value::Str("1".into()), Value::Str("2".into())]),
    );
    ctx.set("c", Value::List(vec![Value::Str("!".into())]));
    let result = compiled_render(
        "{% for ai in a %}{% for bi in b %}{% for ci in c %}{{ ai }}{{ bi }}{{ ci }}{% /for %}{% /for %}{% /for %}",
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
    let err = compiled_render("{% for item in items %}x{% /for %}", &ctx)
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
fn deprecated_endfor_rejected() {
    let err = compile("> {% endfor %}").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("/for"),
        "error should suggest new syntax: {msg}"
    );
}

#[test]
fn parity_default_filter() {
    let mut ctx = Context::new();
    ctx.set("val", "");
    assert_same("{{ val | default(\"fallback\") }}", &ctx);
}

#[test]
fn parity_length_filter() {
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
    );
    assert_same("{{ items | length }}", &ctx);
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
        Value::List(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
            Value::Str("c".into()),
        ]),
    );
    let result = compiled_render(
        "{% for item in items %}{{ idx(item) }}:{{ item }} {% /for %}",
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
        err.to_string().contains("idx(name)") || err.to_string().contains("undefined"),
        "should mention unresolvable idx call: {err}"
    );
}

#[test]
fn nested_loops_independent_idx() {
    let mut ctx = Context::new();
    ctx.set(
        "outer",
        Value::List(vec![Value::Str("A".into()), Value::Str("B".into())]),
    );
    ctx.set(
        "inner",
        Value::List(vec![Value::Str("x".into()), Value::Str("y".into())]),
    );
    let result = compiled_render(
        "{% for o in outer %}{% for i in inner %}{{ idx(i) }}{% /for %},{% /for %}",
        &ctx,
    )
    .unwrap();
    assert_eq!(result, "01,01,");
}

#[test]
fn nested_loops_outer_idx_accessible_from_inner() {
    let mut ctx = Context::new();
    ctx.set(
        "bugs",
        Value::List(vec![Value::Str("bug1".into()), Value::Str("bug2".into())]),
    );
    ctx.set(
        "tags",
        Value::List(vec![Value::Str("t1".into()), Value::Str("t2".into())]),
    );
    // From inside the inner loop, idx(bug) should still resolve
    // the OUTER loop's index — that's the whole point.
    let result = compiled_render(
        "{% for bug in bugs %}{% for tag in tags %}{{ idx(bug) }}.{{ idx(tag) }} {% /for %}{% /for %}",
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
        Value::List(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
            Value::Str("c".into()),
        ]),
    );
    let result = compiled_render("{{ len(items) }}", &ctx).unwrap();
    assert_eq!(result, "3");
}

#[test]
fn bare_index_not_available() {
    let mut ctx = Context::new();
    ctx.set("items", Value::List(vec![Value::Str("a".into())]));
    // Bare `index` should not resolve — use `{{ idx(item) }}`.
    let err = compiled_render("{% for item in items %}{{ index }}{% /for %}", &ctx)
        .expect_err("bare 'index' should not resolve");
    assert!(
        err.to_string().contains("index") || err.to_string().contains("undefined"),
        "should mention unresolvable 'index': {err}"
    );
}

// -- include depth limit --

#[test]
fn include_depth_limit_enforced() {
    let dir = tempfile::tempdir().unwrap();
    // Create a self-including template to trigger depth limit.
    std::fs::write(
        dir.path().join("self.tmpl.md"),
        "---\nname: self\nparams: []\n---\nX{% include [self](self.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("self.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render(&ctx)
        .expect_err("self-including template should exceed depth limit");
    let err = err.to_string();
    assert!(
        err.contains("include depth"),
        "error should mention depth limit: {err}"
    );
}

#[test]
fn include_depth_custom_limit_enforced() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "---\nname: child\nparams: []\n---\nchild",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "---\nname: parent\nparams: []\n---\nparent+{% include [child](child.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("grandparent.tmpl.md"),
        "---\nname: grandparent\nparams: []\n---\ngrandparent+{% include [parent](parent.tmpl.md) %}",
    )
    .unwrap();

    let mut tmpl = crate::Template::from_file(&dir.path().join("grandparent.tmpl.md")).unwrap();
    let ctx = Context::new();

    // 1. With default max include depth (16), it should render fine.
    let res = tmpl.render(&ctx).unwrap();
    assert_eq!(res, "grandparent+parent+child");

    // 2. Set max include depth to 1. Since grandparent -> parent -> child requires 2 includes, depth is 2, so it should error.
    tmpl.set_max_include_depth(1);
    let err = tmpl
        .render(&ctx)
        .expect_err("include depth 1 should be exceeded by 2-level chain");
    let err = err.to_string();
    assert!(
        err.contains("include depth (1) exceeded"),
        "error should mention depth limit: {err}"
    );

    // 3. Set max include depth to 2, it should render fine again.
    let tmpl = tmpl.with_max_include_depth(2);
    let res2 = tmpl.render(&ctx).unwrap();
    assert_eq!(res2, "grandparent+parent+child");
}

#[test]
fn deeply_nested_includes_three_levels() {
    // Grandparent → Parent → Child: 3 levels of distinct includes.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "---\nname: child\nparams: [leaf = str]\n---\nLeaf:{{ leaf }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "---\nname: parent\nparams: [middle = str]\n---\n\
         Mid:{{ middle }},{% include [child](child.tmpl.md) with leaf=\"deep\" %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("grandparent.tmpl.md"),
        "---\nname: grandparent\nparams: [top = str]\n---\n\
         Top:{{ top }},{% include [parent](parent.tmpl.md) with middle=\"mid\" %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("grandparent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("top", "root");
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(result, "Top:root,Mid:mid,Leaf:deep");
}

#[test]
fn deeply_nested_includes_four_levels() {
    // A → B → C → D: 4 levels of distinct includes.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("d.tmpl.md"),
        "---\nname: d\nparams: []\n---\nD",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("c.tmpl.md"),
        "---\nname: c\nparams: []\n---\nC+{% include [d](d.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "---\nname: b\nparams: []\n---\nB+{% include [c](c.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "---\nname: a\nparams: []\n---\nA+{% include [b](b.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(result, "A+B+C+D");
}

// -- include contract enforcement --

#[test]
fn include_missing_contract_params_errors() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "---\nname: child\nparams: [msg = str]\n---\n{{ msg }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "---\nname: parent\nparams: []\n---\n> {% include [child](child.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render(&ctx)
        .expect_err("include missing required param 'msg' should fail");
    let err = err.to_string();
    assert!(
        err.contains("msg"),
        "error should mention missing 'msg': {err}"
    );
}

#[test]
fn include_with_contract_satisfied_renders() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "---\nname: child\nparams: [msg = str]\n---\nHello {{ msg }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "---\nname: parent\nparams: []\n---\n> {% include [child](child.tmpl.md) with msg=\"World\" %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(result, "Hello World");
}

#[test]
fn include_no_params_always_ok() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("static.tmpl.md"),
        "---\nname: static\nparams: []\n---\nStatic!",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "---\nname: parent\nparams: []\n---\n> {% include [static](static.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(result, "Static!");
}

// ---------------------------------------------------------------------------
// `match / case` tests
// ---------------------------------------------------------------------------

#[test]
fn match_multi_arm_unit_variant() {
    let template = "\
> {% match status %}
> {% case Active %}
Running
> {% case Paused %}
Paused
> {% case Stopped %}
Stopped
> {% /match %}";
    let mut ctx = Context::new();
    ctx.set("status", "Paused");
    let result = compiled_render(template, &ctx).unwrap();
    assert!(result.contains("Paused"), "got: {result:?}");
    assert!(!result.contains("Running"));
    assert!(!result.contains("Stopped"));
}

#[test]
fn match_multi_arm_struct_variant() {
    let template = "\
> {% match outcome %}
> {% case Confirmed %}
CONFIRMED: {{ outcome.evidence }}
> {% case NotConfirmed %}
NOT CONFIRMED
> {% /match %}";
    let mut ctx = Context::new();
    ctx.set(
        "outcome",
        Value::dict([
            ("tag", Value::from("Confirmed")),
            ("evidence", Value::from("crash log")),
        ]),
    );
    let result = compiled_render(template, &ctx).unwrap();
    assert!(result.contains("CONFIRMED: crash log"), "got: {result:?}");
    assert!(!result.contains("NOT CONFIRMED"));
}

#[test]
fn match_inline_case_matches() {
    let template = "> {% match status case Active %}Running{% /match %}";
    let mut ctx = Context::new();
    ctx.set("status", "Active");
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(result.trim(), "Running");
}

#[test]
fn match_inline_case_no_match() {
    let template = "> {% match status case Active %}Running{% /match %}";
    let mut ctx = Context::new();
    ctx.set("status", "Stopped");
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(result.trim(), "");
}

#[test]
fn match_no_matching_arm_produces_no_output() {
    let template = "\
> {% match status %}
> {% case Active %}
active
> {% case Paused %}
paused
> {% /match %}";
    let mut ctx = Context::new();
    ctx.set("status", "Stopped");
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(result.trim(), "");
}

#[test]
fn match_inline_with_nested_if() {
    let template = "\
> {% match vuln case Known %}
>   {% if vuln.label == \"test\" %}
Found test!
>   {% /if %}
> {% /match %}";
    let mut ctx = Context::new();
    ctx.set(
        "vuln",
        Value::dict([
            ("tag", Value::from("Known")),
            ("label", Value::from("test")),
        ]),
    );
    let result = compiled_render(template, &ctx).unwrap();
    assert!(result.contains("Found test!"), "got: {result:?}");
}

#[test]
fn match_inline_with_nested_if_no_match() {
    let template = "\
> {% match vuln case Known %}
>   {% if vuln.label == \"test\" %}
Found test!
>   {% /if %}
> {% /match %}";
    let mut ctx = Context::new();
    ctx.set(
        "vuln",
        Value::dict([
            ("tag", Value::from("Known")),
            ("label", Value::from("other")),
        ]),
    );
    let result = compiled_render(template, &ctx).unwrap();
    assert!(!result.contains("Found test!"));
}

#[test]
fn match_error_on_non_enum_value() {
    let template = "> {% match count %}{% case One %}one{% /match %}";
    let mut ctx = Context::new();
    ctx.set("count", Value::from(42));
    let err = compiled_render(template, &ctx).unwrap_err();
    assert!(err.to_string().contains("not an enum value"), "got: {err}");
}

// ---------------------------------------------------------------------------
// Multi-variant case (`{% case A | B %}`)
// ---------------------------------------------------------------------------

#[test]
fn match_multi_variant_case_first_matches() {
    let template = "\
> {% match outcome %}
> {% case Confirmed | ConfirmedWithCaveats %}
Evidence found.
> {% case NotConfirmed %}
No evidence.
> {% /match %}";
    let mut ctx = Context::new();
    ctx.set("outcome", "Confirmed");
    let result = compiled_render(template, &ctx).unwrap();
    assert!(result.contains("Evidence found."), "got: {result:?}");
}

#[test]
fn match_multi_variant_case_second_matches() {
    let template = "\
> {% match outcome %}
> {% case Confirmed | ConfirmedWithCaveats %}
Evidence found.
> {% case NotConfirmed %}
No evidence.
> {% /match %}";
    let mut ctx = Context::new();
    ctx.set("outcome", "ConfirmedWithCaveats");
    let result = compiled_render(template, &ctx).unwrap();
    assert!(result.contains("Evidence found."), "got: {result:?}");
}

#[test]
fn match_multi_variant_case_no_match() {
    let template = "\
> {% match outcome %}
> {% case Confirmed | ConfirmedWithCaveats %}
Evidence found.
> {% case NotConfirmed %}
No evidence.
> {% /match %}";
    let mut ctx = Context::new();
    ctx.set("outcome", "Inconclusive");
    let result = compiled_render(template, &ctx).unwrap();
    assert!(!result.contains("Evidence"), "got: {result:?}");
}

#[test]
fn match_inline_multi_variant_case() {
    let template = "> {% match status case Active | Running %}active{% /match %}";
    let mut ctx = Context::new();
    ctx.set("status", "Running");
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(result.trim(), "active");
}
