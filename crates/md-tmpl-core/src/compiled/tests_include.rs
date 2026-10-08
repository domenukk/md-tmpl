//! Include depth, contract, recursion, and markdown-link include tests.

use std::sync::Arc;

use super::*;
use crate::{compat::HashMap, context::Context, value::Value};

// -- include depth limit --

#[test]
fn include_depth_limit_enforced() {
    let dir = tempfile::tempdir().unwrap();
    // Create a self-including template to trigger depth limit.
    std::fs::write(
        dir.path().join("self.tmpl.md"),
        "\
---

name: self
params: []
---
X{% include [self](./self.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("self.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("self-including template should exceed depth limit");
    let err = err.to_string();
    assert!(
        err.contains("include depth"),
        "error should mention depth limit: {err}"
    );
}

#[test]
fn include_depth_custom_limit_enforced() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

name: child
params: []
---
child",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: []
---
parent+{% include [child](./child.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("grandparent.tmpl.md"),
        "\
---

name: grandparent
params: []
---
grandparent+{% include [parent](./parent.tmpl.md) %}",
    )
    .unwrap();

    let mut tmpl = crate::Template::from_file(&dir.path().join("grandparent.tmpl.md")).unwrap();
    let ctx = Context::new();

    // 1. With default max include depth (16), it should render fine.
    let res = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(res, "grandparent+parent+child");

    // 2. Set max include depth to 1. Since grandparent -> parent -> child requires 2 includes, depth is 2, so it should error.
    tmpl.set_max_include_depth(1);
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("include depth 1 should be exceeded by 2-level chain");
    let err = err.to_string();
    assert!(
        err.contains("include depth (1) exceeded"),
        "error should mention depth limit: {err}"
    );

    // 3. Set max include depth to 2, it should render fine again.
    let tmpl = tmpl.with_max_include_depth(2);
    let res2 = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(res2, "grandparent+parent+child");
}

#[test]
fn deeply_nested_includes_three_levels() {
    // Grandparent → Parent → Child: 3 levels of distinct includes.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

name: child
params: [leaf = str]
---
Leaf:{{ leaf }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: [middle = str]
---
\
         Mid:{{ middle }},{% include [child](./child.tmpl.md) with leaf=\"deep\" %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("grandparent.tmpl.md"),
        "\
---

name: grandparent
params: [top = str]
---
\
         Top:{{ top }},{% include [parent](./parent.tmpl.md) with middle=\"mid\" %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("grandparent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("top", "root");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Top:root,Mid:mid,Leaf:deep");
}

#[test]
fn deeply_nested_includes_four_levels() {
    // A → B → C → D: 4 levels of distinct includes.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("d.tmpl.md"),
        "\
---

name: d
params: []
---
D",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("c.tmpl.md"),
        "\
---

name: c
params: []
---
C+{% include [d](./d.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "\
---

name: b
params: []
---
B+{% include [c](./c.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "\
---

name: a
params: []
---
A+{% include [b](./b.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "A+B+C+D");
}

// -- include contract enforcement --

#[test]
fn include_missing_contract_params_errors() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

name: child
params: [msg = str]
---
{{ msg }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: []
---
> {% include [child](./child.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("include missing required param 'msg' should fail");
    let err = err.to_string();
    assert!(
        err.contains("msg"),
        "error should mention missing 'msg': {err}"
    );
}

#[test]
fn include_with_contract_satisfied_renders() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

name: child
params: [msg = str]
---
Hello {{ msg }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: []
---
> {% include [child](./child.tmpl.md) with msg=\"World\" %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Hello World");
}

#[test]
fn include_no_params_always_ok() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("static.tmpl.md"),
        "\
---

name: static
params: []
---
Static!",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: []
---
> {% include [static](./static.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Static!");
}

// ---------------------------------------------------------------------------
// Recursive include tests (runtime depth limit)
// ---------------------------------------------------------------------------

#[test]
fn self_recursive_include_hits_depth_limit() {
    // A template that unconditionally includes itself — should hit the
    // runtime depth limit, not hang.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("self.tmpl.md"),
        "\
---

name: self
params: []
---
R> {% include [self](./self.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("self.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("self-recursion should hit depth limit");
    assert!(
        err.to_string().contains("include depth"),
        "should mention depth limit: {err}"
    );
}

#[test]
fn mutual_recursive_includes_hit_depth_limit() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.tmpl.md"),
        "\
---

name: a
params: []
---
A> {% include [b](./b.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "\
---

name: b
params: []
---
B> {% include [a](./a.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let ctx = Context::new();
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("mutual recursion should hit depth limit");
    assert!(
        err.to_string().contains("include depth"),
        "should mention depth limit: {err}"
    );
}

#[test]
fn include_type_mismatch_caught_at_runtime() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        "\
---

name: child
params: [count = int]
---
{{ count }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        "\
---

name: parent
params: [name = str]
---
\
         > {% include [child](./child.tmpl.md) with count=name %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "not_a_number");
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("type mismatch should error at runtime");
    assert!(
        err.to_string().contains("type mismatch") || err.to_string().contains("expected"),
        "should mention type error: {err}"
    );
}

// -- markdown link include E2E pipeline tests -----------------------------

#[test]
fn markdown_link_include_simple() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("greeting.tmpl.md"),
        r"---
name: greeting
params: []
---
Hello!",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("main.tmpl.md"),
        r"---
name: main
params: []
---
> {% include [greeting](./greeting.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("main.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Hello!");
}

#[test]
fn markdown_link_include_with_vars() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        r"---
name: child
params: [msg = str]
---
Got: {{ msg }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("parent.tmpl.md"),
        r#"---
name: parent
params: []
---
> {% include [child](./child.tmpl.md) with msg="hi" %}"#,
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("parent.tmpl.md")).unwrap();
    let ctx = Context::new();
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "Got: hi");
}

#[test]
fn markdown_link_include_for_each() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("row.tmpl.md"),
        r"---
name: row
params: [item = str]
---
- {{ item }}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("list.tmpl.md"),
        r"---
name: list
params: [items = list(str)]
---
> {% include [row](./row.tmpl.md) for item in items %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("list.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(Arc::new(vec![
            Value::Str("one".into()),
            Value::Str("two".into()),
        ])),
    );
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "- one- two");
}

// -- self-recursive include tests -----------------------------------------

#[test]
fn self_recursive_include_renders_tree() {
    let dir = tempfile::tempdir().unwrap();
    // A template that includes itself for each child.
    std::fs::write(
        dir.path().join("node.tmpl.md"),
        r"---
name: node
params:
  - label = str
  - children = list(label = str, children = list(label = str, children = list(str)))
---
{{ label }}

> {% for child in children %}> {% include [node](./node.tmpl.md) with label=child.label, children=child.children %}> {% /for %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("node.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("label", "root");
    ctx.set(
        "children",
        Value::List(Arc::new(vec![
            Value::Struct(Arc::new(HashMap::from([
                ("label".into(), Value::Str("child_a".into())),
                ("children".into(), Value::List(Arc::new(vec![]))),
            ]))),
            Value::Struct(Arc::new(HashMap::from([
                ("label".into(), Value::Str("child_b".into())),
                (
                    "children".into(),
                    Value::List(Arc::new(vec![Value::Struct(Arc::new(HashMap::from([
                        ("label".into(), Value::Str("grandchild".into())),
                        ("children".into(), Value::List(Arc::new(vec![]))),
                    ])))])),
                ),
            ]))),
        ])),
    );
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"root

> child_a

> > child_b

> grandchild

> > "
    );
}

#[test]
fn self_recursive_include_terminates_on_empty_children() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("leaf.tmpl.md"),
        r"---
name: leaf
params:
  - label = str
  - children = list(label = str, children = list(label = str, children = list(str)))
---
{{ label }}

> {% for child in children %}> {% include [leaf](./leaf.tmpl.md) with label=child.label, children=child.children %}> {% /for %}",
    )
    .unwrap();

    let tmpl = crate::Template::from_file(&dir.path().join("leaf.tmpl.md")).unwrap();
    let mut ctx = Context::new();
    ctx.set("label", "solo");
    ctx.set("children", Value::List(Arc::new(vec![])));
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result.trim(), "solo");
}
