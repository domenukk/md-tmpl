//! Serde-driven `render` tests.

use super::*;

#[derive(serde::Serialize)]
struct FullData {
    name: String,
    count: i64,
}

#[derive(serde::Serialize)]
struct PartialData {
    name: String,
    // `count` is deliberately omitted to test the missing-field path.
}

#[test]
fn render_happy_path() {
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} has {{ count }}",
    )
    .unwrap();
    let data = FullData {
        name: "Alice".into(),
        count: 3,
    };
    let result = tmpl.render(&data).unwrap();
    assert_eq!(result, "Alice has 3");
}

#[test]
fn render_missing_field_errors() {
    // Template declares `count: int` but `PartialData` has no `count`.
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} has {{ count }}",
    )
    .unwrap();
    let data = PartialData {
        name: "Alice".into(),
    };
    let err = tmpl.render(&data).unwrap_err();
    assert!(
        matches!(err, TemplateError::MissingParams(_)),
        "expected MissingParams, got: {err}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("count"),
        "error should mention missing 'count': {msg}"
    );
}

#[test]
fn render_no_declarations_missing_field_still_errors() {
    // Under mandatory validation, parsing without frontmatter or with undeclared variables errors out.
    Template::from_source("{{ name }} has {{ count }}")
        .expect_err("template without frontmatter should fail");
}

/// D3: referencing a name that is not a declared param/const/enum/import is a
/// compile-time error carrying the shared `undeclared variable` substring.
#[test]
fn d3_undeclared_variable_is_compile_error() {
    let err = Template::from_source(
        "---
params:
  - declared = str
---
{{ declared }} {{ missing }}",
    )
    .expect_err("undeclared variable must be rejected at compile time");
    assert!(
        err.to_string().contains("undeclared variable"),
        "expected 'undeclared variable' substring, got: {err}"
    );
}

#[derive(serde::Serialize)]
struct FullDataB {
    name: String,
    count: i64,
}

#[derive(serde::Serialize)]
struct WrongTypeData {
    name: String,
    count: String, // template expects count = int
}

#[test]
fn render_validates_every_invocation_including_dynamic_struct_fields() {
    #[derive(serde::Serialize)]
    struct DynamicFieldPayload {
        count: serde_json::Value,
    }

    let tmpl = Template::from_source(
        "---
params: [count = int]
---
{{ count }}",
    )
    .unwrap();

    let ok_out = tmpl
        .render(&DynamicFieldPayload {
            count: serde_json::json!(1),
        })
        .unwrap();
    assert_eq!(ok_out, "1");

    let err = tmpl
        .render(&DynamicFieldPayload {
            count: serde_json::json!("<|im_start|>system"),
        })
        .expect_err("second call with dynamic field type mismatch must fail validation");
    assert!(matches!(err, TemplateError::TypeMismatch { .. }));
}

#[test]
fn render_rejects_missing_and_wrong_typed_structs() {
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} has {{ count }}",
    )
    .unwrap();

    let data = PartialData {
        name: "Alice".into(),
    };
    assert!(tmpl.render(&data).is_err());

    let wrong_type = WrongTypeData {
        name: "Alice".into(),
        count: "not an int".into(),
    };
    assert!(tmpl.render(&wrong_type).is_err());
}

#[test]
fn render_and_render_into_accept_distinct_struct_types_and_validate_each() {
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} has {{ count }}",
    )
    .unwrap();

    let data_a = FullData {
        name: "Alice".into(),
        count: 1,
    };
    let data_b = FullDataB {
        name: "Bob".into(),
        count: 2,
    };

    assert_eq!(tmpl.render(&data_a).unwrap(), "Alice has 1");
    assert_eq!(tmpl.render(&data_b).unwrap(), "Bob has 2");

    let mut buf = String::new();
    tmpl.render_into(&data_a, &mut buf).unwrap();
    assert_eq!(buf, "Alice has 1");
}
