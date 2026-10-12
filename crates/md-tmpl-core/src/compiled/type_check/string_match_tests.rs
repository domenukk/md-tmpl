//! Type-check tests for string and param-reference `match` labels.

use super::{test_helpers::*, *};
use crate::types::{VarDecl, VarType};

// -- String match (quoted case labels) -----------------------------------

#[test]
fn string_match_on_str_param_ok() {
    let decls = vec![VarDecl {
        name: "status".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl = r#"---
name: t
params:
  - status = str
---
> {% match status %}{% case "Active" %}active{% case "Inactive" %}inactive{% /match %}"#;
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "quoted string cases on str should work: {errors:?}"
    );
}

#[test]
fn match_on_str_with_quoted_cases_renders() {
    let tmpl = crate::Template::from_source(
        r#"---
params:
  - status = str
---
> {% match status %}{% case "Active" %}active{% case "Inactive" %}inactive{% /match %}"#,
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set("status", "Active");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "active");
    ctx.set("status", "Inactive");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "inactive");
    ctx.set("status", "Unknown");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "");
}

#[test]
fn match_on_enum_with_quoted_cases_is_error() {
    let decls = vec![enum_decl(
        "status",
        vec![unit_variant("Active"), unit_variant("Inactive")],
    )];
    let tmpl = r#"---
name: t
params:
  - status = enum(Active, Inactive)
---
> {% match status %}{% case "Active" %}active{% /match %}"#;
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        !errors.is_empty(),
        "quoted cases on enum should error: {errors:?}"
    );
}

#[test]
fn inline_match_quoted_case_on_str_ok() {
    let decls = vec![VarDecl {
        name: "status".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl = r#"---
name: t
params:
  - status = str
---
> {% match status case "Active" %}yes{% /match %}"#;
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "inline quoted case on str should work: {errors:?}"
    );
}

#[test]
fn mixed_quoted_unquoted_cases_compiles_ok() {
    // Mixed quoted/unquoted labels on str params are allowed.
    // Quoted labels compare against the string value (quotes stripped),
    // while unquoted labels compare literally.
    let tmpl = crate::Template::from_source(
        r#"---
params:
  - status = str
---
> {% match status %}{% case "Active" %}a{% case Inactive %}b{% /match %}"#,
    )
    .expect("mixed quoted/unquoted should compile");
    let mut ctx = crate::Context::new();
    ctx.set("status", "Active");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "a");
    ctx.set("status", "Inactive");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "b");
}

#[test]
fn string_match_with_else_arm() {
    let tmpl = crate::Template::from_source(
        r#"---
params:
  - status = str
---
> {% match status %}{% case "Active" %}active{% else %}other{% /match %}"#,
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set("status", "Active");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "active");
    ctx.set("status", "Inactive");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "other");
    ctx.set("status", "Whatever");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "other");
}

#[test]
fn inline_single_quoted_case_renders() {
    let tmpl = crate::Template::from_source(
        r#"---
params:
  - status = str
---
> {% match status case "Active" %}yes{% /match %}"#,
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set("status", "Active");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "yes");
    ctx.set("status", "Inactive");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "");
}

// -- Param-reference matching (unquoted labels on str params) --------

#[test]
fn param_ref_match_compiles_when_label_is_declared_param() {
    let decls = vec![
        VarDecl {
            name: "status".to_string(),
            var_type: VarType::Str,
            default_value: None,
        },
        VarDecl {
            name: "expected".to_string(),
            var_type: VarType::Str,
            default_value: None,
        },
    ];
    let tmpl = r"---
name: t
params:
  - status = str
  - expected = str
---
> {% match status %}{% case expected %}matched{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "param-ref match with declared param should compile: {errors:?}"
    );
}

#[test]
fn str_match_unquoted_label_compiles_and_matches_literally() {
    // Unquoted labels on str params are compared literally at runtime.
    let tmpl = crate::Template::from_source(
        r"---
params:
  - status = str
---
> {% match status %}{% case Unknown %}nope{% else %}ok{% /match %}",
    )
    .expect("unquoted label on str should compile");
    let mut ctx = crate::Context::new();
    ctx.set("status", "Unknown");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "nope");
    ctx.set("status", "Other");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "ok");
}

#[test]
fn scalar_and_collection_types_in_if_condition_are_allowed() {
    let tmpl_str = "---
---
> {% if name %}hello{% /if %}";
    let errors = compile_and_check(tmpl_str, &[str_decl("name")]);
    assert!(
        errors.is_empty(),
        "str in if condition should succeed: {errors:?}"
    );

    let tmpl_int = "---
---
> {% if count %}hello{% /if %}";
    let errors = compile_and_check(tmpl_int, &[int_decl("count")]);
    assert!(
        errors.is_empty(),
        "int in if condition should succeed: {errors:?}"
    );

    let tmpl_list = "---
---
> {% if items %}hello{% /if %}";
    let errors = compile_and_check(
        tmpl_list,
        &[VarDecl {
            name: "items".to_string(),
            var_type: VarType::List(vec![VarDecl {
                name: String::new(),
                var_type: VarType::Str,
                default_value: None,
            }]),
            default_value: None,
        }],
    );
    assert!(
        errors.is_empty(),
        "list in bare if condition should succeed: {errors:?}"
    );
}

#[test]
fn has_on_str_and_list_rejected() {
    // has() is strictly for option(T) presence/unwrapping. String and list
    // presence is expressed with bare truthiness (`{% if title %}`), not has().
    let tmpl_str = r"---
---
> {% if has(title) %}x{% /if %}";
    let errors = compile_and_check(tmpl_str, &[str_decl("title")]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'has()' requires an option type")),
        "has(str) should be rejected: {errors:?}"
    );

    let tmpl_list = r"---
---
> {% if has(items) %}x{% /if %}";
    let errors = compile_and_check(
        tmpl_list,
        &[VarDecl {
            name: "items".to_string(),
            var_type: VarType::List(vec![VarDecl {
                name: String::new(),
                var_type: VarType::Str,
                default_value: None,
            }]),
            default_value: None,
        }],
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'has()' requires an option type")),
        "has(list) should be rejected: {errors:?}"
    );
}

#[test]
fn bare_truthiness_on_str_and_list_renders() {
    // The replacement for has(str)/has(list): bare truthiness.
    let tmpl = crate::Template::from_source(
        r"---
params:
  - title = str
  - items = list(str)
---
> {% if title %}title: {{ title }}{% /if %}
> {% if items %}items count: {{ len(items) }}{% /if %}",
    )
    .expect("bare truthiness on str/list should compile");

    let mut ctx = crate::Context::new();
    ctx.set("title", "");
    ctx.set("items", crate::Value::list(Vec::<crate::Value>::new()));
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "");

    ctx.set("title", "Hello");
    ctx.set(
        "items",
        crate::Value::list(vec![
            crate::Value::Str("a".to_string()),
            crate::Value::Str("b".to_string()),
        ]),
    );
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "title: Helloitems count: 2");
}

#[test]
fn bare_struct_enum_and_tmpl_in_condition_rejected() {
    let tmpl_struct = r"---
---
> {% if s %}hi{% /if %}";
    let struct_decl_val = VarDecl {
        name: "s".to_string(),
        var_type: VarType::Struct(vec![VarDecl {
            name: "id".to_string(),
            var_type: VarType::Int,
            default_value: None,
        }]),
        default_value: None,
    };
    let errors = compile_and_check(tmpl_struct, &[struct_decl_val]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot evaluate truthiness of struct")),
        "bare struct in if condition should fail: {errors:?}"
    );

    let tmpl_enum = r"---
types:
  - Status = enum(Active, Paused)
---
> {% if st %}hi{% /if %}";
    let enum_decl_val = VarDecl {
        name: "st".to_string(),
        var_type: VarType::Enum(vec![
            crate::types::VariantDecl {
                name: "Active".into(),
                fields: vec![],
            },
            crate::types::VariantDecl {
                name: "Paused".into(),
                fields: vec![],
            },
        ]),
        default_value: None,
    };
    let errors = compile_and_check(tmpl_enum, &[enum_decl_val]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot evaluate truthiness of enum")),
        "bare enum in if condition should fail: {errors:?}"
    );

    let tmpl_handle = r"---
---
> {% if t %}hi{% /if %}";
    let tmpl_decl_val = VarDecl {
        name: "t".to_string(),
        var_type: VarType::Tmpl(vec![]),
        default_value: None,
    };
    let errors = compile_and_check(tmpl_handle, &[tmpl_decl_val]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot evaluate truthiness of template handle")),
        "bare tmpl in if condition should fail: {errors:?}"
    );
}

#[test]
fn option_type_in_if_condition_succeeds_and_narrows() {
    let tmpl = crate::Template::from_source(
        r"---
params:
  - opt = option(str) := None
---
> {% if has(opt) %} · {{ opt }}{% /if %}",
    )
    .expect("option in has() condition should compile and narrow");

    let mut ctx = crate::Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "");

    ctx.set("opt", "High");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), " · High");
}

#[test]
fn has_on_invalid_types_rejected() {
    let tmpl_int = r"---
---
> {% if has(n) %}hi{% /if %}";
    let errors = compile_and_check(tmpl_int, &[int_decl("n")]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'has()' requires an option type")),
        "has(int) should fail: {errors:?}"
    );

    let tmpl_float = r"---
---
> {% if has(x) %}hi{% /if %}";
    let float_decl_val = VarDecl {
        name: "x".to_string(),
        var_type: VarType::Float,
        default_value: None,
    };
    let errors = compile_and_check(tmpl_float, &[float_decl_val]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'has()' requires an option type")),
        "has(float) should fail: {errors:?}"
    );

    let tmpl_enum = r"---
types:
  - Status = enum(Active, Paused)
---
> {% if has(st) %}hi{% /if %}";
    let enum_decl_val = VarDecl {
        name: "st".to_string(),
        var_type: VarType::Enum(vec![
            crate::types::VariantDecl {
                name: "Active".into(),
                fields: vec![],
            },
            crate::types::VariantDecl {
                name: "Paused".into(),
                fields: vec![],
            },
        ]),
        default_value: None,
    };
    let errors = compile_and_check(tmpl_enum, &[enum_decl_val]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'has()' requires an option type")),
        "has(enum) should fail: {errors:?}"
    );

    let tmpl_struct = r"---
---
> {% if has(s) %}hi{% /if %}";
    let struct_decl_val = VarDecl {
        name: "s".to_string(),
        var_type: VarType::Struct(vec![VarDecl {
            name: "id".to_string(),
            var_type: VarType::Int,
            default_value: None,
        }]),
        default_value: None,
    };
    let errors = compile_and_check(tmpl_struct, &[struct_decl_val]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'has()' requires an option type")),
        "has(struct) should fail: {errors:?}"
    );

    let tmpl_handle = r"---
---
> {% if has(t) %}hi{% /if %}";
    let tmpl_decl_val = VarDecl {
        name: "t".to_string(),
        var_type: VarType::Tmpl(vec![]),
        default_value: None,
    };
    let errors = compile_and_check(tmpl_handle, &[tmpl_decl_val]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'has()' requires an option type")),
        "has(tmpl) should fail: {errors:?}"
    );
}

#[test]
fn not_has_negative_narrowing_carry_in_rust() {
    let tmpl = crate::Template::from_source(
        r"---
params:
  - label = option(str) := None
---
> {% if !has(label) %}default{% else %}label: {{ label }}{% /if %}",
    )
    .expect("!has(label) should narrow label to inner str in else branch");

    let mut ctx = crate::Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "default");

    ctx.set("label", "custom");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "label: custom");
}

#[test]
fn has_on_option_enum_reports_present_for_struct_variant() {
    // Regression: a Some(enum struct-variant) carries a `__kind__` tag in its
    // transparent representation. Presence must be decided by Value::None only
    // (not by inspecting the tag), so has() reports such a value present.
    let tmpl = crate::Template::from_source(
        r"---
params:
  - status = option(enum(Active(score = int), Idle)) := None
---
> {% if has(status) %}present{% else %}absent{% /if %}",
    )
    .expect("option(enum) should compile");

    let mut ctx = crate::Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "absent");

    // Some(Active { score: 9 }) — transparent enum struct-variant payload.
    ctx.set(
        "status",
        crate::Value::new_struct([
            (
                crate::consts::ENUM_TAG_KEY,
                crate::Value::Str("Active".into()),
            ),
            ("score", crate::Value::Int(9)),
        ]),
    );
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "present");
}

#[test]
fn bare_option_condition_rejected_at_compile_time() {
    let decls = vec![VarDecl {
        name: "opt".to_string(),
        var_type: VarType::Option(Box::new(VarType::Str)),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - opt = option(str)
---
> {% if opt %}yes{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected bare option error: {errors:?}");
    assert!(
        errors[0].contains("cannot evaluate truthiness of option"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn has_and_unwrapped_operand_in_same_condition() {
    let tmpl = crate::Template::from_source(
        r"---
params:
  - test = option(str) := None
---
> {% if has(test) && test %}val: {{ test }}{% /if %}",
    )
    .expect("has(test) && test should compile cleanly due to left-to-right narrowing");

    let mut ctx = crate::Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "");

    ctx.set("test", "");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "");

    ctx.set("test", "hello");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "val: hello");
}

#[test]
fn kind_on_plain_str_is_compile_error() {
    let decls = vec![VarDecl {
        name: "msg".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl_str = r"---
params:
  - msg = str
---
{{ kind(msg) }}";
    let errors = compile_and_check(tmpl_str, &decls);
    assert_eq!(errors.len(), 1, "expected compile error: {errors:?}");
    assert!(
        errors[0].contains("'kind()' requires an enum or option type, got str"),
        "unexpected error: {}",
        errors[0]
    );

    // `Template::from_source` runs the same checker at compile time.
    let compile_err = crate::Template::from_source(tmpl_str).unwrap_err();
    assert!(
        compile_err
            .to_string()
            .contains("'kind()' requires an enum or option type, got str on 'msg'"),
        "unexpected compile error: {compile_err}"
    );
}

#[test]
fn len_on_int_is_compile_error() {
    let decls = vec![VarDecl {
        name: "count".to_string(),
        var_type: VarType::Int,
        default_value: None,
    }];
    let tmpl_str = r"---
params:
  - count = int
---
{{ len(count) }}";
    let errors = compile_and_check(tmpl_str, &decls);
    assert_eq!(errors.len(), 1, "expected compile error: {errors:?}");
    assert!(
        errors[0].contains("'len()' requires a list or str type, got int"),
        "unexpected error: {}",
        errors[0]
    );
}

#[test]
fn has_in_expr_on_plain_str_is_compile_error() {
    let decls = vec![VarDecl {
        name: "label".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl_str = r"---
params:
  - label = str
---
{{ has(label) }}";
    let errors = compile_and_check(tmpl_str, &decls);
    assert_eq!(errors.len(), 1, "expected compile error: {errors:?}");
    assert!(
        errors[0].contains("'has()' requires an option type, got str"),
        "unexpected error: {}",
        errors[0]
    );

    // `Template::from_source` runs the same checker at compile time.
    let compile_err = crate::Template::from_source(tmpl_str).unwrap_err();
    assert!(
        compile_err
            .to_string()
            .contains("'has()' requires an option type, got str on 'label'"),
        "unexpected compile error: {compile_err}"
    );
}

#[test]
fn option_match_non_exhaustive_missing_none_is_compile_error() {
    let decls = vec![VarDecl {
        name: "opt".to_string(),
        var_type: VarType::Option(Box::new(VarType::Str)),
        default_value: None,
    }];
    let tmpl_str = r#"---
params:
  - opt = option(str)
---

> {% match opt %}
> {% case Some && opt == "special" %}

special: {{ opt }}

> {% case Some %}

has: {{ opt }}

> {% /match %}"#;
    let errors = compile_and_check(tmpl_str, &decls);
    assert_eq!(errors.len(), 1, "expected non-exhaustive error: {errors:?}");
    assert!(
        errors[0].contains("non-exhaustive") && errors[0].contains("None"),
        "unexpected error message: {}",
        errors[0]
    );
}

#[test]
fn option_match_some_or_none_does_not_unsoundly_narrow() {
    let decls = vec![VarDecl {
        name: "opt".to_string(),
        var_type: VarType::Option(Box::new(VarType::Struct(vec![VarDecl {
            name: "title".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }]))),
        default_value: None,
    }];
    let tmpl_str = r"---
params:
  - opt = option(struct(title = str))
---

> {% match opt %}
> {% case Some | None %}

{{ opt.title }}

> {% /match %}";
    let errors = compile_and_check(tmpl_str, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot access field 'title' on option"),
        "unexpected error message: {}",
        errors[0]
    );
}

#[test]
fn case_wildcard_is_rejected_in_match() {
    let tmpl = r"---
params:
  - outcome = enum(Confirmed, NotConfirmed, Pending)
---
> {% match outcome %}
> {% case Confirmed %}

confirmed

> {% case _ %}

fallback

> {% /match %}";
    let err = crate::Template::from_source(tmpl).unwrap_err();
    assert!(
        err.to_string()
            .contains("wildcard '_' in {% case %} is not supported"),
        "unexpected error: {err}"
    );
}

#[test]
fn match_without_else_arm_rejected_when_non_exhaustive() {
    let tmpl = r"---
params:
  - outcome = enum(Confirmed, NotConfirmed, Pending)
---
> {% match outcome %}
> {% case Confirmed %}

confirmed

> {% case NotConfirmed %}

not confirmed

> {% /match %}";
    let err = crate::Template::from_source(tmpl).unwrap_err();
    assert!(
        err.to_string().contains("non-exhaustive"),
        "unexpected error: {err}"
    );
}

#[test]
fn match_with_else_arm_accepted() {
    let tmpl = r"---
params:
  - outcome = enum(Confirmed, NotConfirmed, Pending)
---
> {% match outcome %}
> {% case Confirmed %}

confirmed

> {% else %}

fallback

> {% /match %}";
    assert!(crate::Template::from_source(tmpl).is_ok());
}
