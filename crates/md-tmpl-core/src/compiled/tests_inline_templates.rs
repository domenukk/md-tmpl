//! Inline template include and scoping tests.

use super::*;
use crate::context::Context;

// ---------------------------------------------------------------------------
// Inline template include tests (runtime)
// ---------------------------------------------------------------------------

#[test]
fn inline_template_include_renders_test() {
    // Uses `> {% tmpl %}` blockquote prefix as required by the parser.
    let src = r#"---

params: []
---
> {% tmpl greeting %}
---
params: [who = str]
---
Hello {{ who }}!

> {% /tmpl %}

> {% include greeting with who="World" %}"#;
    let tmpl = crate::Template::from_source(src).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(
        result,
        "Hello World!
"
    );
}

#[test]
fn inline_template_missing_params_errors() {
    // Inline template with required param not provided at include site.
    let src = r"---

params: []
---
> {% tmpl greeting %}
---
params: [who = str]
---
Hello {{ who }}!

> {% /tmpl %}

> {% include greeting %}";
    let err =
        crate::Template::from_source(src).expect_err("missing include params are a compile error");
    let msg = err.to_string();
    assert!(
        msg.contains("who"),
        "should mention missing 'who' param: {msg}"
    );
}

// -- Inline template scoping tests (runtime) ---------------------------

#[test]
fn same_named_tmpl_in_different_files_render_independently() {
    // Parent defines {% tmpl helper %} rendering "PARENT".
    // Included file defines its own {% tmpl helper %} rendering "CHILD".
    // Each should resolve to its own definition.
    let dir = tempfile::tempdir().unwrap();

    // included_file.tmpl.md: defines its own "helper" and uses it
    std::fs::write(
        dir.path().join("included_file.tmpl.md"),
        r"---

params: []
---
> {% tmpl helper %}
---
params: []
---
CHILD

> {% /tmpl %}

> {% include helper %}",
    )
    .unwrap();

    // Parent template: defines its own "helper" and includes the file
    let parent_src = r"---

params: []
---
> {% tmpl helper %}
---
params: []
---
PARENT

> {% /tmpl %}

> {% include helper %}

---
";

    let tmpl = crate::Template::from_source(parent_src).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    // Parent's "helper" renders "PARENT"
    assert_eq!(
        result,
        r"PARENT
---
"
    );
}

#[test]
fn included_file_uses_own_inline_templates() {
    // child.tmpl.md defines {% tmpl row %} and uses it internally.
    // Parent doesn't define "row" — it must resolve to the child's own tmpl.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

params:
  - name = str
---
\
         > {% tmpl row %}

\
         ---
\
         params:
  - label = str
\
         ---
\
         - {{ label }}

\
         > {% /tmpl %}

\
         > {% include row with label=name %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params:
  - name = str
---
\
         > {% include [child](./child.tmpl.md) with name=name %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        "- Alice
"
    );
}

#[test]
fn parent_tmpl_does_not_leak_to_included_file() {
    // Parent defines {% tmpl secret %}. Included file tries {% include secret %}.
    // This should fail because parent's tmpl defs don't leak to included files.
    let dir = tempfile::tempdir().unwrap();

    // child tries to use "secret" which only parent defines
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

params: []
---
\
         > {% include secret %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl secret %}

\
         ---
\
         params: []
\
         ---
\
         SECRET

\
         > {% /tmpl %}

\
         > {% include [child](./child.tmpl.md) %}",
    )
    .unwrap();

    // child.tmpl.md tries to include "secret" which doesn't exist
    // (neither as a file nor as an inline template in child's scope).
    // This should be a render error at runtime (file not found for "secret").
    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let err = tmpl
        .render_ctx(&Context::new())
        .expect_err("child should not see parent's inline template 'secret'");
    let msg = err.to_string();
    assert!(
        msg.contains("secret"),
        "error should mention 'secret': {msg}",
    );
}

#[test]
fn same_named_tmpl_in_parent_and_child_are_independent() {
    // Both parent and child define {% tmpl greeting %} with different content.
    // Each should resolve to its own definition.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl greeting %}

\
         ---
\
         params: []
\
         ---
\
         CHILD_GREETING

\
         > {% /tmpl %}

\
         > {% include greeting %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl greeting %}

\
         ---
\
         params: []
\
         ---
\
         PARENT_GREETING

\
         > {% /tmpl %}

\
         > {% include greeting %}

\
         > {% include [child](./child.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(
        result,
        "PARENT_GREETING
CHILD_GREETING
"
    );
}

#[test]
fn two_included_files_same_tmpl_name_different_content() {
    // Two different files both define {% tmpl row %} with different content.
    // Parent includes both — each should use its own "row" definition.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("alpha.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl row %}

\
         ---
\
         params: []
\
         ---
\
         ALPHA_ROW

\
         > {% /tmpl %}

\
         > {% include row %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("beta.tmpl.md"),
        "\
---

params: []
---
\
         > {% tmpl row %}

\
         ---
\
         params: []
\
         ---
\
         BETA_ROW

\
         > {% /tmpl %}

\
         > {% include row %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params: []
---
\
         > {% include [alpha](./alpha.tmpl.md) %}

\
         > {% include [beta](./beta.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(
        result,
        "ALPHA_ROW
BETA_ROW
"
    );
}

#[test]
fn same_display_name_different_files_work() {
    // {% include [greeting](./en/greeting.tmpl.md) %} and
    // {% include [greeting](./de/greeting.tmpl.md) %} should work independently.
    let dir = tempfile::tempdir().unwrap();

    std::fs::create_dir_all(dir.path().join("en")).unwrap();
    std::fs::create_dir_all(dir.path().join("de")).unwrap();

    std::fs::write(
        dir.path().join("en/greeting.tmpl.md"),
        "\
---

params:
  - name = str
---
Hello {{ name }}!",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("de/greeting.tmpl.md"),
        "\
---

params:
  - name = str
---
Hallo {{ name }}!",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

params:
  - name = str
---
\
         > {% include [greeting](./en/greeting.tmpl.md) with name=name %}

\
         > {% include [greeting](./de/greeting.tmpl.md) with name=name %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "World");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Hello World!Hallo World!");
}

#[test]
fn nested_include_chain_a_b_c() {
    // A includes B, B includes C — tests multi-level include works.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("c.tmpl.md"),
        "\
---

params:
  - msg = str
---
[C:{{ msg }}]",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "\
---

params:
  - msg = str
---
\
         [B:{{ msg }}]

\
         > {% include [c](./c.tmpl.md) with msg=msg %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "\
---

params:
  - msg = str
---
\
         [A:{{ msg }}]

\
         > {% include [b](./b.tmpl.md) with msg=msg %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("msg", "hello");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        "[A:hello]
[B:hello]
[C:hello]"
    );
}

#[test]
fn diamond_include_deduplicates_correctly() {
    // A includes B and C, both include D.
    // D should render twice (once per include site) but not cause errors.
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("d.tmpl.md"),
        "\
---

params:
  - label = str
---
[D:{{ label }}]",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "\
---

params:
  - val = str
---
\
         [B]

\
         > {% include [d](./d.tmpl.md) with label=val %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("c.tmpl.md"),
        "\
---

params:
  - val = str
---
\
         [C]

\
         > {% include [d](./d.tmpl.md) with label=val %}",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "\
---

params:
  - x = str
  - y = str
---
\
         > {% include [b](./b.tmpl.md) with val=x %}

\
         > {% include [c](./c.tmpl.md) with val=y %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "from_b");
    ctx.set("y", "from_c");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        "[B]
[D:from_b][C]
[D:from_c]"
    );
}
