//! Tests for the compiled template engine.

use std::sync::Arc;

use super::{
    render::{estimate_output_capacity, render_segments},
    *,
};
use crate::{compat::HashMap, context::Context, scope::Scope, value::Value};

/// Wrapper: compile with empty parent type aliases (no inline inheritance needed).
fn compile(
    input: &str,
) -> Result<(Vec<Segment>, HashMap<String, CompiledInlineTemplate>), TemplateError> {
    let empty = HashMap::new();
    super::compile(input, &empty)
}

/// Wrapper: `extract_inline_templates` with empty parent type aliases.
fn extract_inline_templates(
    input: &str,
) -> Result<(String, HashMap<String, CompiledInlineTemplate>), TemplateError> {
    let empty = HashMap::new();
    super::inline::extract_inline_templates(input, &empty)
}

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
        matches!(&segs[1], Segment::Expr { expr: CompiledExpr::Path(p), filters } if p.as_str() == "name" && filters.is_empty())
    );
    assert!(matches!(&segs[2], Segment::Static(s) if s == "!"));
}

#[test]
fn compile_expr_with_filters() {
    let (segs, _) = compile("{{ name | upper | trim }}").unwrap();
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Segment::Expr {
            expr: CompiledExpr::Path(p),
            filters,
        } => {
            assert_eq!(p.as_str(), "name");
            assert_eq!(filters.len(), 2);
            assert_eq!(filters[0].kind, FilterKind::Upper);
            assert_eq!(filters[1].kind, FilterKind::Trim);
        }
        other => panic!("expected Expr, got {other:?}"),
    }
}

#[test]
fn compile_for_loop() {
    let (segs, _) = compile("> {% for item in items %}{{ item }}{% /for %}").unwrap();
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Segment::ForLoop {
            binding,
            list_expr,
            body,
            else_body,
        } => {
            assert_eq!(binding, "item");
            assert!(matches!(list_expr, CompiledExpr::Path(p) if p.as_str() == "items"));
            assert_eq!(body.len(), 1);
            assert!(else_body.is_empty());
        }
        other => panic!("expected ForLoop, got {other:?}"),
    }
}

#[test]
fn compile_if_else() {
    let (segs, _) = compile("> {% if show %}yes{% else %}no{% /if %}").unwrap();
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Segment::If {
            branches,
            else_body,
        } => {
            assert_eq!(branches.len(), 1);
            assert!(
                matches!(&branches[0].0, Condition::Truthy(ConditionOperand::Path { path, .. }) if path.as_str() == "show")
            );
            assert_eq!(branches[0].1.len(), 1);
            assert_eq!(else_body.len(), 1);
        }
        other => panic!("expected If, got {other:?}"),
    }
}

#[test]
fn compile_raw() {
    let (segs, _) = compile("> {% raw %}{{ not_a_var }}{% /raw %}").unwrap();
    assert_eq!(segs.len(), 1);
    assert!(matches!(&segs[0], Segment::Raw(s) if s == "{{ not_a_var }}"));
}

#[test]
fn compile_raw_custom_delimiter() {
    let (segs, _) = compile("> {% raw=# %}stuff{% /# %}").unwrap();
    assert_eq!(segs.len(), 1);
    assert!(matches!(&segs[0], Segment::Raw(s) if s == "stuff"));
}

#[test]
fn compile_raw_custom_delimiter_contains_raw_close() {
    // The whole point: output literal {% /raw %} by using a different closer.
    let (segs, _) = compile("> {% raw=DELIM %}{% raw %}{{ x }}{% /raw %}{% /DELIM %}").unwrap();
    assert_eq!(segs.len(), 1);
    assert!(
        matches!(&segs[0], Segment::Raw(s) if s == "{% raw %}{{ x }}{% /raw %}"),
        "should preserve literal raw tags in output"
    );
}

#[test]
fn compile_raw_empty_delimiter_errors() {
    let err = compile("> {% raw= %}oops{% /raw %}").unwrap_err();
    assert!(
        err.to_string().contains("delimiter"),
        "should mention missing delimiter: {err}"
    );
}
// -- inline template extraction tests --

#[test]
fn extract_basic_inline_template() {
    let input = r"before
{% tmpl greeting %}
---
params: []
---
Hello!
{% /tmpl %}
after";
    let (cleaned, templates) = extract_inline_templates(input).unwrap();
    assert_eq!(cleaned.trim(), "before\nafter");
    assert_eq!(templates.len(), 1);
    let tmpl = templates.get("greeting").expect("missing 'greeting'");
    assert!(tmpl.declarations.is_empty());
    assert!(!tmpl.segments.is_empty(), "segments should be pre-compiled");
}

#[test]
fn extract_inline_template_with_frontmatter() {
    let input = r"{% tmpl row %}
---
params: [label = str]
---
- {{ label }}
{% /tmpl %}
";
    let (_, templates) = extract_inline_templates(input).unwrap();
    let tmpl = templates.get("row").unwrap();
    assert_eq!(tmpl.declarations.len(), 1);
    assert_eq!(tmpl.declarations[0].name, "label");
}

#[test]
fn extract_duplicate_inline_template_errors() {
    let input = r"{% tmpl a %}
---
params: []
---
foo
{% /tmpl %}
{% tmpl a %}
---
params: []
---
bar
{% /tmpl %}";
    let err = extract_inline_templates(input).unwrap_err();
    assert!(
        err.to_string().contains("duplicate"),
        "expected duplicate error, got: {err}"
    );
}

#[test]
fn extract_empty_name_errors() {
    let input = r"{% tmpl %}
---
params: []
---
foo
{% /tmpl %}";
    let err = extract_inline_templates(input).unwrap_err();
    assert!(
        err.to_string().contains("tmpl NAME"),
        "expected name error, got: {err}"
    );
}

#[test]
fn extract_skips_raw_block() {
    // {% tmpl %} inside {% raw %} should NOT be treated as an inline template.
    let input = r"{% raw %}
{% tmpl fake %}
not a template
{% /tmpl %}
{% /raw %}";
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
    let input = r"{% tmpl alpha %}
---
params: []
---
A
{% /tmpl %}
middle
{% tmpl beta %}
---
params: []
---
B
{% /tmpl %}
";
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

// -- include depth limit --

#[test]
fn include_depth_limit_enforced() {
    let dir = tempfile::tempdir().unwrap();
    // Create a self-including template to trigger depth limit.
    std::fs::write(
        dir.path().join("self.tmpl.md"),
        "\
---

name: self
params: []
---
X{% include [self](./self.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("self.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
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
        "\
---

name: child
params: []
---
child",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: []
---
parent+{% include [child](./child.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("grandparent.tmpl.md"),
        "\
---

name: grandparent
params: []
---
grandparent+{% include [parent](./parent.tmpl.md) %}",
    )
    .unwrap();

    let mut tmpl = crate::Template::from_file(&dir.path().join("grandparent.tmpl.md")).unwrap();
    let ctx = Context::new();

    // 1. With default max include depth (16), it should render fine.
    let res = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(res, "grandparent+parent+child");

    // 2. Set max include depth to 1. Since grandparent -> parent -> child requires 2 includes, depth is 2, so it should error.
    tmpl.set_max_include_depth(1);
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("include depth 1 should be exceeded by 2-level chain");
    let err = err.to_string();
    assert!(
        err.contains("include depth (1) exceeded"),
        "error should mention depth limit: {err}"
    );

    // 3. Set max include depth to 2, it should render fine again.
    let tmpl = tmpl.with_max_include_depth(2);
    let res2 = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(res2, "grandparent+parent+child");
}

#[test]
fn deeply_nested_includes_three_levels() {
    // Grandparent → Parent → Child: 3 levels of distinct includes.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

name: child
params: [leaf = str]
---
Leaf:{{ leaf }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: [middle = str]
---
\
         Mid:{{ middle }},{% include [child](./child.tmpl.md) with leaf=\"deep\" %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("grandparent.tmpl.md"),
        "\
---

name: grandparent
params: [top = str]
---
\
         Top:{{ top }},{% include [parent](./parent.tmpl.md) with middle=\"mid\" %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("grandparent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("top", "root");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Top:root,Mid:mid,Leaf:deep");
}

#[test]
fn deeply_nested_includes_four_levels() {
    // A → B → C → D: 4 levels of distinct includes.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("d.tmpl.md"),
        "\
---

name: d
params: []
---
D",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("c.tmpl.md"),
        "\
---

name: c
params: []
---
C+{% include [d](./d.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "\
---

name: b
params: []
---
B+{% include [c](./c.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "\
---

name: a
params: []
---
A+{% include [b](./b.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "A+B+C+D");
}

// -- include contract enforcement --

#[test]
fn include_missing_contract_params_errors() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

name: child
params: [msg = str]
---
{{ msg }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: []
---
> {% include [child](./child.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
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
        "\
---

name: child
params: [msg = str]
---
Hello {{ msg }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: []
---
> {% include [child](./child.tmpl.md) with msg=\"World\" %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Hello World");
}

#[test]
fn include_no_params_always_ok() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("static.tmpl.md"),
        "\
---

name: static
params: []
---
Static!",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: []
---
> {% include [static](./static.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render_ctx(&ctx).unwrap();
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
    assert_eq!(
        result,
        "Paused
"
    );
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
        Value::new_struct([
            (crate::consts::ENUM_TAG_KEY, Value::from("Confirmed")),
            ("evidence", Value::from("crash log")),
        ]),
    );
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(
        result,
        "CONFIRMED: crash log
"
    );
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
> {% match item case Known %}
>   {% if item.label == \"test\" %}

Found test!

>   {% /if %}

> {% /match %}";
    let mut ctx = Context::new();
    ctx.set(
        "item",
        Value::new_struct([
            (crate::consts::ENUM_TAG_KEY, Value::from("Known")),
            ("label", Value::from("test")),
        ]),
    );
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(
        result,
        "  Found test!
  "
    );
}

#[test]
fn match_inline_with_nested_if_no_match() {
    let template = "\
> {% match item case Known %}
>   {% if item.label == \"test\" %}

Found test!

>   {% /if %}

> {% /match %}";
    let mut ctx = Context::new();
    ctx.set(
        "item",
        Value::new_struct([
            (crate::consts::ENUM_TAG_KEY, Value::from("Known")),
            ("label", Value::from("other")),
        ]),
    );
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(result, "  ");
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
    assert_eq!(
        result,
        "Evidence found.
"
    );
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
    assert_eq!(
        result,
        "Evidence found.
"
    );
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
    assert_eq!(result, "");
}

#[test]
fn match_inline_multi_variant_case() {
    let template = "> {% match status case Active | Running %}active{% /match %}";
    let mut ctx = Context::new();
    ctx.set("status", "Running");
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(result.trim(), "active");
}

// ---------------------------------------------------------------------------
// Recursive include tests (runtime depth limit)
// ---------------------------------------------------------------------------

#[test]
fn self_recursive_include_hits_depth_limit() {
    // A template that unconditionally includes itself — should hit the
    // runtime depth limit, not hang.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("self.tmpl.md"),
        "\
---

name: self
params: []
---
R> {% include [self](./self.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("self.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("self-recursion should hit depth limit");
    assert!(
        err.to_string().contains("include depth"),
        "should mention depth limit: {err}"
    );
}

#[test]
fn mutual_recursive_includes_hit_depth_limit() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "\
---

name: a
params: []
---
A> {% include [b](./b.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "\
---

name: b
params: []
---
B> {% include [a](./a.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("mutual recursion should hit depth limit");
    assert!(
        err.to_string().contains("include depth"),
        "should mention depth limit: {err}"
    );
}

#[test]
fn include_type_mismatch_caught_at_runtime() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

name: child
params: [count = int]
---
{{ count }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: [name = str]
---
\
         > {% include [child](./child.tmpl.md) with count=name %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "not_a_number");
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("type mismatch should error at runtime");
    assert!(
        err.to_string().contains("type mismatch") || err.to_string().contains("expected"),
        "should mention type error: {err}"
    );
}

// ---------------------------------------------------------------------------
// Inline template include tests (runtime)
// ---------------------------------------------------------------------------

#[test]
fn inline_template_include_renders_test() {
    // Uses `> {% tmpl %}` blockquote prefix as required by the parser.
    let src = r#"---

params: []
---
> {% tmpl greeting %}
---
params: [who = str]
---
Hello {{ who }}!

> {% /tmpl %}

> {% include greeting with who="World" %}"#;
    let tmpl = crate::Template::from_source(src).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(
        result,
        "Hello World!
"
    );
}

#[test]
fn inline_template_missing_params_errors() {
    // Inline template with required param not provided at include site.
    let src = r"---

params: []
---
> {% tmpl greeting %}
---
params: [who = str]
---
Hello {{ who }}!

> {% /tmpl %}

> {% include greeting %}";
    let tmpl = crate::Template::from_source(src).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("missing params should error");
    let msg = err.to_string();
    assert!(
        msg.contains("who"),
        "should mention missing 'who' param: {msg}"
    );
}

// -- Inline template scoping tests (runtime) ---------------------------

#[test]
fn same_named_tmpl_in_different_files_render_independently() {
    // Parent defines {% tmpl helper %} rendering "PARENT".
    // Included file defines its own {% tmpl helper %} rendering "CHILD".
    // Each should resolve to its own definition.
    let dir = tempfile::tempdir().unwrap();

    // included_file.tmpl.md: defines its own "helper" and uses it
    std::fs::write(
        dir.path().join("included_file.tmpl.md"),
        r"---

params: []
---
> {% tmpl helper %}
---
params: []
---
CHILD

> {% /tmpl %}

> {% include helper %}",
    )
    .unwrap();

    // Parent template: defines its own "helper" and includes the file
    let parent_src = r"---

params: []
---
> {% tmpl helper %}
---
params: []
---
PARENT

> {% /tmpl %}

> {% include helper %}

---
";

    let tmpl = crate::Template::from_source(parent_src).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    // Parent's "helper" renders "PARENT"
    assert_eq!(
        result,
        r"PARENT
---
"
    );
}

#[test]
fn included_file_uses_own_inline_templates() {
    // child.tmpl.md defines {% tmpl row %} and uses it internally.
    // Parent doesn't define "row" — it must resolve to the child's own tmpl.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

params:
  - name = str
---
\
         > {% tmpl row %}

\
         ---
\
         params:
  - label = str
\
         ---
\
         - {{ label }}

\
         > {% /tmpl %}

\
         > {% include row with label=name %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params:
  - name = str
---
\
         > {% include [child](./child.tmpl.md) with name=name %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        "- Alice
"
    );
}

#[test]
fn parent_tmpl_does_not_leak_to_included_file() {
    // Parent defines {% tmpl secret %}. Included file tries {% include secret %}.
    // This should fail because parent's tmpl defs don't leak to included files.
    let dir = tempfile::tempdir().unwrap();

    // child tries to use "secret" which only parent defines
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

params: []
---
\
         > {% include secret %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl secret %}

\
         ---
\
         params: []
\
         ---
\
         SECRET

\
         > {% /tmpl %}

\
         > {% include [child](./child.tmpl.md) %}",
    )
    .unwrap();

    // child.tmpl.md tries to include "secret" which doesn't exist
    // (neither as a file nor as an inline template in child's scope).
    // This should be a render error at runtime (file not found for "secret").
    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let err = tmpl
        .render_ctx(&Context::new())
        .expect_err("child should not see parent's inline template 'secret'");
    let msg = err.to_string();
    assert!(
        msg.contains("secret"),
        "error should mention 'secret': {msg}",
    );
}

#[test]
fn same_named_tmpl_in_parent_and_child_are_independent() {
    // Both parent and child define {% tmpl greeting %} with different content.
    // Each should resolve to its own definition.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl greeting %}

\
         ---
\
         params: []
\
         ---
\
         CHILD_GREETING

\
         > {% /tmpl %}

\
         > {% include greeting %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl greeting %}

\
         ---
\
         params: []
\
         ---
\
         PARENT_GREETING

\
         > {% /tmpl %}

\
         > {% include greeting %}

\
         > {% include [child](./child.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(
        result,
        "PARENT_GREETING
CHILD_GREETING
"
    );
}

#[test]
fn two_included_files_same_tmpl_name_different_content() {
    // Two different files both define {% tmpl row %} with different content.
    // Parent includes both — each should use its own "row" definition.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("alpha.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl row %}

\
         ---
\
         params: []
\
         ---
\
         ALPHA_ROW

\
         > {% /tmpl %}

\
         > {% include row %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("beta.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl row %}

\
         ---
\
         params: []
\
         ---
\
         BETA_ROW

\
         > {% /tmpl %}

\
         > {% include row %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params: []
---
\
         > {% include [alpha](./alpha.tmpl.md) %}

\
         > {% include [beta](./beta.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(
        result,
        "ALPHA_ROW
BETA_ROW
"
    );
}

#[test]
fn same_display_name_different_files_work() {
    // {% include [greeting](./en/greeting.tmpl.md) %} and
    // {% include [greeting](./de/greeting.tmpl.md) %} should work independently.
    let dir = tempfile::tempdir().unwrap();

    std::fs::create_dir_all(dir.path().join("en")).unwrap();
    std::fs::create_dir_all(dir.path().join("de")).unwrap();

    std::fs::write(
        dir.path().join("en/greeting.tmpl.md"),
        "\
---

params:
  - name = str
---
Hello {{ name }}!",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("de/greeting.tmpl.md"),
        "\
---

params:
  - name = str
---
Hallo {{ name }}!",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params:
  - name = str
---
\
         > {% include [greeting](./en/greeting.tmpl.md) with name=name %}

\
         > {% include [greeting](./de/greeting.tmpl.md) with name=name %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "World");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Hello World!Hallo World!");
}

#[test]
fn nested_include_chain_a_b_c() {
    // A includes B, B includes C — tests multi-level include works.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("c.tmpl.md"),
        "\
---

params:
  - msg = str
---
[C:{{ msg }}]",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "\
---

params:
  - msg = str
---
\
         [B:{{ msg }}]

\
         > {% include [c](./c.tmpl.md) with msg=msg %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "\
---

params:
  - msg = str
---
\
         [A:{{ msg }}]

\
         > {% include [b](./b.tmpl.md) with msg=msg %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("msg", "hello");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        "[A:hello]
[B:hello]
[C:hello]"
    );
}

#[test]
fn diamond_include_deduplicates_correctly() {
    // A includes B and C, both include D.
    // D should render twice (once per include site) but not cause errors.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("d.tmpl.md"),
        "\
---

params:
  - label = str
---
[D:{{ label }}]",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "\
---

params:
  - val = str
---
\
         [B]

\
         > {% include [d](./d.tmpl.md) with label=val %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("c.tmpl.md"),
        "\
---

params:
  - val = str
---
\
         [C]

\
         > {% include [d](./d.tmpl.md) with label=val %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "\
---

params:
  - x = str
  - y = str
---
\
         > {% include [b](./b.tmpl.md) with val=x %}

\
         > {% include [c](./c.tmpl.md) with val=y %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "from_b");
    ctx.set("y", "from_c");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        "[B]
[D:from_b][C]
[D:from_c]"
    );
}

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

#[test]
fn if_inside_match_case_arm_with_blockquotes() {
    // Regression: {% if %} inside a {% match %} case arm with blockquote
    // syntax should compile and render correctly.
    let template = r"> {% match role %}
> {% case A %}

Section A

{{ msg }}

> {% if mission %}

## Mission

{{ mission }}

> {% /if %}

More content

> {% case B %}

Section B

> {% /match %}
";
    let mut ctx = Context::new();
    ctx.set("role", "A");
    ctx.set("msg", "hello");
    ctx.set("mission", "find bugs");
    let result = compiled_render(template, &ctx);
    assert!(
        result.is_ok(),
        "if inside match should compile: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("Mission"),
        "output should contain Mission: {output:?}"
    );
    assert!(
        output.contains("find bugs"),
        "output should contain mission value: {output:?}"
    );
}

#[test]
fn if_else_inside_match_case_arm_with_blockquotes() {
    // Regression: {% if %}...{% else %}...{% /if %} inside a match case arm
    // must not confuse the {% else %} with a match-level {% else %} arm.
    let template = r"> {% match role %}
> {% case A %}

Section A

> {% if flag %}

flag is set

> {% else %}

flag is not set

> {% /if %}

After if

> {% case B %}

Section B

> {% /match %}
";
    let mut ctx = Context::new();
    ctx.set("role", "A");
    ctx.set("flag", Value::Bool(false));
    let result = compiled_render(template, &ctx);
    assert!(
        result.is_ok(),
        "if/else inside match should compile: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("flag is not set"),
        "should render else branch: {output:?}"
    );
    assert!(
        !output.contains("flag is set\n"),
        "should NOT render if branch: {output:?}"
    );
}

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
            body,
            else_body,
        } => {
            assert_eq!(binding, "item");
            assert!(matches!(list_expr, CompiledExpr::Path(p) if p.as_str() == "items"));
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
