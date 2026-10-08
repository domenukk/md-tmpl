//! Serde enum rendering integration tests.

use serde::Serialize;

use super::*;

const ENUM_TEMPLATE: &str = "\
---

params:
  - severity = enum(Critical(reason = str), High, Low)
---
> {% match severity %}
> {% case Critical %}

CRITICAL: {{ severity.reason }}

> {% case High %}

HIGH

> {% case Low %}

LOW

> {% /match %}";

#[derive(Serialize)]
enum Severity {
    Critical { reason: String },
    High,
    Low,
}

#[test]
fn to_value_struct_variant_renders_with_match() {
    let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
    let val = crate::to_value(&Severity::Critical {
        reason: "deadline missed".into(),
    })
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("severity", val);
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "CRITICAL: deadline missed
"
    );
}

#[test]
fn to_value_unit_variant_renders_with_match() {
    let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
    let val = crate::to_value(&Severity::High).unwrap();
    let mut ctx = Context::new();
    ctx.set("severity", val);
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "HIGH
"
    );
}

#[test]
fn ctx_set_with_to_value() {
    let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
    let mut ctx = Context::new();
    ctx.set("severity", crate::to_value(&Severity::Low).unwrap());
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "LOW
"
    );
}

#[test]
fn ctx_macro_with_dict_for_struct_variant() {
    let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
    let ctx = crate::ctx! {
        severity: { __kind__: "Critical", reason: "system outage" }
    };
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "CRITICAL: system outage
"
    );
}

#[test]
fn ctx_macro_with_string_for_unit_variant() {
    let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
    let ctx = crate::ctx! { severity: "High" };
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "HIGH
"
    );
}

#[test]
fn manual_context_set_unit_variant() {
    let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
    let mut ctx = Context::new();
    ctx.set("severity", "Low");
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "LOW
"
    );
}

#[test]
fn manual_context_set_struct_variant() {
    let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
    let mut ctx = Context::new();
    ctx.set(
        "severity",
        Value::new_struct([
            (crate::consts::ENUM_TAG_KEY, Value::Str("Critical".into())),
            ("reason", Value::Str("data corruption".into())),
        ]),
    );
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "CRITICAL: data corruption
"
    );
}

#[test]
fn to_value_struct_variant_with_serde_tag_attr() {
    // Macro-generated enums use #[serde(tag = "__kind__")] — verify this
    // still works (serde routes through serialize_struct, not
    // serialize_struct_variant).
    #[derive(Serialize)]
    #[serde(tag = "__kind__")]
    enum TaggedSeverity {
        Critical { reason: String },
        High,
    }

    let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();

    let val = crate::to_value(&TaggedSeverity::Critical {
        reason: "critical issue".into(),
    })
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("severity", val);
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "CRITICAL: critical issue
"
    );

    // Unit variant with #[serde(tag)] produces {"__kind__": "High"} dict
    let val = crate::to_value(&TaggedSeverity::High).unwrap();
    let mut ctx = Context::new();
    ctx.set("severity", val);
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "HIGH
"
    );
}

#[test]
fn multi_field_struct_variant() {
    #[derive(Serialize)]
    enum Result {
        Ok { msg: String, code: i64 },
        Err,
    }

    let tmpl = Template::from_source(
        "---
params:
  - r = enum(Ok(msg = str, code = int), Err)
---
> {% match r %}

> {% case Ok %}

{{ r.msg }} ({{ r.code }})

> {% case Err %}

ERROR

> {% /match %}",
    )
    .unwrap();

    let val = crate::to_value(&Result::Ok {
        msg: "success".into(),
        code: 200,
    })
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("r", val);
    assert_eq!(
        tmpl.render_ctx(&ctx).unwrap(),
        "success (200)
"
    );

    // Also exercise the Err variant so it's genuinely used.
    let err_val = crate::to_value(&Result::Err).unwrap();
    let mut err_ctx = Context::new();
    err_ctx.set("r", err_val);
    assert_eq!(
        tmpl.render_ctx(&err_ctx).unwrap(),
        "ERROR
"
    );
}
