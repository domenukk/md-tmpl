//! Integration tests for the md-tmpl-macros crate.
//!
//! Exercises `include_template!` and `template!` — the compile-time proc macros.

// ── include_template! ──────────────────────────────────────────────

// Generate the module from the template — this emits:
//   pub mod greeting { pub fn template() ...; pub struct Params { ... } ... }
md_tmpl_macros::include_template!("prompts/greeting.tmpl.md");

// Also test a simple template.
md_tmpl_macros::include_template!("prompts/simple_greeting.tmpl.md");

#[test]
fn include_template_loads_and_renders() {
    let tmpl = greeting::template();

    let mut ctx = md_tmpl::Context::new();
    ctx.set("name", "Alice");
    ctx.set("count", 42);
    ctx.set(
        "items",
        md_tmpl::Value::List(std::sync::Arc::new(vec![md_tmpl::Value::Struct(
            std::sync::Arc::new(
                [("label".to_string(), md_tmpl::Value::Str("hello".into()))]
                    .into_iter()
                    .collect(),
            ),
        )])),
    );

    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "\nHello Alice! Count: 42. Items: hello");
}

#[test]
fn include_template_produces_equivalent_templates() {
    // Two different template calls, but both point to the same module.
    let tmpl1 = simple_greeting::template();
    let tmpl2 = simple_greeting::template();

    let mut ctx = md_tmpl::Context::new();
    ctx.set("name", "Test");
    assert_eq!(
        tmpl1.render_ctx(&ctx).unwrap(),
        tmpl2.render_ctx(&ctx).unwrap(),
        "both invocations should produce identical results"
    );
}

#[test]
fn include_template_hot_loop_pattern() {
    let tmpl = simple_greeting::template();

    let names = ["Alice", "Bob", "Charlie"];
    for name in &names {
        let mut ctx = md_tmpl::Context::new();
        ctx.set("name", *name);
        let output = tmpl.render_ctx(&ctx).unwrap();
        assert_eq!(output, format!("\nHello {name}!\n"));
    }
}

// ── Module-level Params struct (from include_template!) ──────────────

#[test]
fn params_struct_renders_template() {
    let params = greeting::Params {
        name: "Alice".into(),
        count: 42,
        items: vec![greeting::ParamsItemsItem {
            label: "hello".into(),
        }],
    };

    // Zero-arg render using the embedded template.
    let output = params.render().unwrap();
    assert_eq!(output, "\nHello Alice! Count: 42. Items: hello");
}

#[test]
fn params_struct_render_reloaded_with_external_template() {
    // Test hot-reload: render_reloaded() accepts an externally-loaded template.
    let params = greeting::Params {
        name: "Bob".into(),
        count: 1,
        items: vec![],
    };

    let tmpl =
        md_tmpl::Template::from_file(std::path::Path::new("prompts/greeting.tmpl.md")).unwrap();
    greeting::Params::validate_template(&tmpl).unwrap();

    let output = params.render_reloaded(&tmpl).unwrap();
    assert_eq!(output, "\nHello Bob! Count: 1. Items: ");
}

#[test]
fn params_struct_validate_template_succeeds_for_matching() {
    let tmpl = greeting::template();
    greeting::Params::validate_template(tmpl).unwrap();
}

#[test]
fn params_struct_validate_template_fails_for_mismatched() {
    // Load a different template that has different params.
    let tmpl = md_tmpl::Template::from_source(
        "\
---
name: wrong
params: [totally_different = str]
---
{{ totally_different }}
",
    )
    .unwrap();

    let result = greeting::Params::validate_template(&tmpl);
    assert!(result.is_err(), "should fail with mismatched params");
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
    let tmpl = greeting::template();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "\nHello Test! Count: 99. Items: ab");
}

// ── include_template! with custom module name ─────────────────────────

md_tmpl_macros::include_template!("prompts/simple_greeting.tmpl.md" => my_greeting);

#[test]
fn custom_module_name_works() {
    let output = my_greeting::Params {
        name: "Custom".into(),
    }
    .render()
    .unwrap();
    assert_eq!(output, "\nHello Custom!\n");
}

// ── include_template! with types: block ─────────────────────────────

md_tmpl_macros::include_template!("prompts/type_library.tmpl.md");

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

// ── template! (inline) with module ────────────────────────────────────

md_tmpl_macros::template!(
    r#"
---
params:
  - name = str
---
Hello {{ name }}!
"# => inline_greeting
);

#[test]
fn template_inline_basic_render() {
    let output = inline_greeting::Params {
        name: "World".into(),
    }
    .render()
    .unwrap();
    assert_eq!(output, "Hello World!\n");
}

#[test]
fn template_inline_template_accessor() {
    let tmpl = inline_greeting::template();
    let mut ctx = md_tmpl::Context::new();
    ctx.set("name", "Test");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "Hello Test!\n");
}

md_tmpl_macros::template!(
    r#"
---
params:
  - greeting = str := "Howdy"
  - name = str
---
{{ greeting }} {{ name }}!
"# => defaults_tmpl
);

#[test]
fn template_inline_with_defaults() {
    let tmpl = defaults_tmpl::template();
    let mut ctx = tmpl.defaults_context();
    ctx.set("name", "Partner");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "Howdy Partner!\n");
}

md_tmpl_macros::template!(
    r#"
---
params:
  - user = str
  - count = int
  - active = bool
---
{{ user }}: {{ count }} (active={{ active }})
"# => multi_param
);

#[test]
fn template_inline_multi_param_types() {
    let tmpl = multi_param::template();

    let mut ctx = md_tmpl::Context::new();
    ctx.set("user", "Alice");
    ctx.set("count", 42);
    ctx.set("active", true);
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "Alice: 42 (active=true)\n");
}

md_tmpl_macros::template!(
    r#"
---
params:
  - x = str
  - y = int := 10
---
{{ x }}: {{ y }}
"# => decls_tmpl
);

#[test]
fn template_inline_declarations() {
    let tmpl = decls_tmpl::template();

    let decls = tmpl.declarations();
    assert_eq!(decls.len(), 2);
    assert_eq!(decls[0].name, "x");
    assert_eq!(decls[1].name, "y");
    assert_eq!(decls[1].default_value, Some(md_tmpl::Value::Int(10)));
}

md_tmpl_macros::template!(
    r#"
---
params:
  - item = str
---
Buy: {{ item }}
"# => reuse_tmpl
);

#[test]
fn template_inline_reuse_in_loop() {
    let tmpl = reuse_tmpl::template();

    let items = ["Coffee", "Tea", "Juice"];
    for item in &items {
        let mut ctx = md_tmpl::Context::new();
        ctx.set("item", *item);
        let output = tmpl.render_ctx(&ctx).unwrap();
        assert_eq!(output, format!("Buy: {item}\n"));
    }
}

// ── template() function accessible ────────────────────────────────────

#[test]
fn include_template_module_has_template_function() {
    // Verify the module exposes a template() accessor.
    let tmpl: &'static md_tmpl::Template = greeting::template();
    assert!(
        !tmpl.declarations().is_empty(),
        "template should have declarations"
    );
}

// ── include_template! with option(T) params ──────────────────────────

md_tmpl_macros::include_template!("prompts/option_test.tmpl.md");

#[test]
fn option_param_struct_fields_are_option_type() {
    // Verify that option(str) generates Option<String> and
    // option(int) generates Option<i64>.
    let params = option_test::Params {
        name: "Alice".into(),
        nickname: Some("Ali".into()),
        age: Some(30),
    };
    assert_eq!(params.name, "Alice");
    assert_eq!(params.nickname, Some("Ali".to_string()));
    assert_eq!(params.age, Some(30));
}

#[test]
fn option_param_none_renders_correctly() {
    let params = option_test::Params {
        name: "Bob".into(),
        nickname: None,
        age: None,
    };

    let output = params.render().unwrap();
    // With None for both options, the if-has blocks should be skipped.
    assert!(output.contains("Hello Bob!"), "output: {output}");
    assert!(
        !output.contains("Nickname:"),
        "None nickname should not render, output: {output}"
    );
    assert!(
        !output.contains("Age:"),
        "None age should not render, output: {output}"
    );
}

#[test]
fn option_param_some_renders_correctly() {
    let params = option_test::Params {
        name: "Charlie".into(),
        nickname: Some("Chuck".into()),
        age: Some(25),
    };

    let output = params.render().unwrap();
    assert!(output.contains("Hello Charlie!"), "output: {output}");
    assert!(output.contains("Nickname: Chuck"), "output: {output}");
    assert!(output.contains("Age: 25"), "output: {output}");
}

#[test]
fn option_param_to_context_produces_valid_context() {
    let params = option_test::Params {
        name: "Dave".into(),
        nickname: Some("D".into()),
        age: None,
    };

    let ctx = params.to_context();
    let tmpl = option_test::template();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("Hello Dave!"), "output: {output}");
    assert!(output.contains("Nickname: D"), "output: {output}");
    assert!(!output.contains("Age:"), "output: {output}");
}

#[test]
fn option_param_defaults_to_none() {
    // Option fields should default to None (no value set).
    let params = option_test::Params {
        name: "Eve".into(),
        nickname: None,
        age: None,
    };

    let output = params.render().unwrap();
    assert!(output.contains("Hello Eve!"), "output: {output}");
}

// ── template! inline with option(T) ──────────────────────────────────

md_tmpl_macros::template!(
    r#"
---
params:
  - label = str
  - count = option(int)
---
{{ label }}

> {% if has(count) %}

({{ count }})

> {% /if %}
"# => option_inline
);

#[test]
fn template_inline_option_none() {
    let output = option_inline::Params {
        label: "test".into(),
        count: None,
    }
    .render()
    .unwrap();
    assert!(output.contains("test"), "output: {output}");
    assert!(
        !output.contains('('),
        "None should skip the block, output: {output}"
    );
}

#[test]
fn template_inline_option_some() {
    let output = option_inline::Params {
        label: "test".into(),
        count: Some(42),
    }
    .render()
    .unwrap();
    assert!(output.contains("test"), "output: {output}");
    assert!(output.contains("(42)"), "output: {output}");
}

// ── include_template! with filters (parsed_num codegen) ──────────────

md_tmpl_macros::include_template!("prompts/filter_test.tmpl.md");

#[test]
fn filter_codegen_all_filters_render() {
    let params = filter_test::Params {
        name: "  Alice  ".into(),
        score: 3.45679,
        count: 7,
        items: vec![
            filter_test::ParamsItemsItem {
                label: "alpha".into(),
            },
            filter_test::ParamsItemsItem {
                label: "beta".into(),
            },
        ],
    };

    let output = params.render().unwrap();
    // upper
    assert!(
        output.contains("Upper:   ALICE  "),
        "upper filter failed, output: {output}"
    );
    // lower
    assert!(
        output.contains("Lower:   alice  "),
        "lower filter failed, output: {output}"
    );
    // trim
    assert!(
        output.contains("Trim: Alice"),
        "trim filter failed, output: {output}"
    );
    // fixed(2) — this uses parsed_num
    assert!(
        output.contains("Fixed: 3.46"),
        "fixed(2) filter failed (parsed_num codegen), output: {output}"
    );
    // add(10) — this uses parsed_num
    assert!(
        output.contains("Added: 17"),
        "add(10) filter failed (parsed_num codegen), output: {output}"
    );
    // sub(3) — this uses parsed_num
    assert!(
        output.contains("Subtracted: 4"),
        "sub(3) filter failed (parsed_num codegen), output: {output}"
    );
    // filter inside for loop
    assert!(
        output.contains("Items: ALPHABETA"),
        "filter-in-for-loop failed, output: {output}"
    );
}

#[test]
fn filter_codegen_matches_runtime() {
    // Verify that the compile-time (macro) path and the runtime path
    // produce identical output for all filters.
    let tmpl_compiled = filter_test::template();
    let tmpl_runtime =
        md_tmpl::Template::from_file(std::path::Path::new("prompts/filter_test.tmpl.md")).unwrap();

    let mut ctx = md_tmpl::Context::new();
    ctx.set("name", "  Bob  ");
    ctx.set("score", md_tmpl::Value::Float(2.98765));
    ctx.set("count", 100);
    ctx.set(
        "items",
        md_tmpl::Value::List(std::sync::Arc::new(vec![md_tmpl::Value::Struct(
            std::sync::Arc::new(
                [("label".to_string(), md_tmpl::Value::Str("x".into()))]
                    .into_iter()
                    .collect(),
            ),
        )])),
    );

    let compiled_output = tmpl_compiled.render_ctx(&ctx).unwrap();
    let runtime_output = tmpl_runtime.render_ctx(&ctx).unwrap();
    assert_eq!(
        compiled_output, runtime_output,
        "compile-time and runtime filter outputs must match"
    );
}

// ── include_template! with typed env ─────────────────────────────────

md_tmpl_macros::include_template!(
    "prompts/env_typed.tmpl.md",
    env = { MAX_RETRIES: 5, DEBUG: true }
);

#[test]
fn include_template_typed_env_resolves_at_compile_time() {
    let output = env_typed::Params {
        name: "World".into(),
    }
    .render()
    .unwrap();
    assert!(output.contains("Retries: 5"), "output: {output}");
    assert!(
        output.contains("Debug mode enabled."),
        "DEBUG=true should enable debug block, output: {output}"
    );
}

#[test]
fn include_template_typed_env_consts_baked_in() {
    // Verify the env values are baked in as consts — the template
    // should not accept MAX_RETRIES or DEBUG as params.
    let tmpl = env_typed::template();
    let decls = tmpl.declarations();
    // Only `name` should be a param; MAX_RETRIES and DEBUG are env consts.
    assert_eq!(decls.len(), 1, "only name should be a param: {decls:?}");
    assert_eq!(decls[0].name, "name");
}

// ── template! (inline) with typed env ────────────────────────────────

md_tmpl_macros::template!(
    r#"
---
env:
  - RETRIES = int
  - VERBOSE = bool := false

params:
  - msg = str
---
{{ msg }} (retries={{ RETRIES }})

> {% if VERBOSE %}

[verbose]

> {% /if %}
"# => env_inline,
    env = { RETRIES: 3, VERBOSE: false }
);

#[test]
fn template_inline_typed_env_renders() {
    let output = env_inline::Params {
        msg: "hello".into(),
    }
    .render()
    .unwrap();
    assert_eq!(output, "hello (retries=3)\n");
}

md_tmpl_macros::template!(
    r#"
---
env:
  - RETRIES = int
  - VERBOSE = bool := false

params:
  - msg = str
---
{{ msg }} (retries={{ RETRIES }})

> {% if VERBOSE %}

[verbose]

> {% /if %}
"# => env_inline_verbose,
    env = { RETRIES: 10, VERBOSE: true }
);

#[test]
fn template_inline_typed_env_verbose_true() {
    let output = env_inline_verbose::Params { msg: "test".into() }
        .render()
        .unwrap();
    assert!(output.contains("retries=10"), "output: {output}");
    assert!(output.contains("[verbose]"), "output: {output}");
}

// ── template! with string env (backward compat) ────────────────────

md_tmpl_macros::template!(
    r#"
---
env: [LABEL = str]

params: [count = int]
---
{{ LABEL }}: {{ count }}
"# => env_str_compat,
    env = { LABEL: "Total" }
);

#[test]
fn template_inline_string_env_still_works() {
    let output = env_str_compat::Params { count: 42 }.render().unwrap();
    assert_eq!(output, "Total: 42\n");
}
