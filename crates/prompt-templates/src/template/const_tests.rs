use crate::{CompileOptions, Context, Template};

#[test]
fn test_local_constants() {
    let source = r#"---
name: const_test
consts:
  - MAX_RETRY = int := 3
  - APP_NAME = str := "MyApp"
---
{{ APP_NAME }} will retry {{ MAX_RETRY }} times.
"#;
    let tmpl = Template::from_source(source).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        r"MyApp will retry 3 times.
"
    );
}

#[test]
fn test_constant_shadowing() {
    let source = r#"---
consts:
  - VERSION = str := "1.0"
---
Version: {{ VERSION }}
"#;
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("VERSION", "OVERRIDE");

    // Constant MUST win over context.
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        r"Version: 1.0
"
    );
}

#[test]
fn test_complex_constants() {
    let source = r#"---
consts:
  - CONFIG = struct<env = str, debug = bool> := {env = "prod", debug = false}
  - TAGS = list<str> := ["ai", "automation"]
---
Env: {{ CONFIG.env }} (debug={{ CONFIG.debug }})
Tags: {{ TAGS | join(", ") }}
"#;
    let tmpl = Template::from_source(source).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        r"Env: prod (debug=false)
Tags: ai, automation
"
    );
}

#[test]
fn test_imported_constants() {
    // We need a temporary directory to host the templates for import resolution.
    let temp = tempfile::tempdir().unwrap();
    let base_dir = temp.path();

    let library_source = r#"---
name: lib
consts:
  - DEFAULT_TIMEOUT = int := 30
  - COLORS = struct<primary = str> := {primary = "blue"}
---
"#;
    std::fs::write(base_dir.join("lib.tmpl.md"), library_source).unwrap();

    let main_source = r"---
name: main
imports:
  - [lib](lib.tmpl.md)
---
Timeout: {{ lib.DEFAULT_TIMEOUT }}
Primary Color: {{ lib.COLORS.primary }}
";

    let (tmpl, _fm) =
        Template::compile(main_source, CompileOptions::default().base_dir(base_dir)).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        r"Timeout: 30
Primary Color: blue
"
    );
}

#[test]
fn test_constant_with_type_alias() {
    let source = r"---
types:
  - Level = enum<High, Low>
consts:
  - DEFAULT_LEVEL = Level := High
---
Level: {{ kind(DEFAULT_LEVEL) }}
";
    let tmpl = Template::from_source(source).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        r"Level: High
"
    );
}

#[test]
fn test_nested_relative_imports() {
    let temp = tempfile::tempdir().unwrap();
    let base_dir = temp.path();

    // Create a library in the root.
    let lib_source = r"---
name: lib
consts:
  - VAL = int := 100
---
";
    std::fs::write(base_dir.join("lib.tmpl.md"), lib_source).unwrap();

    // Create a subdirectory.
    let sub_dir = base_dir.join("sub");
    std::fs::create_dir(&sub_dir).unwrap();

    // Create a template in the subdirectory that imports the root library.
    let sub_source = r"---
name: sub_template
imports:
  - [lib](../lib.tmpl.md)
---
Sub Value: {{ lib.VAL }}
";
    std::fs::write(sub_dir.join("template.tmpl.md"), sub_source).unwrap();

    // Main template in root that includes the subdirectory template.
    let main_source = r"---
name: main
params: []
---
> {% include [template](sub/template.tmpl.md) %}
";

    let (tmpl, _fm) =
        Template::compile(main_source, CompileOptions::default().base_dir(base_dir)).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        r"Sub Value: 100
"
    );
}

// ---------------------------------------------------------------------------
// Enum literal expressions
// ---------------------------------------------------------------------------

#[test]
fn enum_literal_bare_access_is_error() {
    // Bare {{ Phase.Explore }} should produce a compile error.
    let source = r"---
types:
  - Phase = enum<Explore, Triage>
params: []
---
{{ Phase.Explore }}";
    let result = Template::from_source(source);
    assert!(result.is_err(), "bare enum literal should be rejected");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("bare enum literal") && err.contains("kind("),
        "error should suggest kind(): {err}"
    );
}

#[test]
fn enum_literal_bare_access_all_variants_is_error() {
    let source = r"---
types:
  - Color = enum<Red, Green, Blue>
params: []
---
{{ Color.Red }}";
    assert!(
        Template::from_source(source).is_err(),
        "bare enum literal should be rejected"
    );
}

#[test]
fn enum_literal_kind_unit_variant() {
    let source = r"---
types:
  - Phase = enum<Explore, Triage>
params: []
---
{{ kind(Phase.Explore) }}";
    let tmpl = Template::from_source(source).unwrap();
    let ctx = Context::new();
    assert_eq!(tmpl.render(&ctx).unwrap(), "Explore");
}

#[test]
fn enum_literal_kind_all_variants() {
    let source = r"---
types:
  - Color = enum<Red, Green, Blue>
params: []
---
{{ kind(Color.Red) }}, {{ kind(Color.Green) }}, {{ kind(Color.Blue) }}";
    let tmpl = Template::from_source(source).unwrap();
    let ctx = Context::new();
    assert_eq!(tmpl.render(&ctx).unwrap(), "Red, Green, Blue");
}

#[test]
fn enum_literal_kind_struct_variant() {
    let source = r"---
types:
  - Status = enum<Active, Paused(reason = str)>
params: []
allow_unused: true
---
{{ kind(Status.Active) }} {{ kind(Status.Paused) }}";
    let tmpl = Template::from_source(source).unwrap();
    let ctx = Context::new();
    assert_eq!(tmpl.render(&ctx).unwrap(), "Active Paused");
}

#[test]
fn enum_literal_imported_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("types.tmpl.md"),
        r"---
name: types
types:
  - Severity = enum<Low, Medium, High>
allow_unused: true
---
",
    )
    .unwrap();

    let main_src = r"---
imports:
  - [types](types.tmpl.md)
params: []
---
{{ kind(types.Severity.High) }}";
    let (tmpl, _) = Template::compile(main_src, CompileOptions::default().base_dir(base)).unwrap();

    let ctx = Context::new();
    assert_eq!(tmpl.render(&ctx).unwrap(), "High");
}

#[test]
fn enum_literal_imported_bare_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    std::fs::write(
        base.join("types.tmpl.md"),
        r"---
name: types
types:
  - Severity = enum<Low, Medium, High>
allow_unused: true
---
",
    )
    .unwrap();

    let main_src = r"---
imports:
  - [types](types.tmpl.md)
params: []
---
{{ types.Severity.High }}";
    let result = Template::compile(main_src, CompileOptions::default().base_dir(base));
    assert!(
        result.is_err(),
        "bare imported enum literal should be rejected"
    );
}

#[test]
fn enum_literal_const_name_collision_is_error() {
    // A constant with the same name as a type alias is rejected at compile time.
    let source = r#"---
types:
  - Phase = enum<Explore, Triage>
consts:
  - Phase = str := "overridden"
params: []
---
{{ Phase }}"#;
    let result = Template::from_source(source);
    assert!(
        result.is_err(),
        "constant named same as type alias should be rejected"
    );
}

#[test]
fn enum_literal_in_condition_kind_is_ok() {
    // kind() in conditions should work.
    let source = r"---
types:
  - Phase = enum<Explore, Triage>
params: [p = Phase]
---
> {% if kind(p) == kind(Phase.Explore) %}

found

> {% /if %}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("p", "Explore");
    assert_eq!(
        tmpl.render(&ctx).unwrap(),
        r"found
"
    );
}
