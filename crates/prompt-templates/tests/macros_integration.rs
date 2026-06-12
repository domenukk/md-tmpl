//! Integration tests for the prompt-templates-macros crate.
//!
//! Exercises `include_template!`, `validate_template!`, and
//! `include_types!` — the compile-time proc macros that were
//! previously only shown as documentation examples.

// ── include_template! ──────────────────────────────────────────────

#[test]
fn include_template_loads_and_renders() {
    let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");

    let mut ctx = prompt_templates::Context::new();
    ctx.set("name", "Alice");
    ctx.set("count", 42);
    ctx.set(
        "items",
        prompt_templates::Value::List(vec![prompt_templates::Value::Dict(
            [(
                "label".to_string(),
                prompt_templates::Value::Str("hello".into()),
            )]
            .into_iter()
            .collect(),
        )]),
    );

    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("Hello Alice!"), "got: {output}");
    assert!(output.contains("Count: 42"), "got: {output}");
    assert!(output.contains("hello"), "got: {output}");
}

#[test]
fn include_template_produces_equivalent_templates() {
    // Each include_template! call-site gets its own LazyLock, but the
    // resulting templates should be structurally identical.
    let tmpl1 = prompt_templates_macros::include_template!("prompts/simple_greeting.tmpl.md");
    let tmpl2 = prompt_templates_macros::include_template!("prompts/simple_greeting.tmpl.md");

    let mut ctx = prompt_templates::Context::new();
    ctx.set("name", "Test");
    assert_eq!(
        tmpl1.render(&ctx).unwrap(),
        tmpl2.render(&ctx).unwrap(),
        "both invocations should produce identical results"
    );
}

#[test]
fn include_template_hot_loop_pattern() {
    // This is the exact pattern from the doc comment: bind once, render in a loop.
    let tmpl = prompt_templates_macros::include_template!("prompts/simple_greeting.tmpl.md");

    let names = ["Alice", "Bob", "Charlie"];
    for name in &names {
        let mut ctx = prompt_templates::Context::new();
        ctx.set("name", *name);
        let output = tmpl.render(&ctx).unwrap();
        assert!(
            output.contains(name),
            "expected '{name}' in output, got: {output}"
        );
    }
}

// ── validate_template! ─────────────────────────────────────────────

#[test]
fn validate_template_compiles_valid_template() {
    // This should compile without error — if the template were invalid,
    // compilation itself would fail.
    prompt_templates_macros::validate_template!("prompts/simple_greeting.tmpl.md");
    prompt_templates_macros::validate_template!("prompts/greeting.tmpl.md");
}

// ── include_types! ────────────────────────────────────────────────────

prompt_templates_macros::include_types!("prompts/greeting.tmpl.md");

#[test]
fn params_struct_renders_template() {
    let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");

    let params = greeting::Params {
        name: "Alice".into(),
        count: 42,
        items: vec![greeting::ParamsItemsItem {
            label: "hello".into(),
        }],
    };

    let output = params.render(tmpl).unwrap();
    assert!(output.contains("Hello Alice!"), "got: {output}");
    assert!(output.contains("Count: 42"), "got: {output}");
    assert!(output.contains("hello"), "got: {output}");
}

#[test]
fn params_struct_validate_template_succeeds_for_matching() {
    let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
    greeting::Params::validate_template(tmpl).unwrap();
}

#[test]
fn params_struct_validate_template_fails_for_mismatched() {
    // Load a different template that has different params.
    let tmpl = prompt_templates::Template::from_source(
        "---\nname: wrong\nparams: [totally_different = str]\n---\n{{ totally_different }}\n",
    )
    .unwrap();

    let result = greeting::Params::validate_template(&tmpl);
    assert!(result.is_err(), "should fail with mismatched params");
}

#[test]
fn params_struct_hot_reload_with_disk_template() {
    // Exercises the hot-reload pattern from the doc example:
    // compile-time struct + runtime-loaded template.
    let tmpl =
        prompt_templates::Template::from_file(std::path::Path::new("prompts/greeting.tmpl.md"))
            .unwrap();

    greeting::Params::validate_template(&tmpl).unwrap();

    let params = greeting::Params {
        name: "Bob".into(),
        count: 1,
        items: vec![],
    };

    let output = params.render(&tmpl).unwrap();
    assert!(output.contains("Bob"), "got: {output}");
    assert!(output.contains("Count: 1"), "got: {output}");
}

#[test]
fn params_struct_to_context_produces_valid_context() {
    let params = greeting::Params {
        name: "Test".into(),
        count: 99,
        items: vec![
            greeting::ParamsItemsItem { label: "a".into() },
            greeting::ParamsItemsItem { label: "b".into() },
        ],
    };

    let ctx = params.to_context();
    let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains("Test"), "got: {output}");
    assert!(output.contains("99"), "got: {output}");
    assert!(output.contains('a'), "got: {output}");
    assert!(output.contains('b'), "got: {output}");
}

// ── include_types! with types: block ─────────────────────────────────

prompt_templates_macros::include_types!("prompts/type_library.tmpl.md");

#[test]
fn type_alias_enum_variants_exist() {
    // Unit-variant enum: all variants should exist and be constructable.
    let _ = type_library::Priority::Low;
    let _ = type_library::Priority::Medium;
    let _ = type_library::Priority::High;
    let _ = type_library::Priority::Critical;

    let _ = type_library::Status::Open;
    let _ = type_library::Status::InProgress;
    let _ = type_library::Status::Resolved;
    let _ = type_library::Status::Closed;
}

#[test]
fn type_alias_enum_display() {
    assert_eq!(type_library::Priority::Low.to_string(), "Low");
    assert_eq!(type_library::Priority::Critical.to_string(), "Critical");
    assert_eq!(type_library::Status::InProgress.to_string(), "InProgress");
    assert_eq!(type_library::Status::Closed.to_string(), "Closed");
}

#[test]
fn type_alias_enum_from_str() {
    use std::str::FromStr;
    assert_eq!(
        type_library::Priority::from_str("low").unwrap(),
        type_library::Priority::Low
    );
    assert_eq!(
        type_library::Priority::from_str("CRITICAL").unwrap(),
        type_library::Priority::Critical
    );
    assert_eq!(
        type_library::Status::from_str("inprogress").unwrap(),
        type_library::Status::InProgress
    );
    assert!(type_library::Priority::from_str("unknown").is_err());
}

#[test]
fn type_alias_enum_variant_names() {
    assert_eq!(
        type_library::Priority::VARIANT_NAMES,
        ["Low", "Medium", "High", "Critical"]
    );
    assert_eq!(
        type_library::Status::VARIANT_NAMES,
        ["Open", "InProgress", "Resolved", "Closed"]
    );
}

#[test]
fn type_alias_enum_all() {
    assert_eq!(type_library::Priority::ALL.len(), 4);
    assert_eq!(type_library::Priority::ALL[0], type_library::Priority::Low);
    assert_eq!(
        type_library::Priority::ALL[3],
        type_library::Priority::Critical
    );
}

#[test]
fn type_alias_enum_copy_hash_eq() {
    use std::collections::HashSet;
    // Unit-variant enums should be Copy + Hash.
    let p = type_library::Priority::High;
    let p2 = p; // Copy
    assert_eq!(p, p2);

    let mut set = HashSet::new();
    set.insert(type_library::Priority::Low);
    set.insert(type_library::Priority::High);
    assert!(set.contains(&type_library::Priority::Low));
}

#[test]
fn type_alias_enum_display_roundtrip() {
    // Verify Display → FromStr roundtrip works.
    use std::str::FromStr;
    for p in &type_library::Priority::ALL {
        let s = p.to_string();
        let parsed = type_library::Priority::from_str(&s).unwrap();
        assert_eq!(*p, parsed, "roundtrip failed for {s}");
    }
}

#[test]
fn type_alias_constants_accessible() {
    assert_eq!(type_library::APP_NAME, "TestApp");
    assert_eq!(type_library::MAX_RETRIES, 3);
}

#[test]
fn type_alias_data_enum_exists() {
    // Data-variant enum (Outcome has Confirmed(evidence=str)).
    let confirmed = type_library::Outcome::Confirmed {
        evidence: "proof".into(),
    };
    let rejected = type_library::Outcome::Rejected;
    assert_ne!(confirmed, rejected);
}
