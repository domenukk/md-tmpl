//! Render integration tests: include params, constant precedence, and duck typing.
//!
//! These tests verify that constants, imports, includes, variables, defaults,
//! and template parameters interact correctly — no leaking, no shadowing
//! surprises, and all renders produce expected output.

use std::sync::Arc;

use crate::{CompileOptions, Context, Template, Value};

// ---------------------------------------------------------------------------
// S. Multiple includes with overlapping param names
// ---------------------------------------------------------------------------

/// Two includes with the same param name get different values.
#[test]
fn overlapping_param_names_isolated() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("a.tmpl.md"),
        r"---
name: a
params: [val = str]
---
A={{ val }}",
    )
    .unwrap();

    std::fs::write(
        base.join("b.tmpl.md"),
        r"---
name: b
params: [val = str]
---
B={{ val }}",
    )
    .unwrap();

    let main_src = r#"---
params: []
---
> {% include [a](./a.tmpl.md) with val="first" %}
> {% include [b](./b.tmpl.md) with val="second" %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("A=first"), "got: {output}");
    assert!(output.contains("B=second"), "got: {output}");
    assert!(
        !output.contains("A=second"),
        "val leaked from b to a, got: {output}"
    );
    assert!(
        !output.contains("B=first"),
        "val leaked from a to b, got: {output}"
    );
}

// ---------------------------------------------------------------------------
// T. Constants cannot be overridden by context
// ---------------------------------------------------------------------------

/// Context value cannot override a constant — const always wins.
#[test]
fn constant_wins_over_context() {
    let tmpl = Template::from_source(
        r#"---
consts:
  - FIXED = str := "immutable"

params: []
---
{{ FIXED }}"#,
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("FIXED", "hacked");
    let output = tmpl.render_ctx_allowing_extra(&ctx).unwrap();
    assert_eq!(output, "immutable");
}

// ---------------------------------------------------------------------------
// U. Include with constants + params combined
// ---------------------------------------------------------------------------

/// Included template uses both its own constants and passed params.
#[test]
fn include_uses_own_consts_and_passed_params() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("card.tmpl.md"),
        r#"---
name: card
consts:
  - BORDER = str := "===="

params: [title = str]
---
{{ BORDER }}
{{ title }}
{{ BORDER }}"#,
    )
    .unwrap();

    let main_src = r#"---
params: []
---
> {% include [card](./card.tmpl.md) with title="Hello" %}"#;
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        output,
        r"====
Hello
===="
    );
}

// ---------------------------------------------------------------------------
// V. For-each include with constants in the included template
// ---------------------------------------------------------------------------

/// For-each included template can use its own constants alongside the loop variable.
#[test]
fn for_each_include_with_constants() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("row.tmpl.md"),
        r#"---
name: row
consts:
  - PREFIX = str := ">"

params: [item = str]
---
{{ PREFIX }} {{ item }}"#,
    )
    .unwrap();

    let main_src = r"---
params: [items = list(str)]
---
> {% include [row](./row.tmpl.md) for item in items %}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("alpha".into()),
            Value::Str("beta".into()),
        ])),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("> alpha"), "got: {output}");
    assert!(output.contains("> beta"), "got: {output}");
}

// ---------------------------------------------------------------------------
// W. Structural (duck) typing — extra fields in struct values
// ---------------------------------------------------------------------------

/// Struct param with extra fields renders correctly (extras ignored).
#[test]
fn struct_param_extra_fields_accepted() {
    let tmpl = Template::from_source(
        r"---
params: [user = struct(name = str)]
---
Hello {{ user.name }}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "user",
        Value::new_struct([
            ("name", Value::Str("Alice".into())),
            ("age", Value::Int(30)),
            ("email", Value::Str("alice@test.com".into())),
        ]),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "Hello Alice");
}

/// List items with extra fields render correctly.
#[test]
fn list_items_extra_fields_accepted() {
    let tmpl = Template::from_source(
        r"---
params: [items = list(label = str)]
---
> {% for item in items %}

{{ item.label }}

> {% /for %}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::new_struct([("label", Value::Str("A".into())), ("id", Value::Int(1))]),
            Value::new_struct([
                ("label", Value::Str("B".into())),
                ("id", Value::Int(2)),
                ("extra", Value::Bool(true)),
            ]),
        ])),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains('A'), "got: {output}");
    assert!(output.contains('B'), "got: {output}");
}

/// Type alias resolves to struct — extra fields still accepted.
#[test]
fn type_alias_duck_typing() {
    let tmpl = Template::from_source(
        r"---
types:
  - User = struct(name = str)

params: [user = User]
---
{{ user.name }}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "user",
        Value::new_struct([
            ("name", Value::Str("Bob".into())),
            ("role", Value::Str("admin".into())),
        ]),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "Bob");
}

/// Nested struct with extras at every depth.
#[test]
fn nested_struct_extra_fields_at_every_depth() {
    let tmpl = Template::from_source(
        r"---
params: [config = struct(db = struct(host = str))]
---
{{ config.db.host }}",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "config",
        Value::new_struct([(
            "db",
            Value::new_struct([
                ("host", Value::Str("localhost".into())),
                ("port", Value::Int(5432)),
                ("pool_size", Value::Int(10)),
            ]),
        )]),
    );
    // Also add extras at the top level
    // (but note: top-level extras would be rejected by render_ctx)
    // So we test via render_ctx_allowing_extra or just test the inner struct
    let output = tmpl.render_ctx_allowing_extra(&ctx).unwrap();
    assert_eq!(output, "localhost");
}

// ---------------------------------------------------------------------------
// X. Include with partial struct (duck typing across includes)
// ---------------------------------------------------------------------------

/// Included template declares a subset of the parent's struct fields.
/// The parent passes the full struct; the child only accesses what it declared.
#[test]
fn include_with_partial_struct() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    // Child template only needs `name` from the full user struct.
    std::fs::write(
        base.join("greet_user.tmpl.md"),
        r"---
name: greet_user
params: [user = struct(name = str)]
---
Hello {{ user.name }}!",
    )
    .unwrap();

    // Parent has a richer user struct with name + age + email.
    let main_src = r"---
params: [user = struct(name = str, age = int, email = str)]
---
> {% include [greet_user](./greet_user.tmpl.md) with user=user %}

Age: {{ user.age }}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "user",
        Value::new_struct([
            ("name", Value::Str("Alice".into())),
            ("age", Value::Int(30)),
            ("email", Value::Str("alice@test.com".into())),
        ]),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("Hello Alice!"),
        "include should render with partial struct, got: {output}"
    );
    assert!(
        output.contains("Age: 30"),
        "parent should still access all fields, got: {output}"
    );
}

/// For-each include where child uses subset of list item fields.
#[test]
fn for_each_include_partial_struct_items() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    // Child only cares about the `title` field.
    std::fs::write(
        base.join("show_title.tmpl.md"),
        r"---
name: show_title
params: [item = struct(title = str)]
---
- {{ item.title }}",
    )
    .unwrap();

    // Parent has items with title + description + priority.
    let main_src = r"---
params: [items = list(title = str, description = str, priority = int)]
---
> {% include [show_title](./show_title.tmpl.md) for item in items %}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::new_struct([
                ("title", Value::Str("Fix bug".into())),
                ("description", Value::Str("Crash on startup".into())),
                ("priority", Value::Int(1)),
            ]),
            Value::new_struct([
                ("title", Value::Str("Add tests".into())),
                ("description", Value::Str("Coverage is low".into())),
                ("priority", Value::Int(3)),
            ]),
        ])),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("- Fix bug"),
        "child should render title from partial struct, got: {output}"
    );
    assert!(
        output.contains("- Add tests"),
        "second item should also render, got: {output}"
    );
}

/// Nested include chain: grandchild uses an even smaller subset.
#[test]
fn nested_include_chain_partial_structs() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    // Grandchild only needs name.
    std::fs::write(
        base.join("name_tag.tmpl.md"),
        r"---
name: name_tag
params: [person = struct(name = str)]
---
[{{ person.name }}]",
    )
    .unwrap();

    // Child needs name + role, forwards to grandchild.
    std::fs::write(
        base.join("badge.tmpl.md"),
        r"---
name: badge
params: [person = struct(name = str, role = str)]
---
> {% include [name_tag](./name_tag.tmpl.md) with person=person %}

({{ person.role }})",
    )
    .unwrap();

    // Parent has full person with name + role + id.
    let main_src = r"---
params: [person = struct(name = str, role = str, id = int)]
---
> {% include [badge](./badge.tmpl.md) with person=person %}

ID: {{ person.id }}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let mut ctx = Context::new();
    ctx.set(
        "person",
        Value::new_struct([
            ("name", Value::Str("Alice".into())),
            ("role", Value::Str("Engineer".into())),
            ("id", Value::Int(42)),
        ]),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("[Alice]"),
        "grandchild should render name, got: {output}"
    );
    assert!(
        output.contains("(Engineer)"),
        "child should render role, got: {output}"
    );
    assert!(
        output.contains("ID: 42"),
        "parent should access id, got: {output}"
    );
}
