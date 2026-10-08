//! Basic compilation and inline template extraction tests.

use super::{
    test_helpers::{compile, extract_inline_templates},
    *,
};

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
            filters,
            body,
            else_body,
        } => {
            assert_eq!(binding, "item");
            assert!(matches!(list_expr, CompiledExpr::Path(p) if p.as_str() == "items"));
            assert!(filters.is_empty());
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
        err.to_string().contains("requires a name"),
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
