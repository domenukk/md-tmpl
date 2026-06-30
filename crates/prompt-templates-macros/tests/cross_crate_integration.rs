//! Cross-crate integration tests exercising compile-time macros with complex templates.
//!
//! These tests verify that `prompt-templates` (runtime) and
//! `prompt-templates-macros` (compile-time codegen) work together correctly
//! across crate boundaries with enums, lists, and typed parameters.

use std::str::FromStr;

use prompt_templates::{Template, Value, ctx};
use prompt_templates_macros::include_template;

// ── Generate typed modules from templates ────────────────────────────────

include_template!("prompts/cross_crate_complex.tmpl.md");
include_template!("prompts/task_report.tmpl.md");

// ── Test 1: include_template with complex types ─────────────────────────

#[test]
fn test_include_template_with_complex_types() {
    let tmpl = cross_crate_complex::template();

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

    let output = tmpl.render_ctx(&ctx).unwrap();

    assert_eq!(
        output,
        "\nUser: alice\nRole: Admin\nScore: 95.5\nActive: true\n\nTags:\n- rust\n- testing\n"
    );
}

// ── Test 2: include_template generates correct struct fields ───────────────

#[test]
fn test_include_template_generates_correct_structs() {
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

// ── Test 3: include_template enum variant generation ───────────────────────

#[test]
fn test_include_template_enum_variants() {
    // The `task_report` template defines `Priority = enum(Critical, High, Medium, Low)`.
    // Verify all variants exist and can be pattern-matched.
    let severity = task_report::Priority::Critical;

    let label = match severity {
        task_report::Priority::Critical => "critical",
        task_report::Priority::High => "high",
        task_report::Priority::Medium => "medium",
        task_report::Priority::Low => "low",
    };

    assert_eq!(label, "critical");

    // Verify Display trait works for the top-level type alias enum.
    assert_eq!(task_report::Priority::High.to_string(), "High");
    assert_eq!(task_report::Priority::Low.to_string(), "Low");

    // Verify FromStr roundtrip.
    let parsed = task_report::Priority::from_str("medium").unwrap();
    assert_eq!(parsed, task_report::Priority::Medium);

    // Verify VARIANT_NAMES constant.
    assert_eq!(
        task_report::Priority::VARIANT_NAMES,
        ["Critical", "High", "Medium", "Low"]
    );

    // Verify ALL constant.
    assert_eq!(task_report::Priority::ALL.len(), 4);
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
    let params = cross_crate_complex::Params {
        username: "eve".into(),
        role: cross_crate_complex::ParamsRole::Viewer,
        score: 77.3,
        active: true,
        tags: vec![
            cross_crate_complex::ParamsTagsItem {
                label: "performance".into(),
            },
            cross_crate_complex::ParamsTagsItem {
                label: "review".into(),
            },
        ],
    };

    // Use zero-arg render() which uses the embedded template.
    let output = params.render().unwrap();

    assert_eq!(
        output,
        "\nUser: eve\nRole: Viewer\nScore: 77.3\nActive: true\n\nTags:\n- performance\n- review\n"
    );
}

// ── Test 6: validate_template succeeds for matching template ────────────

#[test]
fn test_validate_template_succeeds_for_matching() {
    let tmpl = cross_crate_complex::template();
    cross_crate_complex::Params::validate_template(tmpl).unwrap();
}

// ── Test 7: task_report full roundtrip with typed enum params ────────────

#[test]
fn test_task_report_roundtrip_with_typed_enums() {
    let params = task_report::Params {
        title: "Update deployment pipeline".into(),
        priority: task_report::ParamsPriority::Critical,
        tasks: vec![
            task_report::ParamsTasksItem {
                name: "fix build script".into(),
                urgency: task_report::ParamsTasksItemUrgency::High,
            },
            task_report::ParamsTasksItem {
                name: "write tests".into(),
                urgency: task_report::ParamsTasksItemUrgency::Medium,
            },
        ],
    };

    let output = params.render().unwrap();

    assert_eq!(
        output,
        "\n# Task Report: Update deployment pipeline\n\nPriority: Critical\n- fix build script (High)\n- write tests (Medium)\n"
    );
}

// ── Test 8: ctx! macro interop with include_template ────────────────────

#[test]
fn test_ctx_macro_with_include_template() {
    let tmpl = task_report::template();

    // Use the ctx! macro (from prompt_templates) with macros (from prompt_templates_macros).
    let ctx = ctx! {
        title: "Refactor auth module",
        priority: "High",
        tasks: [
            { name: "extract interfaces", urgency: "Critical" },
            { name: "add error handling", urgency: "High" }
        ]
    };

    let output = tmpl.render_ctx(&ctx).unwrap();

    assert_eq!(
        output,
        "\n# Task Report: Refactor auth module\n\nPriority: High\n- extract interfaces (Critical)\n- add error handling (High)\n"
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
    let tmpl = cross_crate_complex::template();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        output,
        "\nUser: frank\nRole: Editor\nScore: 88\nActive: false\n\nTags:\n"
    );
}

// ── Test 10: Value enum interop with template rendering ─────────────────

#[test]
fn test_value_dict_rendering() {
    let tmpl = cross_crate_complex::template();

    // Build context entirely from Value constructors.
    let mut ctx = prompt_templates::Context::new();
    ctx.set("username", Value::Str("grace".into()));
    ctx.set("role", Value::Str("Admin".into()));
    ctx.set("score", Value::Float(100.0));
    ctx.set("active", Value::Bool(true));
    ctx.set(
        "tags",
        Value::List(std::sync::Arc::new(vec![Value::new_struct([(
            "label",
            Value::Str("perf".into()),
        )])])),
    );

    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        output,
        "\nUser: grace\nRole: Admin\nScore: 100\nActive: true\n\nTags:\n- perf\n"
    );
}

// ── Test 11: render_reloaded for hot-reload ─────────────────────────────

#[test]
fn test_render_reloaded_hot_reload() {
    let params = cross_crate_complex::Params {
        username: "hot_reload".into(),
        role: cross_crate_complex::ParamsRole::Admin,
        score: 50.0,
        active: true,
        tags: vec![],
    };

    // Load the same template from disk (hot-reload scenario).
    let disk_tmpl =
        Template::from_file(std::path::Path::new("prompts/cross_crate_complex.tmpl.md")).unwrap();

    // render_reloaded() validates then renders.
    let output = params.render_reloaded(&disk_tmpl).unwrap();
    assert_eq!(
        output,
        "\nUser: hot_reload\nRole: Admin\nScore: 50\nActive: true\n\nTags:\n"
    );
}
