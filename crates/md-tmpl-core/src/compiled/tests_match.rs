//! `match` / `case` statement tests.

use super::test_helpers::compiled_render;
use crate::{context::Context, value::Value};

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
fn match_on_int_value() {
    let template = "> {% match count %}{% case 42 %}found{% else %}nope{% /match %}";
    let mut ctx = Context::new();
    ctx.set("count", Value::from(42));
    let result = compiled_render(template, &ctx).unwrap();
    assert_eq!(result.trim(), "found");
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
    let output = result.expect("if inside match should compile");
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
    let output = result.expect("if/else inside match should compile");
    assert!(
        output.contains("flag is not set"),
        "should render else branch: {output:?}"
    );
    assert!(
        !output.contains("flag is set\n"),
        "should NOT render if branch: {output:?}"
    );
}
