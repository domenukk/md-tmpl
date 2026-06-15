//! Cross-crate integration tests exercising compile-time macros with complex templates.
//!
//! These tests verify that `prompt-templates` (runtime) and
//! `prompt-templates-macros` (compile-time codegen) work together correctly
//! across crate boundaries with enums, lists, and typed parameters.

use std::str::FromStr;

use prompt_templates::{Template, Value, ctx};
use prompt_templates_macros::{include_template, include_types};

// ── Generate typed modules from templates ────────────────────────────────

include_types!("prompts/cross_crate_complex.tmpl.md");
include_types!("prompts/bug_report.tmpl.md");

// ── Test 1: include_template with complex types ─────────────────────────

#[test]
fn test_include_template_with_complex_types() {
    let tmpl = include_template!("prompts/cross_crate_complex.tmpl.md");

    let ctx = ctx! {
        username: "alice",
        role: "Admin",
        score: 95.5_f64,
        active: true,
        tags: [
            { label: "rust" },
            { label: "testing" }
        ]
    };

    let output = tmpl.render(&ctx).unwrap();

    assert_eq!(
        output,
        "\nUser: alice\nRole: Admin\nScore: 95.5\nActive: true\n\nTags:\n\n\n- rust\n  > \n- testing\n  > "
    );
}

// ── Test 2: include_types generates correct struct fields ───────────────

#[test]
fn test_include_types_generates_correct_structs() {
    // Construct the Params struct with all field types to verify codegen.
    let params = cross_crate_complex::Params {
        username: "bob".into(),
        role: cross_crate_complex::ParamsRole::Admin,
        score: 42.0,
        active: false,
        tags: vec![cross_crate_complex::ParamsTagsItem {
            label: "integration".into(),
        }],
    };

    // Verify field values are correctly stored.
    assert_eq!(params.username, "bob");
    assert!((params.score - 42.0).abs() < f64::EPSILON);
    assert!(!params.active);
    assert_eq!(params.tags.len(), 1);
    assert_eq!(params.tags[0].label, "integration");
}

// ── Test 3: include_types enum variant generation ───────────────────────

#[test]
fn test_include_types_enum_variants() {
    // The `bug_report` template defines `Severity = enum<Critical, High, Medium, Low>`.
    // Verify all variants exist and can be pattern-matched.
    let severity = bug_report::Severity::Critical;

    let label = match severity {
        bug_report::Severity::Critical => "critical",
        bug_report::Severity::High => "high",
        bug_report::Severity::Medium => "medium",
        bug_report::Severity::Low => "low",
    };

    assert_eq!(label, "critical");

    // Verify Display trait works for the top-level type alias enum.
    assert_eq!(bug_report::Severity::High.to_string(), "High");
    assert_eq!(bug_report::Severity::Low.to_string(), "Low");

    // Verify FromStr roundtrip.
    let parsed = bug_report::Severity::from_str("medium").unwrap();
    assert_eq!(parsed, bug_report::Severity::Medium);

    // Verify VARIANT_NAMES constant.
    assert_eq!(
        bug_report::Severity::VARIANT_NAMES,
        ["Critical", "High", "Medium", "Low"]
    );

    // Verify ALL constant.
    assert_eq!(bug_report::Severity::ALL.len(), 4);
}

// ── Test 4: validate_template catches parameter drift ───────────────────

#[test]
fn test_validate_template_catches_drift() {
    // Construct a "drifted" template at runtime that has different params than
    // what the generated struct expects.
    let drifted_source = "\
---
name: cross_crate_complex
description: Drifted version with different params
params:
  - totally_new_param = str
---

{{ totally_new_param }}
";

    let drifted_tmpl = Template::from_source(drifted_source).unwrap();

    // The generated Params::validate_template should reject the drifted template
    // because the parameter sets differ.
    let result = cross_crate_complex::Params::validate_template(&drifted_tmpl);
    assert!(
        result.is_err(),
        "validate_template should catch param drift, but got Ok"
    );
}

// ── Test 5: full roundtrip with generated Params struct ─────────────────

#[test]
fn test_render_with_generated_params() {
    let tmpl = include_template!("prompts/cross_crate_complex.tmpl.md");

    let params = cross_crate_complex::Params {
        username: "eve".into(),
        role: cross_crate_complex::ParamsRole::Viewer,
        score: 77.3,
        active: true,
        tags: vec![
            cross_crate_complex::ParamsTagsItem {
                label: "security".into(),
            },
            cross_crate_complex::ParamsTagsItem {
                label: "audit".into(),
            },
        ],
    };

    let output = params.render(tmpl).unwrap();

    assert_eq!(
        output,
        "\nUser: eve\nRole: Viewer\nScore: 77.3\nActive: true\n\nTags:\n\n\n- security\n  > \n- audit\n  > "
    );
}

// ── Test 6: validate_template succeeds for matching template ────────────

#[test]
fn test_validate_template_succeeds_for_matching() {
    let tmpl = include_template!("prompts/cross_crate_complex.tmpl.md");
    cross_crate_complex::Params::validate_template(tmpl).unwrap();
}

// ── Test 7: bug_report full roundtrip with typed enum params ────────────

#[test]
fn test_bug_report_roundtrip_with_typed_enums() {
    let tmpl = include_template!("prompts/bug_report.tmpl.md");

    let params = bug_report::Params {
        title: "Null pointer dereference".into(),
        severity: bug_report::ParamsSeverity::Critical,
        bugs: vec![
            bug_report::ParamsBugsItem {
                name: "crash in parser".into(),
                priority: bug_report::ParamsBugsItemPriority::High,
            },
            bug_report::ParamsBugsItem {
                name: "memory leak".into(),
                priority: bug_report::ParamsBugsItemPriority::Medium,
            },
        ],
    };

    let output = params.render(tmpl).unwrap();

    assert_eq!(
        output,
        "\n# Bug Report: Null pointer dereference\n\nSeverity: Critical\n\n\n- crash in parser (High)\n  > \n- memory leak (Medium)\n  > "
    );
}

// ── Test 8: ctx! macro interop with include_template ────────────────────

#[test]
fn test_ctx_macro_with_include_template() {
    let tmpl = include_template!("prompts/bug_report.tmpl.md");

    // Use the ctx! macro (from prompt_templates) with macros (from prompt_templates_macros).
    let ctx = ctx! {
        title: "XSS vulnerability",
        severity: "High",
        bugs: [
            { name: "reflected XSS", priority: "Critical" },
            { name: "stored XSS", priority: "High" }
        ]
    };

    let output = tmpl.render(&ctx).unwrap();

    assert_eq!(
        output,
        "\n# Bug Report: XSS vulnerability\n\nSeverity: High\n\n\n- reflected XSS (Critical)\n  > \n- stored XSS (High)\n  > "
    );
}

// ── Test 9: to_context produces renderable context ──────────────────────

#[test]
fn test_to_context_interop() {
    let params = cross_crate_complex::Params {
        username: "frank".into(),
        role: cross_crate_complex::ParamsRole::Editor,
        score: 88.0,
        active: false,
        tags: vec![],
    };

    let ctx = params.to_context();

    // Verify the context contains expected keys.
    assert!(ctx.get("username").is_some());
    assert!(ctx.get("role").is_some());
    assert!(ctx.get("score").is_some());
    assert!(ctx.get("active").is_some());

    // Render with the context to verify it's valid.
    let tmpl = include_template!("prompts/cross_crate_complex.tmpl.md");
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        "\nUser: frank\nRole: Editor\nScore: 88\nActive: false\n\nTags:\n\n"
    );
}

// ── Test 10: Value enum interop with template rendering ─────────────────

#[test]
fn test_value_dict_rendering() {
    let tmpl = include_template!("prompts/cross_crate_complex.tmpl.md");

    // Build context entirely from Value constructors.
    let mut ctx = prompt_templates::Context::new();
    ctx.set("username", Value::Str("grace".into()));
    ctx.set("role", Value::Str("Admin".into()));
    ctx.set("score", Value::Float(100.0));
    ctx.set("active", Value::Bool(true));
    ctx.set(
        "tags",
        Value::List(vec![Value::dict([("label", Value::Str("perf".into()))])]),
    );

    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(
        output,
        "\nUser: grace\nRole: Admin\nScore: 100\nActive: true\n\nTags:\n\n\n- perf\n  > "
    );
}
