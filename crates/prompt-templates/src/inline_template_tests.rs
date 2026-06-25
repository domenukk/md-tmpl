//! Tests for inline template rendering (`{% tmpl name %}...{% /tmpl %}`).

use crate::{context::Context, ctx, template::Template};

#[test]
fn basic_inline_template() {
    let src = concat!(
        "\
---
",
        "params: [name = str]\n",
        "\
---
",
        "> {% tmpl greeting %}\n",
        "\
---
",
        "params: [name = str]\n",
        "\
---
",
        "Hello {{ name }}!\n\n",
        "> {% /tmpl %}\n\n",
        "> {% include greeting with name=name %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render_ctx(&ctx! { name: "world" }).unwrap();
    assert_eq!(output, "Hello world!\n");
}

#[test]
fn inline_template_with_typed_vars() {
    let src = concat!(
        "\
---
",
        "params: [label = str, count = int]\n",
        "\
---
",
        "> {% tmpl row %}\n",
        "\
---
",
        "params: [label = str, count = int]\n",
        "\
---
",
        "- {{ label }}: {{ count }}\n\n",
        "> {% /tmpl %}\n\n",
        "> {% include row with label=label, count=count %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl
        .render_ctx(&ctx! { label: "tasks", count: 42 })
        .unwrap();
    assert_eq!(output, "- tasks: 42\n");
}

#[test]
fn inline_template_type_mismatch() {
    let src = concat!(
        "\
---
",
        "params: [name = str]\n",
        "\
---
",
        "> {% tmpl row %}\n",
        "\
---
",
        "params: [count = int]\n",
        "\
---
",
        "{{ count }}\n\n",
        "> {% /tmpl %}\n\n",
        "> {% include row with count=name %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let err = tmpl.render_ctx(&ctx! { name: "oops" }).unwrap_err();
    assert!(
        err.to_string().contains("type mismatch"),
        "expected type mismatch error, got: {err}"
    );
}

#[test]
fn inline_template_for_loop() {
    let src = concat!(
        "\
---
",
        "params: [items = list<str>]\n",
        "\
---
",
        "> {% tmpl item %}\n",
        "\
---
",
        "params: [it = str]\n",
        "\
---
",
        "- {{ it }}\n\n",
        "> {% /tmpl %}\n\n",
        "> {% include item for it in items %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render_ctx(&ctx! { items: ["a", "b", "c"] }).unwrap();
    assert_eq!(output, "- a\n- b\n- c\n");
}

#[test]
fn inline_template_used_twice() {
    let src = concat!(
        "\
---
",
        "params: []\n",
        "\
---
",
        "> {% tmpl sep %}\n",
        "\
---
",
        "params: []\n",
        "\
---
",
        "\
---

",
        "> {% /tmpl %}\n\n",
        "\n",
        "A\n\n",
        "> {% include sep %}\n\n",
        "\n",
        "B\n\n",
        "> {% include sep %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(output, "A\n---\nB\n---\n");
}

#[test]
fn raw_block_inside_inline_template() {
    let src = concat!(
        "\
---
",
        "params: []\n",
        "\
---
",
        "> {% tmpl code %}\n",
        "\
---
",
        "params: []\n",
        "\
---
",
        "> {% raw %}\n",
        "{{ not_processed }}\n\n",
        "> {% /raw %}\n\n",
        "> {% /tmpl %}\n\n",
        "> {% include code %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(output, "{{ not_processed }}\n");
}

#[test]
fn tmpl_inside_raw_is_literal() {
    let src = concat!(
        "\
---
",
        "params: []\n",
        "\
---
",
        "> {% raw %}\n",
        "{% tmpl fake %}\n",
        "this is not a template\n",
        "{% /tmpl %}\n\n",
        "> {% /raw %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(
        output,
        "{% tmpl fake %}\nthis is not a template\n{% /tmpl %}\n"
    );
}

#[test]
fn tmpl_definition_produces_no_output() {
    let src = concat!(
        "\
---
",
        "params: []\n",
        "\
---
",
        "> {% tmpl unused_but_included_later %}\n",
        "\
---
",
        "params: []\n",
        "\
---
",
        "invisible\n\n",
        "> {% /tmpl %}\n\n",
        "visible\n\n",
        "> {% include unused_but_included_later %}",
    );
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render_ctx(&Context::new()).unwrap();
    // The definition itself doesn't render; only the include does.
    assert_eq!(output, "visible\ninvisible\n");
}

#[test]
fn comment_in_template_strips_cleanly() {
    let src = "\
---
params: [x = str]
---
before{# a comment #}after {{ x }}";
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render_ctx(&ctx! { x: "!" }).unwrap();
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
        "\
---
",
        "params: [name = str]\n",
        "\
---
",
        "> {% tmpl greeting %}\n",
        "\
---
",
        "params: [name = str]\n",
        "\
---
",
        "Hello {{ name }}!\n\n",
        "> {% /tmpl %}\n\n",
        "> {% include greeting with name=name %}",
    );
    // This should NOT fail with "undeclared variable: greeting".
    let tmpl = Template::from_source(src).unwrap();
    let output = tmpl.render_ctx(&ctx! { name: "world" }).unwrap();
    assert_eq!(output, "Hello world!\n");
}
