//! Name-collision, loop-binding shadowing, and blockquote-prefix rules.

use super::*;

// -- Rule 11: import stem vs inline template name collision ----------------

#[test]
fn import_stem_conflicts_with_inline_template_name() {
    // If imports: has stem "helper" and there's a {% tmpl helper %} inline,
    // that's an error since they share the same namespace.
    let source = "---
imports: [[helper](./helper.tmpl.md)]
params: [x = str]
allow_unused: true
---
> {% tmpl helper %}

---

params: []
---
inner

> {% /tmpl %}

{{ x }}";
    let err = Template::from_source(source).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("import stem") && msg.contains("conflicts with inline template"),
        "Expected import stem collision error, got: {msg}"
    );
}

// -- Rule 12: param/const name vs inline template name collision -----------

#[test]
fn param_name_conflicts_with_inline_template_name() {
    // A declared param with the same name as an inline template is ambiguous.
    let source = "---
params: [helper = str]
---
> {% tmpl helper %}
---

params: []
---
inner

> {% /tmpl %}

{{ helper }}
";
    let err = Template::from_source(source).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("inline template name 'helper'")
            && msg.contains("conflicts with a declared parameter or constant"),
        "Expected param/tmpl collision error, got: {msg}"
    );
}

#[test]
fn const_name_conflicts_with_inline_template_name() {
    // A declared const with the same name as an inline template is ambiguous.
    let source = r#"---
consts:
  - helper = str := "value"
---
> {% tmpl helper %}
---

params: []
---
inner

> {% /tmpl %}

{{ helper }}
"#;
    let err = Template::from_source(source).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("inline template name 'helper'")
            && msg.contains("conflicts with a declared parameter or constant"),
        "Expected const/tmpl collision error, got: {msg}"
    );
}

// -- Rule 3: param name vs const name collision ---------------------------

#[test]
fn param_name_conflicts_with_const_name() {
    let err = Template::from_source(
        r#"---
params:
  - x = str

consts:
  - x = str := "hi"
---
{{ x }}"#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("parameter name conflicts with constant name"),
        "Expected param/const collision error, got: {msg}"
    );
}

#[test]
fn const_name_conflicts_with_param_name() {
    // Order reversed — const first, param second.
    let err = Template::from_source(
        r#"---
consts:
  - x = str := "hi"

params:
  - x = str
---
{{ x }}"#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("parameter name conflicts with constant name"),
        "Expected const/param collision error, got: {msg}"
    );
}

#[test]
fn param_and_const_different_names_ok() {
    let tmpl = Template::from_source(
        "---
params:
  - x = str

consts:
  - Y = int := 42
---
{{ x }} {{ Y }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "hello");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "hello 42");
}

// -- Rule 13: for-loop binding must not shadow declared names -------------

#[test]
fn for_binding_shadows_param_rejected() {
    let err = Template::from_source(
        "---
params:
  - items = list(name = str)
  - x = str
---
> {% for x in items %}{{ x.name }}

> {% /for %}

{{ x }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'x'"),
        "Expected for-binding shadow error, got: {msg}"
    );
}

#[test]
fn for_binding_shadows_const_rejected() {
    let err = Template::from_source(
        r#"---
consts:
  - x = str := "hi"

params:
  - items = list(name = str)
---
> {% for x in items %}{{ x.name }}

> {% /for %}"#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'x'"),
        "Expected for-binding shadow const error, got: {msg}"
    );
}

#[test]
fn for_binding_shadows_import_rejected() {
    let err = Template::from_source(
        r#"---
imports:
  - "[shared](./shared.tmpl.md)"

params:
  - items = list(name = str)

allow_unused: true
---
> {% for shared in items %}{{ shared.name }}

> {% /for %}"#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'shared'"),
        "Expected for-binding shadow import error, got: {msg}"
    );
}

#[test]
fn for_binding_shadows_inline_tmpl_rejected() {
    let err = Template::from_source(
        "---
params: [items = list(name = str)]
allow_unused: true
---
> {% tmpl greeting %}
---

params: [name = str]
---
hi {{ name }}

> {% /tmpl %}
> {% for greeting in items %}{{ greeting.name }}
> {% /for %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'greeting'"),
        "Expected for-binding shadow tmpl error, got: {msg}"
    );
}

#[test]
fn for_binding_in_nested_if_shadows_param_rejected() {
    let err = Template::from_source(
        r"---
params:
  - items = list(name = str)
  - x = str
  - show = bool
---
> {% if show %}

> {% for x in items %}{{ x.name }}
> {% /for %}

> {% /if %}

{{ x }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'x'"),
        "Expected nested for-binding shadow error, got: {msg}"
    );
}

// -- for-loop binding reuse is ALLOWED ------------------------------------

#[test]
fn sequential_for_loops_same_binding_allowed() {
    let tmpl = Template::from_source(
        "---
params:
  - items = list(name = str)

allow_unused: true
---
> {% for x in items %}{{ x.name }}

> {% /for %}

> {% for x in items %}{{ x.name }}

> {% /for %}",
    )
    .unwrap();
    let ctx = crate::ctx! {
        items: [{ name: "a" }, { name: "b" }]
    };
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        output,
        "a
b
a
b
"
    );
}

#[test]
fn fresh_for_binding_allowed() {
    // A binding name that doesn't conflict with any declared name is fine.
    let tmpl = Template::from_source(
        "---
params:
  - items = list(name = str)

allow_unused: true
---
> {% for item in items %}{{ item.name }}

> {% /for %}",
    )
    .unwrap();
    let ctx = crate::ctx! { items: [{ name: "hello" }] };
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "hello
"
    );
}

#[test]
fn nested_for_loops_different_bindings_allowed() {
    let tmpl = Template::from_source(
        "---
params: [items = list(children = list(name = str))]
allow_unused: true
---
> {% for item in items %}
>   {% for child in item.children %}{{ child.name }}
>   {% /for %}
> {% /for %}",
    )
    .unwrap();
    let ctx = crate::ctx! {
        items: [{ children: [{ name: "leaf" }] }]
    };
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "  leaf
  "
    );
}

// -- Blockquote prefix enforcement ----------------------------------------

#[test]
fn bare_stmt_tag_at_line_start_rejected() {
    let err = Template::from_source(
        "---
params: [x = str]
allow_unused: true
---
{% if x %}yes

> {% /if %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote-prefixed"),
        "Expected blockquote error, got: {msg}"
    );
}

#[test]
fn bare_for_tag_at_line_start_rejected() {
    let err = Template::from_source(
        "---
params: [items = list(name = str)]
allow_unused: true
---
{% for item in items %}{{ item.name }}

> {% /for %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote-prefixed"),
        "Expected blockquote error, got: {msg}"
    );
}

#[test]
fn indented_bare_stmt_tag_rejected() {
    let err = Template::from_source(
        "---
params: [x = str]
allow_unused: true
---
  {% if x %}yes

> {% /if %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote-prefixed"),
        "Expected blockquote error for indented tag, got: {msg}"
    );
}

#[test]
fn midline_stmt_tag_allowed_without_prefix() {
    // {% %} tags in the middle of a line don't need >
    let tmpl = Template::from_source(
        r#"---
params: [x = str]
---
text: {{ x }}{% match x case "a" %} is a{% /match %}"#,
    );
    // This should at least not fail on blockquote validation
    // (it may have other errors depending on x's type, but NOT a blockquote error)
    if let Err(e) = &tmpl {
        assert!(
            !e.to_string().contains("blockquote-prefixed"),
            "Mid-line tag should not require blockquote"
        );
    }
}

#[test]
fn expression_tag_at_line_start_allowed() {
    // {{ }} never needs >
    let tmpl = Template::from_source(
        "---
params: [name = str]
---
{{ name }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "hello");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "hello");
}

#[test]
fn blockquote_prefixed_stmt_tag_works() {
    let tmpl = Template::from_source(
        "---
params: [x = bool]
---
> {% if x %}

yes

> {% else %}

no

> {% /if %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", true);
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "yes
"
    );
}
