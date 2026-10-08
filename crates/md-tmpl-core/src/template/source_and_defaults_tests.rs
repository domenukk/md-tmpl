//! Template construction and default-value tests.

use super::*;
use crate::value::Value;

#[test]
fn from_source_and_render() {
    let tmpl = Template::from_source(
        "---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Hello world!");
}

#[test]
fn from_source_with_frontmatter_extracts_metadata() {
    let (tmpl, fm) = Template::compile(
        "---
name: demo
params: [a = str]
---
{{ a }}",
        CompileOptions::default(),
    )
    .unwrap();
    assert_eq!(fm.name, Some("demo".to_string()));
    assert_eq!(fm.declarations[0].name, "a");
    let mut ctx = Context::new();
    ctx.set("a", "val");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "val");
}

// -- defaults --

#[test]
fn defaults_returns_default_values() {
    let tmpl = Template::from_source(
        r#"---
params:
  - name = str
  - count = int := 42
  - label = str := "hello"
---
{{ name }} {{ count }} {{ label }}"#,
    )
    .unwrap();
    let defaults = tmpl.defaults();
    assert_eq!(defaults.len(), 2);
    assert_eq!(defaults.get("count"), Some(&Value::Int(42)));
    assert_eq!(defaults.get("label"), Some(&Value::Str("hello".into())));
    assert!(!defaults.contains_key("name"), "name has no default");
}

#[test]
fn defaults_empty_when_no_defaults() {
    let tmpl = Template::from_source(
        "---
params: [name = str, age = int]
---
{{ name }} {{ age }}",
    )
    .unwrap();
    assert!(tmpl.defaults().is_empty());
}

#[test]
fn default_single_param() {
    let tmpl = Template::from_source(
        "---
params:
  - name = str
  - count = int := 7
---
{{ name }} {{ count }}",
    )
    .unwrap();
    assert_eq!(tmpl.default("count"), Some(&Value::Int(7)));
    assert_eq!(tmpl.default("name"), None);
    assert_eq!(tmpl.default("nonexistent"), None);
}

#[test]
fn defaults_context_prefills_and_renders() {
    let tmpl = Template::from_source(
        "---
params:
  - name = str
  - count = int := 5
---
{{ name }} ({{ count }})",
    )
    .unwrap();
    let mut ctx = tmpl.defaults_context();
    ctx.set("name", "Alice");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Alice (5)");
}

#[test]
fn defaults_context_kinds_default_prefills_and_renders() {
    let tmpl = Template::from_source(
        r"---
types:
  - ForumTag = enum(FINDING, HYPOTHESIS)

params:
  - tags = list(str) := kinds(ForumTag)
---
> {% for tag in tags %}{{ tag }} {% /for %}",
    )
    .unwrap();
    let ctx = tmpl.defaults_context();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "FINDING HYPOTHESIS ");
}

#[test]
fn defaults_context_overridable() {
    let tmpl = Template::from_source(
        "---
params:
  - name = str
  - count = int := 5
---
{{ name }} ({{ count }})",
    )
    .unwrap();
    let mut ctx = tmpl.defaults_context();
    ctx.set("name", "Bob");
    ctx.set("count", 99_i64);
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Bob (99)");
}

#[test]
fn declarations_expose_defaults() {
    let tmpl = Template::from_source(
        "---
params:
  - name = str
  - count = int := 10
---
{{ name }} {{ count }}",
    )
    .unwrap();
    let decls = tmpl.declarations();
    assert_eq!(decls.len(), 2);

    assert_eq!(decls[0].name, "name");
    assert_eq!(decls[0].default_value(), None);

    assert_eq!(decls[1].name, "count");
    assert_eq!(decls[1].default_value(), Some(&Value::Int(10)));
}

#[test]
fn validate_missing_params() {
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} {{ count }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    // count is missing
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    assert!(matches!(err, TemplateError::MissingParams(_)));
}

#[test]
fn validate_type_mismatch() {
    let tmpl = Template::from_source(
        "---
params: [flag = bool]
---
{{ flag }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("flag", "not a bool"); // str, not bool
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    assert!(matches!(err, TemplateError::TypeMismatch { .. }));
}

#[test]
fn from_file_and_render() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tmpl.md");
    std::fs::write(
        &path,
        "---
name: test
params: [x = str]
---
Content {{ x }}",
    )
    .unwrap();
    let tmpl = Template::from_file(&path).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "here");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Content here");
}

#[test]
fn load_template_from_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("greeting.tmpl.md"),
        "---
name: greeting
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let tmpl = load_template(dir.path(), "greeting").unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Hello world!");
}

#[test]
fn load_template_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let err = load_template(dir.path(), "nonexistent")
        .expect_err("loading nonexistent template should fail");
    assert!(
        err.to_string().contains("No such file") || err.to_string().contains("not found"),
        "should mention file not found: {err}"
    );
}

#[test]
fn from_file_with_frontmatter_extracts_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fm.tmpl.md");
    std::fs::write(
        &path,
        "---
name: fm
description: Desc
params: [x = str, y = str]
---
{{ x }} {{ y }}",
    )
    .unwrap();
    let (_tmpl, fm) = Template::compile_file(&path, CompileOptions::default()).unwrap();
    assert_eq!(fm.name, Some("fm".to_string()));
    assert_eq!(fm.description, Some("Desc".to_string()));
    assert_eq!(fm.params, vec!["x", "y"]);
}

#[test]
fn from_source_with_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    let (tmpl, _fm) = Template::compile(
        "---
params: [name = str]
---
Hello {{ name }}!",
        CompileOptions::default().base_dir(dir.path()),
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "test");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Hello test!");
}

#[test]
fn render_no_frontmatter_errors() {
    let err =
        Template::from_source("Just text").expect_err("template without frontmatter should fail");
    assert!(
        err.to_string().contains("frontmatter"),
        "should mention missing frontmatter: {err}"
    );
}

#[test]
fn render_empty_variables_passes_validation() {
    let tmpl = Template::from_source(
        "---
params: []
---
Ok",
    )
    .unwrap();
    let ctx = Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Ok");
}

#[test]
fn render_bool_value() {
    let tmpl = Template::from_source(
        "---
params: [flag = bool]
---
> {% if flag %}yes{% /if %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("flag", Value::Bool(true));
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "yes");
}
