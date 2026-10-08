//! End-to-end hot-reload and source-hash tests.

use std::fs;

use super::*;
use crate::compat::HashMap;

#[test]
fn reload_same_vars_renders_new_body() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("greeting.tmpl.md");

    // Initial version.
    fs::write(
        &path,
        "---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let original = Template::from_file(&path).unwrap();
    let expected_decls = original.declarations().to_vec();

    let mut ctx = Context::new();
    ctx.set("name", "World");
    assert_eq!(original.render_ctx(&ctx).unwrap(), "Hello World!");

    // Modify ONLY the body on disk (vars unchanged).
    fs::write(
        &path,
        "---
params: [name = str]
---
Goodbye {{ name }}!",
    )
    .unwrap();
    let reloaded = Template::from_file(&path).unwrap();

    // Validation passes — same vars.
    reloaded.validate_declarations(&expected_decls).unwrap();

    // Renders the NEW body.
    assert_eq!(reloaded.render_ctx(&ctx).unwrap(), "Goodbye World!");
}

#[test]
fn reload_mutated_vars_detected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("greeting.tmpl.md");

    // Initial.
    fs::write(
        &path,
        "---
params: [name = str]
---
Hi {{ name }}!",
    )
    .unwrap();
    let original = Template::from_file(&path).unwrap();
    let expected_decls = original.declarations().to_vec();

    // Agent edits the vars — adds `count`.
    fs::write(
        &path,
        "---
params: [name = str, count = int]
---
Hi {{ name }} x{{ count }}!",
    )
    .unwrap();
    let reloaded = Template::from_file(&path).unwrap();

    let err = reloaded.validate_declarations(&expected_decls).unwrap_err();
    assert!(
        matches!(err, TemplateError::DeclarationsMutated { .. }),
        "expected DeclarationsMutated, got: {err}"
    );
}

#[test]
fn reload_removed_var_detected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.tmpl.md");

    // Initial: two vars.
    fs::write(
        &path,
        "---
params: [title = str, body = str]
---
# {{ title }}
{{ body }}",
    )
    .unwrap();
    let original = Template::from_file(&path).unwrap();
    let expected_decls = original.declarations().to_vec();

    // Agent removes `body`.
    fs::write(
        &path,
        "---
params: [title = str]
---
# {{ title }}",
    )
    .unwrap();
    let reloaded = Template::from_file(&path).unwrap();

    let err = reloaded.validate_declarations(&expected_decls).unwrap_err();
    assert!(
        matches!(err, TemplateError::DeclarationsMutated { .. }),
        "expected DeclarationsMutated, got: {err}"
    );
    assert!(
        err.to_string().contains("body"),
        "should mention removed var 'body': {err}"
    );
}

#[test]
fn reload_retyped_var_detected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("score.tmpl.md");

    // Initial: count as int.
    fs::write(
        &path,
        "---
params: [count = int]
---
Count: {{ count }}",
    )
    .unwrap();
    let original = Template::from_file(&path).unwrap();
    let expected_decls = original.declarations().to_vec();

    // Agent changes type from int to str.
    fs::write(
        &path,
        "---
params: [count = str]
---
Count: {{ count }}",
    )
    .unwrap();
    let reloaded = Template::from_file(&path).unwrap();

    let err = reloaded.validate_declarations(&expected_decls).unwrap_err();
    assert!(
        matches!(err, TemplateError::DeclarationsMutated { .. }),
        "expected DeclarationsMutated, got: {err}"
    );
    assert!(
        err.to_string().contains("retyped"),
        "should mention retyped: {err}"
    );
}

#[test]
fn precompiled_from_source_matches_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("example.tmpl.md");
    let source = "---
params: [name = str]
---
Hello {{ name }}!";
    fs::write(&path, source).unwrap();

    let from_file = Template::from_file(&path).unwrap();
    let from_source = Template::from_source(source).unwrap();

    let mut ctx = Context::new();
    ctx.set("name", "test");

    let file_output = from_file.render_ctx(&ctx).unwrap();
    let source_output = from_source.render_ctx(&ctx).unwrap();
    assert_eq!(
        file_output, source_output,
        "from_file and from_source should produce identical output"
    );
}

#[test]
fn precompiled_renders_with_all_features() {
    // Pre-compile a template that uses most features:
    // for loop, idx(), conditionals, filters.
    let tmpl = Template::from_source(
            "---
params: [items = list(name = str, active = bool)]
---
> {% for item in items %}{{ idx(item) }}. {% if item.active %}[✓]{% else %}[ ]{% /if %} {{ item.name | upper }}

> {% /for %}",
        ).unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Struct(Arc::new(HashMap::from([
                ("name".into(), Value::Str("alpha".into())),
                ("active".into(), Value::Bool(true)),
            ]))),
            Value::Struct(Arc::new(HashMap::from([
                ("name".into(), Value::Str("beta".into())),
                ("active".into(), Value::Bool(false)),
            ]))),
        ])),
    );

    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        output,
        "0. [✓] ALPHA
1. [ ] BETA
"
    );
}

#[test]
fn same_source_same_hash() {
    let src = "---
params: [name = str]
---
Hello {{ name }}!";
    let t1 = Template::from_source(src).unwrap();
    let t2 = Template::from_source(src).unwrap();
    assert_eq!(t1.source_hash(), t2.source_hash());
}

#[test]
fn different_source_different_hash() {
    let t1 = Template::from_source(
        "---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let t2 = Template::from_source(
        "---
params: [name = str]
---
Goodbye {{ name }}!",
    )
    .unwrap();
    assert_ne!(t1.source_hash(), t2.source_hash());
}

#[test]
fn hash_changes_with_frontmatter() {
    let src1 = "---
params: [x = str]
---
{{ x }}";
    let src2 = "---
params: [y = str]
---
{{ y }}";
    let t1 = Template::from_source(src1).unwrap();
    let t2 = Template::from_source(src2).unwrap();
    assert_ne!(t1.source_hash(), t2.source_hash());
}
