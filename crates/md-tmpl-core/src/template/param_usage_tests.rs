//! Comment variable-reference tracking and option requiredness tests.

use super::*;
use crate::value::Value;

// -- comment variable reference tracking -----------------------------------

#[test]
fn comment_multiple_refs_in_one_comment() {
    // Multiple {{ }} refs in one comment all count as used.
    let tmpl = Template::from_source(
        "---
params: [a = str, b = int, c = bool]
---
> {# unused: {{ a }}, {{ b }}, {{ c }} #}",
    )
    .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    assert!(
        referenced.contains("a"),
        "a should be referenced: {referenced:?}"
    );
    assert!(
        referenced.contains("b"),
        "b should be referenced: {referenced:?}"
    );
    assert!(
        referenced.contains("c"),
        "c should be referenced: {referenced:?}"
    );
}

#[test]
fn comment_dotted_path_tracks_root() {
    // {{ item.label }} in a comment tracks the root "item".
    let tmpl = Template::from_source(
        "---
params: [name = str, item = struct(label = str)]
---
{{ name }}

> {# {{ item.label }} #}",
    )
    .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    assert!(
        referenced.contains("item"),
        "item should be referenced via dotted path: {referenced:?}"
    );
}

#[test]
fn comment_bare_name_not_tracked() {
    // Bare variable names (without {{ }}) in comments are NOT tracked.
    let err = Template::from_source(
        "---
params: [name = str, extra = str]
---
{{ name }}
> {# uses: extra #}",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("extra"),
        "bare name in comment should NOT suppress unused error: {err}"
    );
}

#[test]
fn comment_descriptive_unused_pattern() {
    // The recommended `{# unused: {{ var }} #}` pattern works.
    let tmpl = Template::from_source(
        "---
params: [name = str, role_type = str, agent_name = str]
---
{{ name }}

> {# unused: {{ role_type }}, {{ agent_name }} #}",
    )
    .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    assert!(referenced.contains("role_type"));
    assert!(referenced.contains("agent_name"));
}

#[test]
fn comment_blockquote_rules_enforced() {
    let err = Template::from_source(
        "---
params: []
---
{# line start #}",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains(crate::consts::ERR_BARE_COMMENT),
        "got: {err}"
    );

    let err2 = Template::from_source(
        "---
params: []
---
> {#nospace#}",
    )
    .unwrap_err();
    assert!(
        err2.to_string().contains("spaces around the content"),
        "got: {err2}"
    );
}

#[test]
fn statement_tag_spacing_enforced() {
    let err = Template::from_source(
        "---
params: [x = bool]
---
> {%if x%}hello{%/if%}",
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("Statement tags must have spaces around the content"),
        "got: {err}"
    );
}

#[test]
fn comment_mixed_with_statement_tag_on_same_line() {
    let tmpl = Template::from_source(
        "---
params: [flag = bool]
---
> {# comment explaining complex if #}{% if flag %}hello{% /if %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("flag", true);
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "hello");
}

#[test]
fn multiple_comments_on_same_line() {
    let tmpl = Template::from_source(
        "---
params: [a = str, b = str]
---
> {# first comment: {{ a }} #}text{# second comment: {{ b }} #}",
    )
    .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    assert!(referenced.contains("a"));
    assert!(referenced.contains("b"));
    let mut ctx = Context::new();
    ctx.set("a", "1");
    ctx.set("b", "2");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "text");
}

// -- option(T) required vs optional ----------------------------------------

#[test]
fn option_without_default_is_required_missing_param_errors() {
    let tmpl = Template::from_source(
        "---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}",
    )
    .unwrap();
    let ctx = Context::new();
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    assert!(
        matches!(err, TemplateError::MissingParams(_)),
        "option without default should be required: {err}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains('x'),
        "error should mention the missing param 'x': {msg}"
    );
}

#[test]
fn option_without_default_providing_value_works() {
    let tmpl = Template::from_source(
        "---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", 42_i64);
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("42"),
        "should render the value, got: {output}"
    );
}

#[test]
fn option_without_default_providing_none_works() {
    let tmpl = Template::from_source(
        "---
params:
  - x = option(int)
---
> {% if has(x) %}

present

> {% else %}

absent

> {% /if %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", Value::None);
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("absent"),
        "Value::None should trigger else branch, got: {output}"
    );
}

#[test]
fn option_with_none_default_omitting_param_is_ok() {
    let tmpl = Template::from_source(
        "---
params:
  - x = option(int) := None
---
> {% if has(x) %}

present

> {% else %}

absent

> {% /if %}",
    )
    .unwrap();
    let ctx = Context::new();
    // Default None applied — no MissingParams error
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("absent"),
        "default None should render absent, got: {output}"
    );
}

#[test]
fn option_with_none_default_can_override_with_value() {
    let tmpl = Template::from_source(
        "---
params:
  - x = option(int) := None
---
> {% if has(x) %}

{{ x }}

> {% else %}

absent

> {% /if %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", 99_i64);
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("99"),
        "overriding None default should work, got: {output}"
    );
}

#[test]
fn defaults_does_not_include_option_without_default() {
    let tmpl = Template::from_source(
        r#"---
params:
  - x = option(int)
  - y = option(str) := None
  - z = option(str) := "hi"
---
> {% if has(x) %}

{{ x }}

> {% /if %}{% if has(y) %}{{ y }}{% /if %}{% if has(z) %}{{ z }}{% /if %}"#,
    )
    .unwrap();
    let defaults = tmpl.defaults();
    assert!(
        !defaults.contains_key("x"),
        "x should not have a default: {defaults:?}"
    );
    assert_eq!(
        defaults.get("y"),
        Some(&Value::None),
        "y should default to None"
    );
    assert_eq!(
        defaults.get("z"),
        Some(&Value::Str("hi".into())),
        "z should default to 'hi'"
    );
}
