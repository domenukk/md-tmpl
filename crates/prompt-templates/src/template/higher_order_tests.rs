use std::sync::Arc;

use crate::{Template, Value};

#[test]
fn test_higher_order_template() {
    let helper = Template::from_source(
        r"---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [test = tmpl(name = str)]
---
> {% include test with name="World" %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("test", Value::Tmpl(Arc::new(helper)));

    let result = main.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Hello World!");
}

#[test]
fn test_higher_order_template_type_mismatch() {
    let helper = Template::from_source(
        r"---
params: [age = int]
---
Age: {{ age }}",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [test = tmpl(name = str)]
---
> {% include test with name="World" %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("test", Value::Tmpl(Arc::new(helper)));

    let err = main.render_ctx(&ctx).unwrap_err();
    eprintln!("ACTUAL ERROR: {err}");
    assert!(
        err.to_string().contains("type mismatch")
            && err.to_string().contains("test")
            && err.to_string().contains("name")
            && err.to_string().contains("expected str"),
        "expected type mismatch error for 'test' at '.name', got: {err}"
    );
}

#[test]
fn test_higher_order_template_with_defaults() {
    let helper = Template::from_source(
        r#"---
params:
  - name = str
  - greeting = str := "Hi"
---
{{ greeting }} {{ name }}!"#,
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [test = tmpl(name = str)]
---
> {% include test with name="World" %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("test", Value::Tmpl(Arc::new(helper)));

    let result = main.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Hi World!");
}

#[test]
fn test_higher_order_template_nested() {
    let inner = Template::from_source(
        r"---
params: [val = str]
---
Inner: {{ val }}",
    )
    .unwrap();

    let middle = Template::from_source(
        r"---
params:
  - target = tmpl(val = str)
  - value = str
---
> {% include target with val=value %}",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params:
  - processor = tmpl(target = tmpl(val = str), value = str)
  - callback = tmpl(val = str)
---
> {% include processor with target=callback, value="Success" %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("processor", Value::Tmpl(Arc::new(middle)));
    ctx.set("callback", Value::Tmpl(Arc::new(inner)));

    let result = main.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Inner: Success");
}

/// `tmpl()` with empty parens accepts a template with no required params.
/// This is the pattern ARTIST uses for the `preamble` parameter.
#[test]
fn test_higher_order_empty_tmpl() {
    let helper = Template::from_source(
        r"---
params: []
---
Preamble content here.",
    )
    .unwrap();

    let main = Template::from_source(
        r"---
params: [preamble = tmpl()]
---

> {% include preamble %}

Done.",
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("preamble", Value::Tmpl(Arc::new(helper)));

    let result = main.render_ctx(&ctx).unwrap();
    assert!(result.contains("Preamble content here."), "got: {result}");
    assert!(result.contains("Done."), "got: {result}");
}

/// A template with extra defaulted params matches a `tmpl()` signature that
/// doesn't list them — the defaults are used automatically.
#[test]
fn test_higher_order_extra_defaulted_params_match() {
    let helper = Template::from_source(
        r#"---
params:
  - x = str
  - extra = str := "fallback"
---
{{ x }} {{ extra }}"#,
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [widget = tmpl(x = str)]
---
> {% include widget with x="hello" %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("widget", Value::Tmpl(Arc::new(helper)));

    let result = main.render_ctx(&ctx).unwrap();
    assert_eq!(result, "hello fallback");
}

/// A template with extra REQUIRED params (no default) does NOT match a
/// `tmpl()` signature that doesn't list them — this is a type mismatch.
#[test]
fn test_higher_order_extra_required_params_reject() {
    let helper = Template::from_source(
        r"---
params:
  - x = str
  - extra = str
---
{{ x }} {{ extra }}",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [widget = tmpl(x = str)]
---
> {% include widget with x="hello" %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("widget", Value::Tmpl(Arc::new(helper)));

    let err = main.render_ctx(&ctx).unwrap_err();
    assert!(
        err.to_string().contains("type mismatch") || err.to_string().contains("extra"),
        "expected type mismatch for extra required param, got: {err}"
    );
}

/// Full external flow: outer template declares `tmpl(x = str, y = int)`,
/// passes multiple params via `with`, and the inner template uses them.
#[test]
fn test_higher_order_multi_param_forwarding() {
    let helper = Template::from_source(
        r"---
params:
  - x = str
  - y = int
---
{{ x }}-{{ y }}",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params:
  - widget = tmpl(x = str, y = int)
  - label = str
  - num = int
---
Label: {{ label }}

> {% include widget with x="hello", y=num %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("widget", Value::Tmpl(Arc::new(helper)));
    ctx.set("label", "test");
    ctx.set("num", 42);

    let result = main.render_ctx(&ctx).unwrap();
    assert!(result.contains("Label: test"), "got: {result}");
    assert!(result.contains("hello-42"), "got: {result}");
}

/// Passing a non-template value for a `tmpl()` param must be rejected.
#[test]
fn test_higher_order_non_template_value_rejected() {
    let main = Template::from_source(
        r#"---
params: [widget = tmpl(name = str)]
---
> {% include widget with name="test" %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    // Pass a plain string instead of a Template — should error.
    ctx.set("widget", "not a template");

    let err = main.render_ctx(&ctx).unwrap_err();
    assert!(
        err.to_string().contains("type mismatch")
            || err.to_string().contains("widget")
            || err.to_string().contains("tmpl"),
        "expected type mismatch for non-template value, got: {err}"
    );
}

/// Using `{{ widget }}` with a `tmpl()` param errors at render time
/// (templates are not directly renderable — use `{% include %}` instead).
///
/// NOTE: The SPEC says this should be a compile-time error, and it IS
/// rejected at compile-time through the `compile()` / proc-macro path
/// (validated by `is_displayable()` in `type_check.rs`). The `from_source()`
/// interpretation path only catches this at render time.
#[test]
fn test_higher_order_display_tmpl_rejected_at_render() {
    let main = Template::from_source(
        r"---
params: [widget = tmpl()]
---
{{ widget }}",
    )
    .unwrap();

    let helper = Template::from_source(
        r"---
params: []
---
content",
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("widget", Value::Tmpl(Arc::new(helper)));

    let err = main.render_ctx(&ctx).unwrap_err();
    assert!(
        err.to_string().contains("display")
            || err.to_string().contains("cannot")
            || err.to_string().contains("tmpl"),
        "expected display error for tmpl() param, got: {err}"
    );
}

/// `tmpl()` param is truthy when set (should work in {% if %} guards).
#[test]
fn test_higher_order_tmpl_is_truthy() {
    let helper = Template::from_source(
        r"---
params: []
---
present",
    )
    .unwrap();

    let main = Template::from_source(
        r"---
params: [widget = tmpl()]
---
> {% if widget %}

yes

> {% /if %}",
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("widget", Value::Tmpl(Arc::new(helper)));

    let result = main.render_ctx(&ctx).unwrap();
    assert!(
        result.contains("yes"),
        "tmpl should be truthy, got: {result}"
    );
}
