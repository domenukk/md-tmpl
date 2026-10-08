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
fn typeid_caching_validates_once_and_caches_on_success() {
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} has {{ count }}",
    )
    .unwrap();

    assert_eq!(
        tmpl.checked_type_ids.lock().unwrap().len(),
        0,
        "cache should start empty"
    );

    let data1 = FullData {
        name: "Alice".into(),
        count: 3,
    };
    let res1 = tmpl.render(&data1).unwrap();
    assert_eq!(res1, "Alice has 3");
    assert_eq!(
        tmpl.checked_type_ids.lock().unwrap().len(),
        1,
        "cache should have 1 entry after first successful render"
    );

    let data2 = FullData {
        name: "Bob".into(),
        count: 5,
    };
    let res2 = tmpl.render(&data2).unwrap();
    assert_eq!(res2, "Bob has 5");
    assert_eq!(
        tmpl.checked_type_ids.lock().unwrap().len(),
        1,
        "cache should still have 1 entry on subsequent call with same type"
    );
}

#[test]
fn typeid_caching_does_not_cache_on_validation_failure() {
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} has {{ count }}",
    )
    .unwrap();

    assert_eq!(tmpl.checked_type_ids.lock().unwrap().len(), 0);

    let data = PartialData {
        name: "Alice".into(),
    };
    assert!(tmpl.render(&data).is_err());
    assert_eq!(
        tmpl.checked_type_ids.lock().unwrap().len(),
        0,
        "cache should remain empty when validation fails on missing field"
    );

    let wrong_type = WrongTypeData {
        name: "Alice".into(),
        count: "not an int".into(),
    };
    assert!(tmpl.render(&wrong_type).is_err());
    assert_eq!(
        tmpl.checked_type_ids.lock().unwrap().len(),
        0,
        "cache should remain empty when validation fails on type mismatch"
    );
}

#[test]
fn typeid_caching_differentiates_distinct_struct_types() {
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

    tmpl.render(&data_a).unwrap();
    assert_eq!(tmpl.checked_type_ids.lock().unwrap().len(), 1);

    tmpl.render(&data_b).unwrap();
    assert_eq!(
        tmpl.checked_type_ids.lock().unwrap().len(),
        2,
        "different struct types with identical fields should get distinct cache entries"
    );
}

#[test]
fn typeid_caching_shared_between_render_and_render_into() {
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} has {{ count }}",
    )
    .unwrap();

    let data = FullData {
        name: "Alice".into(),
        count: 1,
    };
    tmpl.render(&data).unwrap();
    assert_eq!(tmpl.checked_type_ids.lock().unwrap().len(), 1);

    let mut buf = String::new();
    tmpl.render_into(&data, &mut buf).unwrap();
    assert_eq!(buf, "Alice has 1");
    assert_eq!(
        tmpl.checked_type_ids.lock().unwrap().len(),
        1,
        "render_into should reuse cache established by render"
    );
}
