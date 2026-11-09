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

#[test]
fn precompiled_case_labels_populate_all_four_variants() {
    use crate::compiled::{CompiledCaseLabel, Segment};

    let body = r#"> {% match status %}
> {% case "exact_\"quote\"" | "prefix_{{ tag | upper }}" %}

hit_quoted

> {% case plain_var | cfg.expected %}

hit_unquoted

> {% else %}

hit_default

> {% /match %}
"#;
    let empty_aliases = crate::compat::HashMap::new();
    let (segments, _) = crate::compiled::compile(body, &empty_aliases).unwrap();
    let Some(Segment::Match { arms, .. }) = segments.first() else {
        panic!("expected Segment::Match, got {segments:?}");
    };
    assert_eq!(arms.len(), 3);

    // Arm 0: LiteralStr (with unescaped quotes) + InterpolatedStr (pre-compiled segments).
    assert_eq!(arms[0].compiled_variants.len(), 2);
    assert!(matches!(
        &arms[0].compiled_variants[0],
        CompiledCaseLabel::LiteralStr(s) if s.as_ref() == "exact_\"quote\""
    ));
    assert!(matches!(
        &arms[0].compiled_variants[1],
        CompiledCaseLabel::InterpolatedStr(segs) if segs.len() == 2
    ));

    // Arm 1: Unquoted simple var + Unquoted pre-split dotted path.
    assert_eq!(arms[1].compiled_variants.len(), 2);
    assert!(matches!(
        &arms[1].compiled_variants[0],
        CompiledCaseLabel::Unquoted { raw, dotted_path: None } if raw.as_ref() == "plain_var"
    ));
    assert!(matches!(
        &arms[1].compiled_variants[1],
        CompiledCaseLabel::Unquoted { raw, dotted_path: Some(p) }
            if raw.as_ref() == "cfg.expected" && p.parts() == ["cfg", "expected"]
    ));

    // Arm 2: Default (`{% else %}`).
    assert_eq!(arms[2].compiled_variants.len(), 1);
    assert!(matches!(
        &arms[2].compiled_variants[0],
        CompiledCaseLabel::Default
    ));
}

#[test]
fn malformed_interpolated_case_label_rejected_at_compile_time() {
    let unclosed_src = r#"---
params:
  - s = str
  - x = str
---
> {% match s %}
> {% case "ok" %}

matched

> {% case "bad {{ x" %}

unreachable

> {% else %}

other

> {% /match %}
"#;
    let err = crate::Template::from_source(unclosed_src).unwrap_err();
    assert!(
        err.to_string().contains("unclosed"),
        "expected compile-time unclosed error, got: {err}"
    );

    let unknown_filter_src = r#"---
params:
  - s = str
  - x = str
---
> {% match s case "{{ x | bogus_filter_xyz }}" %}

hit

> {% /match %}
"#;
    let err2 = crate::Template::from_source(unknown_filter_src).unwrap_err();
    assert!(
        err2.to_string().contains("unknown filter"),
        "expected compile-time unknown filter error, got: {err2}"
    );
}
