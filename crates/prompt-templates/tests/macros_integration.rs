//! Integration tests for the prompt-templates-macros crate.
//!
//! Exercises `include_template!`, `validate_template!`, and
//! `template_params_struct!` — the compile-time proc macros that were
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

// ── template_params_struct! ────────────────────────────────────────

prompt_templates_macros::template_params_struct!("prompts/greeting.tmpl.md" => GreetingParams);

#[test]
fn params_struct_renders_template() {
    let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");

    let params = GreetingParams {
        name: "Alice".into(),
        count: 42,
        items: vec![GreetingParamsItemsItem {
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
    GreetingParams::validate_template(tmpl).unwrap();
}

#[test]
fn params_struct_validate_template_fails_for_mismatched() {
    // Load a different template that has different params.
    let tmpl = prompt_templates::Template::from_source(
        "---\nname: wrong\nparams: [totally_different = str]\n---\n{{ totally_different }}\n",
    )
    .unwrap();

    let result = GreetingParams::validate_template(&tmpl);
    assert!(result.is_err(), "should fail with mismatched params");
}

#[test]
fn params_struct_hot_reload_with_disk_template() {
    // Exercises the hot-reload pattern from the doc example:
    // compile-time struct + runtime-loaded template.
    let tmpl =
        prompt_templates::Template::from_file(std::path::Path::new("prompts/greeting.tmpl.md"))
            .unwrap();

    GreetingParams::validate_template(&tmpl).unwrap();

    let params = GreetingParams {
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
    let params = GreetingParams {
        name: "Test".into(),
        count: 99,
        items: vec![
            GreetingParamsItemsItem { label: "a".into() },
            GreetingParamsItemsItem { label: "b".into() },
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
