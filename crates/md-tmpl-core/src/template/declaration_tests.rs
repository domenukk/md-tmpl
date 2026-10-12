//! Declaration validation, unused-variable, and diagnostic tests.

use std::sync::Arc;

use super::*;

// -- validate_declarations -------------------------------------------------

#[test]
fn validate_declarations_identical() {
    let tmpl = Template::from_source(
        "---
params: [a = str, b = int]
---
{{ a }} {{ b }}",
    )
    .unwrap();
    assert!(tmpl.validate_declarations(tmpl.declarations()).is_ok());
}

#[test]
fn validate_declarations_empty_both() {
    let tmpl = Template::from_source(
        "---
params: []
---
plain",
    )
    .unwrap();
    assert!(tmpl.validate_declarations(&[]).is_ok());
}

#[test]
fn validate_declarations_order_independent() {
    // Template declares [a, b]; expected is [b, a] — should still match.
    let tmpl = Template::from_source(
        "---
params: [a = str, b = int]
---
{{ a }}{# {{ b }} #}",
    )
    .unwrap();
    let reversed: Vec<_> = tmpl.declarations().iter().rev().cloned().collect();
    assert!(tmpl.validate_declarations(&reversed).is_ok());
}

#[test]
fn validate_declarations_detects_removed() {
    // Template has [a], but expected has [a, b] → b was removed.
    let tmpl = Template::from_source(
        "---
params: [a = str]
---
{{ a }}",
    )
    .unwrap();
    let expected = vec![
        VarDecl {
            name: "a".into(),
            var_type: crate::types::VarType::Str,
            default_value: None,
        },
        VarDecl {
            name: "b".into(),
            var_type: crate::types::VarType::Int,
            default_value: None,
        },
    ];
    let err = tmpl.validate_declarations(&expected).unwrap_err();
    assert!(matches!(err, TemplateError::DeclarationsMutated { .. }));
    let msg = err.to_string();
    assert!(msg.contains('b'), "should mention removed var: {msg}");
    assert!(
        msg.contains("must not be changed"),
        "should explain immutability: {msg}"
    );
}

#[test]
fn validate_declarations_detects_added() {
    // Template has [a, c], but expected only has [a] → c was added.
    let tmpl = Template::from_source(
        "---
params: [a = str, c = bool]
---
{{ a }}{# {{ c }} #}",
    )
    .unwrap();
    let expected = vec![VarDecl {
        name: "a".into(),
        var_type: crate::types::VarType::Str,
        default_value: None,
    }];
    let err = tmpl.validate_declarations(&expected).unwrap_err();
    assert!(matches!(err, TemplateError::DeclarationsMutated { .. }));
    let msg = err.to_string();
    assert!(msg.contains('c'), "should mention added var: {msg}");
}

#[test]
fn validate_declarations_detects_both_added_and_removed() {
    // Template has [a, new_var], expected has [a, old_var].
    let tmpl = Template::from_source(
        "---
params: [a = str, new_var = int]
---
{{ a }}{# {{ new_var }} #}",
    )
    .unwrap();
    let expected = vec![
        VarDecl {
            name: "a".into(),
            var_type: crate::types::VarType::Str,
            default_value: None,
        },
        VarDecl {
            name: "old_var".into(),
            var_type: crate::types::VarType::Int,
            default_value: None,
        },
    ];
    let err = tmpl.validate_declarations(&expected).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("old_var"), "should mention removed var: {msg}");
    assert!(msg.contains("new_var"), "should mention added var: {msg}");
}

#[test]
fn validate_declarations_from_empty_to_nonempty() {
    // Template has [x], expected is [] → x was added.
    let tmpl = Template::from_source(
        "---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();
    let err = tmpl.validate_declarations(&[]).unwrap_err();
    assert!(matches!(err, TemplateError::DeclarationsMutated { .. }));
}

#[test]
fn validate_declarations_from_nonempty_to_empty() {
    // Template has [], expected has [x] → x was removed.
    let tmpl = Template::from_source(
        "---
params: []
---
plain",
    )
    .unwrap();
    let expected = vec![VarDecl {
        name: "x".into(),
        var_type: crate::types::VarType::Str,
        default_value: None,
    }];
    let err = tmpl.validate_declarations(&expected).unwrap_err();
    assert!(matches!(err, TemplateError::DeclarationsMutated { .. }));
}

// -- unused variable detection (non-fatal) ---------------------------------

#[test]
fn unused_declared_variable_is_hard_error() {
    // `extra` is declared but never referenced in the body — parsing
    // must fail with a syntax error about unused parameters.
    let err = Template::from_source(
        "---
params: [name = str, extra = str]
---
Hello {{ name }}!",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("unused declared parameter"),
        "expected unused parameter error, got: {err}"
    );
    assert!(
        err.to_string().contains("extra"),
        "should mention the unused parameter name: {err}"
    );
}

#[test]
fn comment_reference_suppresses_unused_error() {
    // `extra` is declared and referenced in a {# comment #} — rendering
    // must succeed because comments count as variable usage.
    let tmpl = Template::from_source(
        "---
params: [name = str, extra = str]
---
Hello {{ name }}!

> {# {{ extra }} #}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("extra", "unused");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        output,
        "Hello world!
"
    );
}

#[test]
fn all_declared_params_referenced_produces_no_unused() {
    // Directly verify the static analysis: both params should be
    // in the referenced set.
    let tmpl = Template::from_source(
        "---
params: [name = str, count = int]
---
{{ name }} has {{ count }}",
    )
    .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    for decl in tmpl.declarations() {
        assert!(
            referenced.contains(decl.name.as_str()),
            "declared param '{}' should be referenced, refs = {referenced:?}",
            decl.name,
        );
    }
}

#[test]
fn unused_param_detected_by_static_analysis() {
    // `extra` is declared but never in the body — verify parse-time rejection.
    let err = Template::from_source(
        "---
params: [name = str, extra = str]
---
Hello {{ name }}!",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("extra"),
        "'extra' is unused and should cause a parse error: {msg}",
    );
}

#[test]
fn param_in_condition_counts_as_referenced() {
    let tmpl = Template::from_source(
        "---
params: [show = bool]
---
> {% if show %}visible{% /if %}",
    )
    .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    assert!(referenced.contains("show"));
}

#[test]
fn param_in_for_loop_counts_as_referenced() {
    let tmpl = Template::from_source(
        "---
params: [items = list(name = str)]
---
> {% for item in items %}{{ item.name }}{% /for %}",
    )
    .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    assert!(referenced.contains("items"));
}

#[test]
fn no_declarations_means_no_unused() {
    // Unused-param check runs at parse time; empty params should parse fine.
    let tmpl = Template::from_source(
        "---
params: []
---
Hello",
    )
    .unwrap();
    assert!(tmpl.declarations().is_empty());
}

#[test]
fn allow_unused_frontmatter_permits_unused_params() {
    let tmpl = Template::from_source(
        "---
params: [name = str, extra = str]
allow_unused: true
---
Hello {{ name }}!",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("extra", "unused");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Hello world!");
}

#[test]
fn allow_unused_frontmatter_still_rejects_undeclared() {
    let err = Template::from_source(
        "---
params: [name = str]
allow_unused: true
---
{{ name }} {{ missing }}",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("missing"),
        "undeclared params must still be rejected: {err}"
    );
}

#[test]
fn from_source_allowing_unused_permits_unused_params() {
    let (tmpl, _fm) = Template::compile(
        "---
params: [name = str, extra = str]
---
Hello {{ name }}!",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("extra", "unused");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Hello world!");
}

#[test]
fn from_source_allowing_unused_still_rejects_undeclared() {
    let err = Template::compile(
        "---
params: [name = str]
---
{{ name }} {{ oops }}",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("oops"),
        "undeclared params must still be rejected: {err}"
    );
}

// -- validate_declarations: type change detection --------------------------

#[test]
fn empty_frontmatter_block_compiles_and_renders() {
    // A present-but-empty frontmatter block declares no params (it does not
    // set `has_params`); it used to be misreported as an unclosed block.
    let source = r"---
---
# Plain
";
    let (tmpl, fm) = Template::compile(source, CompileOptions::default()).unwrap();
    assert!(fm.declarations.is_empty());
    assert_eq!(tmpl.render_empty().unwrap().trim(), "# Plain");
}

#[test]
fn validate_declarations_detects_type_change() {
    // Template declares [a: str], but expected has [a: int] → type changed.
    let tmpl = Template::from_source(
        "---
params: [a = str]
---
{{ a }}",
    )
    .unwrap();
    let expected = vec![VarDecl {
        name: "a".into(),
        var_type: crate::types::VarType::Int,
        default_value: None,
    }];
    let err = tmpl.validate_declarations(&expected).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("retyped"),
        "error should mention retyped: {msg}"
    );
    assert!(msg.contains('a'), "should name the variable: {msg}");
}

#[test]
fn validate_declarations_type_unchanged_passes() {
    // Same names AND same types → should pass.
    let tmpl = Template::from_source(
        "---
params: [a = str, b = int]
---
{{ a }} {{ b }}",
    )
    .unwrap();
    let expected = vec![
        VarDecl {
            name: "a".into(),
            var_type: crate::types::VarType::Str,
            default_value: None,
        },
        VarDecl {
            name: "b".into(),
            var_type: crate::types::VarType::Int,
            default_value: None,
        },
    ];
    assert!(tmpl.validate_declarations(&expected).is_ok());
}

#[test]
fn validate_declarations_detects_type_change_and_name_change() {
    // Template: [a: str, new_var: int], expected: [a: int, old_var: int].
    // `a` is retyped, `old_var` removed, `new_var` added.
    let tmpl = Template::from_source(
        "---
params: [a = str, new_var = int]
---
{{ a }}{# {{ new_var }} #}",
    )
    .unwrap();
    let expected = vec![
        VarDecl {
            name: "a".into(),
            var_type: crate::types::VarType::Int,
            default_value: None,
        },
        VarDecl {
            name: "old_var".into(),
            var_type: crate::types::VarType::Int,
            default_value: None,
        },
    ];
    let err = tmpl.validate_declarations(&expected).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("old_var"), "should mention removed: {msg}");
    assert!(msg.contains("new_var"), "should mention added: {msg}");
    assert!(msg.contains("retyped"), "should mention retyped: {msg}");
}

// -- from_source fallibility -----------------------------------------------

#[test]
fn from_source_valid() {
    let tmpl = Template::from_source(
        "---
params: []
---
Hello",
    )
    .unwrap();
    assert_eq!(tmpl.render_ctx(&Context::new()).unwrap(), "Hello");
}

#[test]
fn from_source_invalid_syntax() {
    // Should error on invalid template syntax, not crash
    let err = Template::from_source(
        "---
params: []
---
Hello {{ unclosed",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("template syntax error"),
        "error should suggest correct syntax: {err}"
    );
}

#[test]
fn from_source_unclosed_block() {
    let err = Template::from_source("{% for x in items %}body")
        .expect_err("unclosed for block should fail");
    assert!(
        err.to_string().contains("frontmatter") || err.to_string().contains("unclosed"),
        "should mention missing frontmatter or unclosed block: {err}"
    );
}

// -- structured SyntaxError ------------------------------------------------

#[test]
fn syntax_error_has_line_number() {
    // A bad filter reference triggers an UnknownFilter which enrich_error
    // converts into a Syntax with structured line info.
    let err = Template::from_source(
        "---
params: [x = str]
---
{{ x | badfilter }}",
    )
    .unwrap_err();
    if let TemplateError::Syntax(syn) = &err {
        assert!(
            syn.line.is_some(),
            "syntax error from bad filter should have line number: {err}"
        );
        assert!(
            syn.message.contains("badfilter"),
            "message: {}",
            syn.message
        );
    } else {
        panic!("expected Syntax error, got: {err}");
    }
}

#[test]
fn syntax_error_enriched_with_line() {
    // An unclosed expression triggers enrich_error with line context.
    let err = Template::from_source(
        "---
params: [x = str]
---
line1
{{ x |
line3",
    )
    .unwrap_err();
    // Should be a syntax error regardless of whether line info was attached.
    assert!(
        err.to_string().contains("syntax error") || err.to_string().contains("unclosed"),
        "should be a syntax error: {err}"
    );
}

#[test]
fn syntax_error_display_includes_line() {
    let syn = crate::error::SyntaxError::new("bad token").at_line(5, "{% bad %}");
    assert_eq!(
        syn.to_string(),
        "line 5: bad token
  --> {% bad %}"
    );
}

// -- type mismatch with path diagnostics ----------------------------------

#[test]
fn type_mismatch_shows_nested_path() {
    let tmpl = Template::from_source(
        "---
params: [tasks = list(title = str, priority = int)]
---
> {% for task in tasks %}{{ task.title }}{{ task.priority }}{% /for %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set(
        "tasks",
        vec![crate::value::Value::Struct(Arc::new(
            crate::compat::HashMap::from([
                ("title".into(), crate::value::Value::Str("ok".into())),
                (
                    "priority".into(),
                    crate::value::Value::Str("should be int".into()),
                ),
            ]),
        ))],
    );
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("priority") || msg.contains("[0]"),
        "error should mention the nested field path: {msg}"
    );
}

// -- has_defaults flag optimization ----------------------------------------

#[test]
fn has_defaults_flag_set_correctly() {
    // No defaults
    let tmpl = Template::from_source(
        "---
params: [a = str]
---
{{ a }}",
    )
    .unwrap();
    assert!(!tmpl.has_defaults, "no defaults should be false");

    // With defaults
    let tmpl = Template::from_source(
        r#"---
params: [a = str := "hello"]
---
{{ a }}"#,
    )
    .unwrap();
    assert!(tmpl.has_defaults, "with default should be true");
}

// -- MissingParams is Vec<String> -----------------------------------------

#[test]
fn missing_params_are_vec() {
    let tmpl = Template::from_source(
        "---
params: [a = str, b = int]
---
{{ a }} {{ b }}",
    )
    .unwrap();
    let err = tmpl.render_ctx(&Context::new()).unwrap_err();
    if let TemplateError::MissingParams(params) = &err {
        assert!(params.contains(&"a".to_string()));
        assert!(params.contains(&"b".to_string()));
        assert_eq!(params.len(), 2);
    } else {
        panic!("expected MissingParams, got: {err}");
    }
}
