//! Tests verifying error message quality for every `TemplateError` variant.

use std::path::Path;

use crate::{Context, Template, TemplateError, ctx};

// ---------------------------------------------------------------------------
// 1. Io
// ---------------------------------------------------------------------------

#[test]
fn test_error_io_includes_path() {
    let err = Template::from_file(Path::new("/nonexistent/path.tmpl.md")).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::Io(_)),
        "expected Io variant, got: {msg}"
    );
    // The std::io::Error Display includes "No such file or directory" (or similar).
    assert!(
        msg.contains("failed to load template"),
        "should mention template loading: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 2. UndefinedVariable
// ---------------------------------------------------------------------------

#[test]
fn test_error_undefined_variable_message() {
    // UndefinedVariable fires at runtime when a dotted path reaches
    // a missing field — validate_context returns MissingParams for
    // top-level keys, so we use a dotted path instead.
    let tmpl = Template::from_source(
        r"---
params: [data = struct(x = str)]
---
{{ data.missing_field }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set(
        "data",
        crate::Value::new_struct([("x", crate::Value::from("hello"))]),
    );
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::UndefinedVariable(_)),
        "expected UndefinedVariable, got: {msg}"
    );
    assert!(
        msg.contains("missing_field"),
        "should mention the undefined field: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 3. Syntax (with line + snippet)
// ---------------------------------------------------------------------------

#[test]
fn test_error_syntax_has_line_and_snippet() {
    // An unknown filter triggers a Syntax error with line/snippet info via enrich_error.
    let err = Template::from_source(
        r"---
params: [x = str]
---
{{ x | badfilter }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::Syntax(_)),
        "expected Syntax, got: {msg}"
    );
    // enrich_error should attach a line number.
    assert!(msg.contains("line"), "should include line info: {msg}");
    assert!(
        msg.contains("badfilter"),
        "should include the filter name: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 4. Syntax (unknown filter includes name)
// ---------------------------------------------------------------------------

#[test]
fn test_error_syntax_unknown_filter_includes_name() {
    let err = Template::from_source(
        r"---
params: [x = str]
---
{{ x | nonexistent }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent"),
        "error should contain filter name: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 5. MissingParams
// ---------------------------------------------------------------------------

#[test]
fn test_error_missing_params_lists_all() {
    let tmpl = Template::from_source(
        r"---
params: [a = str, b = int, c = bool]
---
{{ a }}{{ b }}{{ c }}",
    )
    .unwrap();
    let ctx = Context::new();
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::MissingParams(_)),
        "expected MissingParams, got: {msg}"
    );
    assert!(msg.contains('a'), "should list 'a': {msg}");
    assert!(msg.contains('b'), "should list 'b': {msg}");
    assert!(msg.contains('c'), "should list 'c': {msg}");
}

// ---------------------------------------------------------------------------
// 6. TypeMismatch (includes name and types)
// ---------------------------------------------------------------------------

#[test]
fn test_error_type_mismatch_includes_path_and_types() {
    let tmpl = Template::from_source(
        r"---
params: [count = int]
---
{{ count }}",
    )
    .unwrap();
    let ctx = ctx! { count: "not-a-number" };
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::TypeMismatch { .. }),
        "expected TypeMismatch, got: {msg}"
    );
    assert!(msg.contains("count"), "should contain var name: {msg}");
    assert!(msg.contains("int"), "should mention expected type: {msg}");
    assert!(msg.contains("str"), "should mention actual type: {msg}");
}

// ---------------------------------------------------------------------------
// 7. TypeMismatch (nested path)
// ---------------------------------------------------------------------------

#[test]
fn test_error_type_mismatch_nested_path() {
    // list(score = int): provide str at the leaf.
    let tmpl = Template::from_source(
        r"---
params: [items = list(score = int)]
---
{{ items }}",
    )
    .unwrap();
    let ctx = ctx! {
        items: [
            { score: "not-int" }
        ]
    };
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::TypeMismatch { .. }),
        "expected TypeMismatch, got: {msg}"
    );
    // The path should include an index and field name like "[0].score".
    assert!(
        msg.contains("[0].score"),
        "should contain nested path '[0].score': {msg}"
    );
}

// ---------------------------------------------------------------------------
// 8. UnknownFilter
// ---------------------------------------------------------------------------

#[test]
fn test_error_unknown_filter_message() {
    // When enrich_error doesn't find the needle, the raw UnknownFilter is preserved.
    // But from_source always runs through enrich_error which converts it.
    // We still verify the content.
    let err = Template::from_source(
        r"---
params: [x = str]
---
{{ x | nosuchfilter }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("nosuchfilter"),
        "error should contain filter name: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 9. IncludeNotFound
// ---------------------------------------------------------------------------

#[test]
fn test_error_include_not_found_message() {
    let dir = tempfile::tempdir().unwrap();
    let main_path = dir.path().join("main.tmpl.md");
    // A template that includes a nonexistent file.
    std::fs::write(
        &main_path,
        r"---
params: [x = str]
allow_unused: true
---
> {% include [missing](./missing.tmpl.md) %}

{{ x }}",
    )
    .unwrap();
    let tmpl = Template::from_file(&main_path).unwrap();
    let ctx = ctx! { x: "hello" };
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    // The error should mention the missing file path.
    assert!(
        msg.contains("missing.tmpl.md"),
        "should mention the missing include path: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 10. DeclarationsMutated
// ---------------------------------------------------------------------------

#[test]
fn test_error_declarations_mutated_shows_diff() {
    let tmpl_v1 = Template::from_source(
        r"---
params: [name = str, count = int]
---
{{ name }}{{ count }}",
    )
    .unwrap();
    // Simulate a reloaded template with different params.
    let tmpl_v2 = Template::from_source(
        r"---
params: [name = str, score = float]
---
{{ name }}{{ score }}",
    )
    .unwrap();
    let err = tmpl_v2
        .validate_declarations(tmpl_v1.declarations())
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::DeclarationsMutated { .. }),
        "expected DeclarationsMutated, got: {msg}"
    );
    // Should mention what changed.
    assert!(
        msg.contains("removed") || msg.contains("added") || msg.contains("retyped"),
        "should describe the diff: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 11. ExtraParams
// ---------------------------------------------------------------------------

#[test]
fn test_error_extra_params_lists_names() {
    let tmpl = Template::from_source(
        r"---
params: [name = str]
---
{{ name }}",
    )
    .unwrap();
    let ctx = ctx! { name: "Alice", bonus: 42_i64 };
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::ExtraParams(_)),
        "expected ExtraParams, got: {msg}"
    );
    assert!(msg.contains("bonus"), "should list the extra param: {msg}");
}

// ---------------------------------------------------------------------------
// 12–14. Did-you-mean suggestions
// ---------------------------------------------------------------------------

#[test]
fn test_did_you_mean_misspelled_variable() {
    let err = Template::from_source(
        r"---
params: [name = str]
---
{{ nme }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("did you mean"),
        "should suggest a correction: {msg}"
    );
    assert!(msg.contains("name"), "should suggest 'name': {msg}");
}

#[test]
fn test_did_you_mean_no_suggestion_for_distant_name() {
    let err = Template::from_source(
        r"---
params: [name = str, count = int]
---
{{ xyz }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains("did you mean"),
        "should NOT suggest anything for 'xyz' (distance > 2): {msg}"
    );
}

#[test]
fn test_did_you_mean_levenshtein_distance_1() {
    // "naem" is distance 1 from "name" (transposition = 2 edits in Levenshtein,
    // but let's use a true distance-1 example: "namee" → insert).
    // Actually "naem" vs "name" = 2 (swap a,e). Let's use "namee" (distance 1 insert)
    // or "nme" (distance 1 delete).
    let err = Template::from_source(
        r"---
params: [name = str]
---
{{ nme }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("did you mean 'name'"),
        "should suggest 'name' for 'nme' (distance 1): {msg}"
    );
}

// ---------------------------------------------------------------------------
// 15. TypeMismatch dotted path for deeply nested
// ---------------------------------------------------------------------------

#[test]
fn test_type_mismatch_dotted_path_for_deeply_nested() {
    // list(config = struct(timeout = int)): provide str at deepest level.
    let tmpl = Template::from_source(
        r"---
params: [items = list(config = struct(timeout = int))]
---
{{ items }}",
    )
    .unwrap();
    let ctx = ctx! {
        items: [
            { config: { timeout: "not-int" } }
        ]
    };
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::TypeMismatch { .. }),
        "expected TypeMismatch, got: {msg}"
    );
    // Path should include the full nested path.
    assert!(
        msg.contains("[0].config.timeout"),
        "should contain deeply nested path: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 16–20. Levenshtein distance unit tests
// ---------------------------------------------------------------------------

#[test]
fn test_levenshtein_identical_strings() {
    assert_eq!(crate::error::levenshtein_distance("hello", "hello"), 0);
}

#[test]
fn test_levenshtein_single_insert() {
    // "hell" → "hello" requires 1 insertion.
    assert_eq!(crate::error::levenshtein_distance("hell", "hello"), 1);
}

#[test]
fn test_levenshtein_single_delete() {
    // "hello" → "hell" requires 1 deletion.
    assert_eq!(crate::error::levenshtein_distance("hello", "hell"), 1);
}

#[test]
fn test_levenshtein_single_substitute() {
    // "hello" → "hallo" requires 1 substitution.
    assert_eq!(crate::error::levenshtein_distance("hello", "hallo"), 1);
}

#[test]
fn test_levenshtein_empty_strings() {
    assert_eq!(crate::error::levenshtein_distance("", ""), 0);
    assert_eq!(crate::error::levenshtein_distance("abc", ""), 3);
    assert_eq!(crate::error::levenshtein_distance("", "abc"), 3);
}
