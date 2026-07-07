//! Verify that proc macros are accessible through `md_tmpl::` re-export.
//!
//! The `md-tmpl` facade re-exports `include_template!` and `template!` from
//! `md-tmpl-macros`, so users only need `md-tmpl` as a dependency.

// ---------------------------------------------------------------------------
// 1. `md_tmpl::include_template!` — file-based macro re-export
// ---------------------------------------------------------------------------

md_tmpl::include_template!("prompts/greeting.tmpl.md");

#[test]
fn include_template_via_facade() {
    let output = greeting::Params::builder()
        .name("Facade")
        .count(1_i64)
        .items(Vec::new())
        .build()
        .render()
        .unwrap();
    assert!(
        output.contains("Facade"),
        "include_template! via md_tmpl:: should work, got: {output}"
    );
}

// ---------------------------------------------------------------------------
// 2. `md_tmpl::template!` — inline macro re-export
// ---------------------------------------------------------------------------

md_tmpl::template!(
    r"---
params:
  - who = str
---
Hello {{ who }}!"
    => inline_greet
);

#[test]
fn template_macro_via_facade() {
    let output = inline_greet::Params {
        who: "World".into(),
    }
    .render()
    .unwrap();
    assert_eq!(output, "Hello World!");
}

// ---------------------------------------------------------------------------
// 3. Core types are also accessible via the facade
// ---------------------------------------------------------------------------

#[test]
fn core_types_via_facade() {
    // Value, Context, Template, CompileOptions — all from md_tmpl_core.
    let val = md_tmpl::Value::Str("test".into());
    assert_eq!(val.as_str(), Some("test"));

    let ctx = md_tmpl::ctx! { x: "hello" };
    let tmpl = md_tmpl::Template::from_source("---\nparams: [x = str]\n---\n{{ x }}").unwrap();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "hello");
}

// ---------------------------------------------------------------------------
// 4. serde round-trip via facade
// ---------------------------------------------------------------------------

#[test]
fn serde_via_facade() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Agent {
        name: String,
        score: i64,
    }

    let agent = Agent {
        name: "Alice".into(),
        score: 95,
    };
    let val = md_tmpl::to_value(&agent).unwrap();
    let back: Agent = md_tmpl::from_value(&val).unwrap();
    assert_eq!(agent, back);
}
