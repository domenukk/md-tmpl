use crate::{Context, Template};

#[test]
fn test_local_constants() {
    let source = r#"---
name: const_test
consts:
  - MAX_RETRY = int := 3
  - APP_NAME = str := "Artist"
---
{{ APP_NAME }} will retry {{ MAX_RETRY }} times.
"#;
    let tmpl = Template::from_source(source).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Artist will retry 3 times.\n");
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
    assert_eq!(output, "Version: 1.0\n");
}

#[test]
fn test_complex_constants() {
    let source = r#"---
consts:
  - CONFIG = dict<env = str, debug = bool> := { env: "prod", debug: false }
  - TAGS = list<str> := ["ai", "security"]
---
Env: {{ CONFIG.env }} (debug={{ CONFIG.debug }})
Tags: {{ TAGS | join(", ") }}
"#;
    let tmpl = Template::from_source(source).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Env: prod (debug=false)\nTags: ai, security\n");
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
  - COLORS = dict<primary = str> := { primary: "blue" }
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

    let tmpl = Template::from_source_with_base_dir(main_source, base_dir).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Timeout: 30\nPrimary Color: blue\n");
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
    assert_eq!(output, "Level: High\n");
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

    let tmpl = Template::from_source_with_base_dir(main_source, base_dir).unwrap();
    let ctx = Context::new();
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Sub Value: 100\n");
}
