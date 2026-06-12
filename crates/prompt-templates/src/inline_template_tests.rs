//! Tests for inline template rendering (`{% tmpl name %}...{% /tmpl %}`).

use crate::{context::Context, ctx, template::Template};

#[test]
fn basic_inline_template() {
    let src = concat!(
        "---\n",
        "params: [name = str]\n",
        "---\n",
        "> {% tmpl greeting %}\n",
        "---\n",
        "params: [name = str]\n",
        "---\n",
        "Hello {{ name }}!\n",
        "> {% /tmpl %}\n",
        "> {% include greeting with name=name %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&ctx! { name: "world" }).unwrap();
    assert_eq!(output, "Hello world!\n");
}

#[test]
fn inline_template_with_typed_vars() {
    let src = concat!(
        "---\n",
        "params: [label = str, count = int]\n",
        "---\n",
        "> {% tmpl row %}\n",
        "---\n",
        "params: [label = str, count = int]\n",
        "---\n",
        "- {{ label }}: {{ count }}\n",
        "> {% /tmpl %}\n",
        "> {% include row with label=label, count=count %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&ctx! { label: "bugs", count: 42 }).unwrap();
    assert_eq!(output, "- bugs: 42\n");
}

#[test]
fn inline_template_type_mismatch() {
    let src = concat!(
        "---\n",
        "params: [name = str]\n",
        "---\n",
        "> {% tmpl row %}\n",
        "---\n",
        "params: [count = int]\n",
        "---\n",
        "{{ count }}\n",
        "> {% /tmpl %}\n",
        "> {% include row with count=name %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let err = tmpl.render(&ctx! { name: "oops" }).unwrap_err();
    assert!(
        err.to_string().contains("type mismatch"),
        "expected type mismatch error, got: {err}"
    );
}

#[test]
fn inline_template_for_loop() {
    let src = concat!(
        "---\n",
        "params: [items = list<>]\n",
        "---\n",
        "> {% tmpl item %}\n",
        "---\n",
        "params: [it = str]\n",
        "---\n",
        "- {{ it }}\n",
        "> {% /tmpl %}\n",
        "> {% include item for it in items %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&ctx! { items: ["a", "b", "c"] }).unwrap();
    assert_eq!(output, "- a\n- b\n- c\n");
}

#[test]
fn inline_template_used_twice() {
    let src = concat!(
        "---\n",
        "params: []\n",
        "---\n",
        "> {% tmpl sep %}\n",
        "---\n",
        "params: []\n",
        "---\n",
        "---\n",
        "> {% /tmpl %}\n",
        "A\n",
        "> {% include sep %}\n",
        "B\n",
        "> {% include sep %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&Context::new()).unwrap();
    assert_eq!(output, "A\n---\nB\n---\n");
}

#[test]
fn raw_block_inside_inline_template() {
    let src = concat!(
        "---\n",
        "params: []\n",
        "---\n",
        "> {% tmpl code %}\n",
        "---\n",
        "params: []\n",
        "---\n",
        "> {% raw %}\n",
        "{{ not_processed }}\n",
        "> {% /raw %}\n",
        "> {% /tmpl %}\n",
        "> {% include code %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&Context::new()).unwrap();
    assert!(
        output.contains("{{ not_processed }}"),
        "raw content should be preserved: {output}"
    );
}

#[test]
fn tmpl_inside_raw_is_literal() {
    let src = concat!(
        "---\n",
        "params: []\n",
        "---\n",
        "> {% raw %}\n",
        "{% tmpl fake %}\n",
        "this is not a template\n",
        "{% /tmpl %}\n",
        "> {% /raw %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&Context::new()).unwrap();
    assert!(
        output.contains("{% tmpl fake %}"),
        "tmpl tag inside raw should be literal: {output}"
    );
}

#[test]
fn tmpl_definition_produces_no_output() {
    let src = concat!(
        "---\n",
        "params: []\n",
        "---\n",
        "> {% tmpl unused_but_included_later %}\n",
        "---\n",
        "params: []\n",
        "---\n",
        "invisible\n",
        "> {% /tmpl %}\n",
        "visible\n",
        "> {% include unused_but_included_later %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&Context::new()).unwrap();
    // The definition itself doesn't render; only the include does.
    assert_eq!(output, "visible\ninvisible\n");
}

#[test]
fn comment_in_template_strips_cleanly() {
    let src = "---\nparams: [x = str]\n---\nbefore{# a comment #}after {{ x }}";
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&ctx! { x: "!" }).unwrap();
    assert_eq!(output, "beforeafter !");
}

/// Regression: inline template names ({% tmpl NAME %}) must not trigger
/// "undeclared variable" errors when they're referenced via {% include NAME %}.
/// Bug: `collect_referenced_params` saw "greeting" in `{% include greeting %}`
/// and flagged it as undeclared because inline template names weren't in
/// the declared set.
#[test]
fn inline_template_name_not_flagged_as_undeclared() {
    let src = concat!(
        "---\n",
        "params: [name = str]\n",
        "---\n",
        "> {% tmpl greeting %}\n",
        "---\n",
        "params: [name = str]\n",
        "---\n",
        "Hello {{ name }}!\n",
        "> {% /tmpl %}\n",
        "> {% include greeting with name=name %}",
    );
    // This should NOT fail with "undeclared variable: greeting".
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render(&ctx! { name: "world" }).unwrap();
    assert_eq!(output, "Hello world!\n");
}
