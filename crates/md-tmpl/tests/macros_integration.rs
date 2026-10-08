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

// ── template! with Rust keyword param names (raw ident codegen) ─────

md_tmpl_macros::template!(
    r#"
---
params:
  - type = str
  - match = int
  - loop = str
---
{{ type }}: {{ match }} (loop={{ loop }})
"# => keyword_params
);

#[test]
fn template_keyword_param_names_render() {
    let output = keyword_params::Params {
        r#type: "widget".into(),
        r#match: 42,
        r#loop: "forever".into(),
    }
    .render()
    .unwrap();
    assert_eq!(output, "widget: 42 (loop=forever)\n");
}

#[test]
fn template_keyword_param_to_context() {
    let params = keyword_params::Params {
        r#type: "gadget".into(),
        r#match: 7,
        r#loop: "once".into(),
    };
    let ctx = params.to_context();
    let tmpl = keyword_params::template();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "gadget: 7 (loop=once)\n");
}

// ── template! with keyword as struct field name ─────────────────────

md_tmpl_macros::template!(
    r#"
---
params:
  - items = list(type = str, value = int)
---

> {% for item in items %}

{{ item.type }}={{ item.value }}

> {% /for %}
"# => keyword_struct_field
);

#[test]
fn template_keyword_struct_field_render() {
    let output = keyword_struct_field::Params {
        items: vec![
            keyword_struct_field::ParamsItemsItem {
                r#type: "a".into(),
                value: 1,
            },
            keyword_struct_field::ParamsItemsItem {
                r#type: "b".into(),
                value: 2,
            },
        ],
    }
    .render()
    .unwrap();
    assert_eq!(output, "a=1\nb=2\n");
}

// ── template! with un-escapable keywords (self → __self) ────────────

md_tmpl_macros::template!(
    r#"
---
params:
  - self = str
  - super = str
---
{{ self }}: {{ super }}
"# => unescapable_keyword_params
);

#[test]
fn template_unescapable_keyword_self_renders() {
    let output = unescapable_keyword_params::Params {
        __self: "me".into(),
        __super: "parent".into(),
    }
    .render()
    .unwrap();
    assert_eq!(output, "me: parent\n");
}

#[test]
fn template_unescapable_keyword_to_context() {
    let params = unescapable_keyword_params::Params {
        __self: "me".into(),
        __super: "up".into(),
    };
    let ctx = params.to_context();
    let tmpl = unescapable_keyword_params::template();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(output, "me: up\n");
}

// ── Constant pruning & StaticScalar codegen verification ────────────

md_tmpl_macros::include_template!("prompts/role_consumer.tmpl.md");

md_tmpl_macros::template!(
    r#"
---
types:
  - UsedEnum = enum(Alpha, Beta, Data(msg = str))
  - UnusedEnum = enum(One, Two, Three)

consts:
  - USED_STR = str := "kept"
  - UNUSED_STR = str := "pruned"
  - USED_LIST = list(str) := ["x", "y"]
  - UNUSED_LIST = list(int) := [1, 2, 3, 4]
  - USED_STRUCT_LIST = list(k = str, v = int) := [{k = "a", v = 1}, {k = "b", v = 2}]
  - UNUSED_STRUCT_LIST = list(k = str, v = int) := [{k = "z", v = 99}]

params:
  - tag = str
---
{{ consts.USED_STR }}:{{ kind(UsedEnum.Alpha) }}:{{ kinds(UsedEnum) | join(",") }}

> {% for item in USED_LIST %}

[{{ item }}]

> {% /for %}

> {% for row in USED_STRUCT_LIST %}

({{ row.k }}={{ row.v }})

> {% /for %}

> {% match tag %}
> {% case "{{ USED_STR }}_arm" %}

matched!

> {% else %}

other

> {% /match %}
"# => const_pruning_test
);

#[test]
fn unreferenced_consts_and_imported_consts_are_pruned_from_runtime_template() {
    // 1. In `type_library` (no body expressions), top-level Rust pub consts exist,
    //    while the runtime Template's `consts` and `imported_consts` maps are empty.
    assert_eq!(type_library::APP_NAME, "TestApp");
    assert_eq!(type_library::MAX_RETRIES, 3);
    let lib_tmpl = type_library::template();
    assert!(
        lib_tmpl.consts().is_empty(),
        "unreferenced consts should be pruned from __TEMPLATE: {:?}",
        lib_tmpl.consts()
    );
    assert!(
        lib_tmpl.imported_consts().is_empty(),
        "unreferenced enum type namespaces should be pruned from __TEMPLATE: {:?}",
        lib_tmpl.imported_consts()
    );

    // 2. In `role_consumer`, `roles_lib.WorkRole` is used only as a param type,
    //    never as a runtime value in the body, so `imported_consts` is empty.
    let consumer_tmpl = role_consumer::template();
    assert!(consumer_tmpl.imported_consts().is_empty());
    let rendered = role_consumer::Params {
        role: role_consumer::ParamsRole::Researcher,
    }
    .render()
    .unwrap();
    assert_eq!(rendered.trim(), "Role: Researcher");

    // 3. In `const_pruning_test`, referenced consts/enums are kept in `__TEMPLATE`,
    //    while unreferenced ones are pruned, and top-level Rust consts remain accessible.
    assert_eq!(const_pruning_test::USED_STR, "kept");
    assert_eq!(const_pruning_test::UNUSED_STR, "pruned");
    assert_eq!(&*const_pruning_test::UNUSED_LIST, &[1, 2, 3, 4]);
    assert_eq!(const_pruning_test::UNUSED_STRUCT_LIST.len(), 1);

    let tmpl = const_pruning_test::template();
    assert!(tmpl.consts().contains_key("USED_STR"));
    assert!(tmpl.consts().contains_key("USED_LIST"));
    assert!(tmpl.consts().contains_key("USED_STRUCT_LIST"));
    assert!(!tmpl.consts().contains_key("UNUSED_STR"));
    assert!(!tmpl.consts().contains_key("UNUSED_LIST"));
    assert!(!tmpl.consts().contains_key("UNUSED_STRUCT_LIST"));

    assert!(tmpl.imported_consts().contains_key("UsedEnum"));
    assert!(!tmpl.imported_consts().contains_key("UnusedEnum"));

    // Verify native `Params::render()` and dynamic `Template::render_ctx()` produce identical output.
    let params = const_pruning_test::Params {
        tag: "kept_arm".into(),
    };
    let native_out = params.render().unwrap();
    let dyn_out = tmpl.render_ctx(&params.to_context()).unwrap();
    assert_eq!(native_out, dyn_out);
    assert!(native_out.contains("kept:Alpha:Alpha,Beta,Data"));
    assert!(native_out.contains("[x]\n[y]"));
    assert!(native_out.contains("(a=1)\n(b=2)"));
    assert!(native_out.contains("matched!"));
}
