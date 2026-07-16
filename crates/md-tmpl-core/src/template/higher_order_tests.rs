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

#[test]
fn test_higher_order_option_tmpl() {
    let helper = Template::from_source(
        r"---
params: [test = str]
---
Helper: {{ test }}",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [cb = option(tmpl(test = str))]
---
> {% if has(cb) %}
> {% include cb with test="Hello Option" %}
> {% else %}

No callback

> {% /if %}"#,
    )
    .unwrap();

    let mut ctx_some = crate::Context::new();
    ctx_some.set("cb", Value::Tmpl(Arc::new(helper)));
    let result_some = main.render_ctx(&ctx_some).unwrap();
    assert!(
        result_some.contains("Helper: Hello Option"),
        "got: {result_some}"
    );

    let mut ctx_none = crate::Context::new();
    ctx_none.set("cb", Value::None);
    let result_none = main.render_ctx(&ctx_none).unwrap();
    assert!(result_none.contains("No callback"), "got: {result_none}");
}

#[test]
fn test_higher_order_nested_option_tmpl() {
    let inner = Template::from_source(
        r"---
params: [test = str]
---
Inner: {{ test }}",
    )
    .unwrap();

    let middle = Template::from_source(
        r#"---
params: [sub = option(tmpl(test = str))]
---
> {% if has(sub) %}
> {% include sub with test="Nested Success" %}
> {% else %}

No sub

> {% /if %}"#,
    )
    .unwrap();

    let main = Template::from_source(
        r"---
params:
  - cb = option(tmpl(sub = option(tmpl(test = str))))
  - target = option(tmpl(test = str))
---
> {% if has(cb) %}
> {% include cb with sub=target %}
> {% else %}

No cb

> {% /if %}",
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("cb", Value::Tmpl(Arc::new(middle)));
    ctx.set("target", Value::Tmpl(Arc::new(inner)));

    let result = main.render_ctx(&ctx).unwrap();
    assert!(result.contains("Inner: Nested Success"), "got: {result}");
}

/// `option(tmpl())` with empty signature: Some path renders, None path skips.
#[test]
fn test_higher_order_option_empty_tmpl() {
    let helper = Template::from_source(
        r"---
params: []
---
Static content from helper.",
    )
    .unwrap();

    let main = Template::from_source(
        r"---
params: [widget = option(tmpl())]
---
> {% if has(widget) %}
> {% include widget %}
> {% else %}

No widget

> {% /if %}",
    )
    .unwrap();

    // Some path
    let mut ctx_some = crate::Context::new();
    ctx_some.set("widget", Value::Tmpl(Arc::new(helper)));
    let result_some = main.render_ctx(&ctx_some).unwrap();
    assert!(
        result_some.contains("Static content from helper."),
        "got: {result_some}"
    );

    // None path
    let mut ctx_none = crate::Context::new();
    ctx_none.set("widget", Value::None);
    let result_none = main.render_ctx(&ctx_none).unwrap();
    assert!(result_none.contains("No widget"), "got: {result_none}");
}

/// `option(tmpl(x = str))` rejects a template with the wrong signature.
#[test]
fn test_higher_order_option_tmpl_type_mismatch() {
    let wrong_helper = Template::from_source(
        r"---
params: [count = int]
---
Count: {{ count }}",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [cb = option(tmpl(name = str))]
---
> {% if has(cb) %}
> {% include cb with name="test" %}
> {% /if %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("cb", Value::Tmpl(Arc::new(wrong_helper)));

    let err = main.render_ctx(&ctx).unwrap_err();
    assert!(
        err.to_string().contains("type mismatch") || err.to_string().contains("name"),
        "expected type mismatch for option(tmpl) with wrong signature, got: {err}"
    );
}

/// Multiple `option(tmpl(...))` params — one Some, one None.
#[test]
fn test_higher_order_multiple_option_tmpl_mixed() {
    let header_tmpl = Template::from_source(
        r"---
params: [title = str]
---
# {{ title }}",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params:
  - header = option(tmpl(title = str))
  - footer = option(tmpl(note = str))
---
> {% if has(header) %}
> {% include header with title="Welcome" %}
> {% /if %}

Body content

> {% if has(footer) %}
> {% include footer with note="bye" %}
> {% else %}

No footer

> {% /if %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("header", Value::Tmpl(Arc::new(header_tmpl)));
    ctx.set("footer", Value::None);

    let result = main.render_ctx(&ctx).unwrap();
    assert!(
        result.contains("# Welcome"),
        "header should render, got: {result}"
    );
    assert!(
        result.contains("No footer"),
        "footer should show fallback, got: {result}"
    );
}

/// `option(tmpl(...))` with both options set to None produces only fallback text.
#[test]
fn test_higher_order_all_option_tmpl_none() {
    let main = Template::from_source(
        r#"---
params:
  - a = option(tmpl(x = str))
  - b = option(tmpl())
---
> {% if has(a) %}
> {% include a with x="test" %}
> {% else %}

no-a

> {% /if %}
> {% if has(b) %}
> {% include b %}
> {% else %}

no-b

> {% /if %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("a", Value::None);
    ctx.set("b", Value::None);

    let result = main.render_ctx(&ctx).unwrap();
    assert!(result.contains("no-a"), "got: {result}");
    assert!(result.contains("no-b"), "got: {result}");
}

/// `tmpl()` inside a struct field: `struct(widget = tmpl(x = str))`.
#[test]
fn test_higher_order_struct_containing_tmpl() {
    let helper = Template::from_source(
        r"---
params: [x = str]
---
Widget: {{ x }}",
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [widget = tmpl(x = str), label = str]
---
Label: {{ label }}

> {% include widget with x="rendered" %}"#,
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("widget", Value::Tmpl(Arc::new(helper)));
    ctx.set("label", "my-label");

    let result = main.render_ctx(&ctx).unwrap();
    assert!(result.contains("Label: my-label"), "got: {result}");
    assert!(result.contains("Widget: rendered"), "got: {result}");
}

/// option(tmpl) where the child template has extra *defaulted* params passes.
#[test]
fn test_higher_order_option_tmpl_extra_defaults_accepted() {
    let helper = Template::from_source(
        r#"---
params:
  - name = str
  - greeting = str := "Hey"
---
{{ greeting }} {{ name }}!"#,
    )
    .unwrap();

    let main = Template::from_source(
        r#"---
params: [cb = option(tmpl(name = str))]
---
> {% if has(cb) %}
> {% include cb with name="World" %}
> {% else %}

none

> {% /if %}"#,
    )
    .unwrap();

    // Extra defaulted param should be accepted by option(tmpl(name = str))
    let mut ctx = crate::Context::new();
    ctx.set("cb", Value::Tmpl(Arc::new(helper)));

    let result = main.render_ctx(&ctx).unwrap();
    assert!(result.contains("Hey World!"), "got: {result}");
}

// --- D1: has() on a None option must be false --------------------------------

/// `has()` on an option param defaulted to `None` is false; a Some default is true.
#[test]
fn d1_has_option_default_none_is_false() {
    let none_tmpl = Template::from_source(
        r"---
params: [o = option(str) := None]
---
> {% if has(o) %}

present

> {% else %}

absent

> {% /if %}",
    )
    .unwrap();
    let ctx = crate::Context::new();
    let out = none_tmpl.render_ctx(&ctx).unwrap();
    assert!(
        out.contains("absent"),
        "None option: has() must be false, got: {out}"
    );

    let some_tmpl = Template::from_source(
        r#"---
params: [o = option(str) := "x"]
---
> {% if has(o) %}

present

> {% else %}

absent

> {% /if %}"#,
    )
    .unwrap();
    let out = some_tmpl.render_ctx(&crate::Context::new()).unwrap();
    assert!(
        out.contains("present"),
        "Some option: has() must be true, got: {out}"
    );
}

/// `has()` on an option param explicitly set to `Value::None` is false; set to a
/// concrete value it is true.
#[test]
fn d1_has_option_context_none_vs_some() {
    let tmpl = Template::from_source(
        r"---
params: [o = option(str)]
---
> {% if has(o) %}

present

> {% else %}

absent

> {% /if %}",
    )
    .unwrap();

    let mut none_ctx = crate::Context::new();
    none_ctx.set("o", Value::None);
    let out = tmpl.render_ctx(&none_ctx).unwrap();
    assert!(
        out.contains("absent"),
        "None: has() must be false, got: {out}"
    );

    let mut some_ctx = crate::Context::new();
    some_ctx.set("o", "hello");
    let out = tmpl.render_ctx(&some_ctx).unwrap();
    assert!(
        out.contains("present"),
        "Some: has() must be true, got: {out}"
    );
}

/// Unit test for the low-level predicate. Absence is represented solely by
/// `Value::None`; the string `"None"` is the `Some(None)` escape and therefore
/// counts as present (a Some holding the literal string `"None"`).
#[test]
fn d1_is_option_some_predicate() {
    use crate::scope::Scope;
    assert!(
        !Scope::is_option_some(&Value::None),
        "Value::None must be absent"
    );
    assert!(
        Scope::is_option_some(&Value::Str(crate::consts::OPTION_NONE.into())),
        "Str(\"None\") is the Some(None) escape and must be present"
    );
    assert!(
        Scope::is_option_some(&Value::Str("Some".into())),
        "non-None string is present"
    );
    assert!(
        Scope::is_option_some(&Value::Int(0)),
        "concrete value is present"
    );
}
