use super::*;
use crate::value::Value;

#[test]
fn from_source_and_render() {
    let tmpl = Template::from_source("---\nparams: [name = str]\n---\nHello {{ name }}!").unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    assert_eq!(tmpl.render(&ctx).unwrap(), "Hello world!");
}

#[test]
fn from_source_with_frontmatter_extracts_metadata() {
    let (tmpl, fm) =
        Template::from_source_with_frontmatter("---\nname: demo\nparams: [a = str]\n---\n{{ a }}")
            .unwrap();
    assert_eq!(fm.name, "demo");
    assert_eq!(fm.declarations[0].name, "a");
    let mut ctx = Context::new();
    ctx.set("a", "val");
    assert_eq!(tmpl.render(&ctx).unwrap(), "val");
}

// -- defaults --

#[test]
fn defaults_returns_default_values() {
    let tmpl = Template::from_source(
        "---\nparams:\n  - name = str\n  - count = int := 42\n  - label = str := \"hello\"\n---\n{{ name }} {{ count }} {{ label }}",
    )
    .unwrap();
    let defaults = tmpl.defaults();
    assert_eq!(defaults.len(), 2);
    assert_eq!(defaults.get("count"), Some(&Value::Int(42)));
    assert_eq!(defaults.get("label"), Some(&Value::Str("hello".into())));
    assert!(!defaults.contains_key("name"), "name has no default");
}

#[test]
fn defaults_empty_when_no_defaults() {
    let tmpl =
        Template::from_source("---\nparams: [name = str, age = int]\n---\n{{ name }} {{ age }}")
            .unwrap();
    assert!(tmpl.defaults().is_empty());
}

#[test]
fn default_single_param() {
    let tmpl = Template::from_source(
        "---\nparams:\n  - name = str\n  - count = int := 7\n---\n{{ name }} {{ count }}",
    )
    .unwrap();
    assert_eq!(tmpl.default("count"), Some(&Value::Int(7)));
    assert_eq!(tmpl.default("name"), None);
    assert_eq!(tmpl.default("nonexistent"), None);
}

#[test]
fn defaults_context_prefills_and_renders() {
    let tmpl = Template::from_source(
        "---\nparams:\n  - name = str\n  - count = int := 5\n---\n{{ name }} ({{ count }})",
    )
    .unwrap();
    let mut ctx = tmpl.defaults_context();
    ctx.set("name", "Alice");
    assert_eq!(tmpl.render(&ctx).unwrap(), "Alice (5)");
}

#[test]
fn defaults_context_overridable() {
    let tmpl = Template::from_source(
        "---\nparams:\n  - name = str\n  - count = int := 5\n---\n{{ name }} ({{ count }})",
    )
    .unwrap();
    let mut ctx = tmpl.defaults_context();
    ctx.set("name", "Bob");
    ctx.set("count", 99_i64);
    assert_eq!(tmpl.render(&ctx).unwrap(), "Bob (99)");
}

#[test]
fn declarations_expose_defaults() {
    let tmpl = Template::from_source(
        "---\nparams:\n  - name = str\n  - count = int := 10\n---\n{{ name }} {{ count }}",
    )
    .unwrap();
    let decls = tmpl.declarations();
    assert_eq!(decls.len(), 2);

    assert_eq!(decls[0].name, "name");
    assert_eq!(decls[0].default_value(), None);

    assert_eq!(decls[1].name, "count");
    assert_eq!(decls[1].default_value(), Some(&Value::Int(10)));
}

#[test]
fn validate_missing_params() {
    let tmpl = Template::from_source(
        "---\nparams: [name = str, count = int]\n---\n{{ name }} {{ count }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    // count is missing
    let err = tmpl.render(&ctx).unwrap_err();
    assert!(matches!(err, TemplateError::MissingParams(_)));
}

#[test]
fn validate_type_mismatch() {
    let tmpl = Template::from_source("---\nparams: [flag = bool]\n---\n{{ flag }}").unwrap();
    let mut ctx = Context::new();
    ctx.set("flag", "not a bool"); // str, not bool
    let err = tmpl.render(&ctx).unwrap_err();
    assert!(matches!(err, TemplateError::TypeMismatch { .. }));
}

#[test]
fn from_file_and_render() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tmpl.md");
    std::fs::write(
        &path,
        "---\nname: test\nparams: [x = str]\n---\nContent {{ x }}",
    )
    .unwrap();
    let tmpl = Template::from_file(&path).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "here");
    assert_eq!(tmpl.render(&ctx).unwrap(), "Content here");
}

#[test]
fn load_template_from_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("greeting.tmpl.md"),
        "---\nname: greeting\nparams: [name = str]\n---\nHello {{ name }}!",
    )
    .unwrap();
    let tmpl = load_template(dir.path(), "greeting").unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    assert_eq!(tmpl.render(&ctx).unwrap(), "Hello world!");
}

#[test]
fn load_template_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let err = load_template(dir.path(), "nonexistent")
        .expect_err("loading nonexistent template should fail");
    assert!(
        err.to_string().contains("No such file") || err.to_string().contains("not found"),
        "should mention file not found: {err}"
    );
}

#[test]
fn from_file_with_frontmatter_extracts_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fm.tmpl.md");
    std::fs::write(
        &path,
        "---\nname: fm\ndescription: Desc\nparams: [x = str, y = str]\n---\n{{ x }} {{ y }}",
    )
    .unwrap();
    let (_tmpl, fm) = Template::from_file_with_frontmatter(&path).unwrap();
    assert_eq!(fm.name, "fm");
    assert_eq!(fm.description, "Desc");
    assert_eq!(fm.params, vec!["x", "y"]);
}

#[test]
fn from_source_with_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    let tmpl = Template::from_source_with_base_dir(
        "---\nparams: [name = str]\n---\nHello {{ name }}!",
        dir.path(),
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "test");
    assert_eq!(tmpl.render(&ctx).unwrap(), "Hello test!");
}

#[test]
fn render_no_frontmatter_errors() {
    let err =
        Template::from_source("Just text").expect_err("template without frontmatter should fail");
    assert!(
        err.to_string().contains("frontmatter"),
        "should mention missing frontmatter: {err}"
    );
}

#[test]
fn render_empty_variables_passes_validation() {
    let tmpl = Template::from_source("---\nparams: []\n---\nOk").unwrap();
    let ctx = Context::new();
    assert_eq!(tmpl.render(&ctx).unwrap(), "Ok");
}

#[test]
fn render_bool_value() {
    let tmpl =
        Template::from_source("---\nparams: [flag = bool]\n---\n> {% if flag %}yes{% /if %}")
            .unwrap();
    let mut ctx = Context::new();
    ctx.set("flag", Value::Bool(true));
    assert_eq!(tmpl.render(&ctx).unwrap(), "yes");
}

// -- validate_declarations -------------------------------------------------

#[test]
fn validate_declarations_identical() {
    let tmpl =
        Template::from_source("---\nparams: [a = str, b = int]\n---\n{{ a }} {{ b }}").unwrap();
    assert!(tmpl.validate_declarations(tmpl.declarations()).is_ok());
}

#[test]
fn validate_declarations_empty_both() {
    let tmpl = Template::from_source("---\nparams: []\n---\nplain").unwrap();
    assert!(tmpl.validate_declarations(&[]).is_ok());
}

#[test]
fn validate_declarations_order_independent() {
    // Template declares [a, b]; expected is [b, a] — should still match.
    let tmpl = Template::from_source("---\nparams: [a = str, b = int]\n---\n{{ a }}{# {{ b }} #}")
        .unwrap();
    let reversed: Vec<_> = tmpl.declarations().iter().rev().cloned().collect();
    assert!(tmpl.validate_declarations(&reversed).is_ok());
}

#[test]
fn validate_declarations_detects_removed() {
    // Template has [a], but expected has [a, b] → b was removed.
    let tmpl = Template::from_source("---\nparams: [a = str]\n---\n{{ a }}").unwrap();
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
    let tmpl = Template::from_source("---\nparams: [a = str, c = bool]\n---\n{{ a }}{# {{ c }} #}")
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
        "---\nparams: [a = str, new_var = int]\n---\n{{ a }}{# {{ new_var }} #}",
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
    let tmpl = Template::from_source("---\nparams: [x = str]\n---\n{{ x }}").unwrap();
    let err = tmpl.validate_declarations(&[]).unwrap_err();
    assert!(matches!(err, TemplateError::DeclarationsMutated { .. }));
}

#[test]
fn validate_declarations_from_nonempty_to_empty() {
    // Template has [], expected has [x] → x was removed.
    let tmpl = Template::from_source("---\nparams: []\n---\nplain").unwrap();
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
    let err =
        Template::from_source("---\nparams: [name = str, extra = str]\n---\nHello {{ name }}!")
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
        "---\nparams: [name = str, extra = str]\n---\nHello {{ name }}!\n{# {{ extra }} #}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("extra", "unused");
    let output = tmpl.render(&ctx).unwrap();
    assert_eq!(output, "Hello world!\n");
}

#[test]
fn all_declared_params_referenced_produces_no_unused() {
    // Directly verify the static analysis: both params should be
    // in the referenced set.
    let tmpl = Template::from_source(
        "---\nparams: [name = str, count = int]\n---\n{{ name }} has {{ count }}",
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
    let err =
        Template::from_source("---\nparams: [name = str, extra = str]\n---\nHello {{ name }}!")
            .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("extra"),
        "'extra' is unused and should cause a parse error: {msg}",
    );
}

#[test]
fn param_in_condition_counts_as_referenced() {
    let tmpl =
        Template::from_source("---\nparams: [show = bool]\n---\n> {% if show %}visible{% /if %}")
            .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    assert!(referenced.contains("show"));
}

#[test]
fn param_in_for_loop_counts_as_referenced() {
    let tmpl = Template::from_source(
        "---\nparams: [items = list<name = str>]\n---\n> {% for item in items %}{{ item.name }}{% /for %}",
    )
    .unwrap();
    let referenced = compiled::collect_referenced_params(&tmpl.segments);
    assert!(referenced.contains("items"));
}

#[test]
fn no_declarations_means_no_unused() {
    // Unused-param check runs at parse time; empty params should parse fine.
    let tmpl = Template::from_source("---\nparams: []\n---\nHello").unwrap();
    assert!(tmpl.declarations().is_empty());
}

#[test]
fn allow_unused_frontmatter_permits_unused_params() {
    let tmpl = Template::from_source(
        "---\nparams: [name = str, extra = str]\nallow_unused: true\n---\nHello {{ name }}!",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("extra", "unused");
    assert_eq!(tmpl.render(&ctx).unwrap(), "Hello world!");
}

#[test]
fn allow_unused_frontmatter_still_rejects_undeclared() {
    let err = Template::from_source(
        "---\nparams: [name = str]\nallow_unused: true\n---\n{{ name }} {{ missing }}",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("missing"),
        "undeclared params must still be rejected: {err}"
    );
}

#[test]
fn from_source_allowing_unused_permits_unused_params() {
    let tmpl = Template::from_source_allowing_unused(
        "---\nparams: [name = str, extra = str]\n---\nHello {{ name }}!",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "world");
    ctx.set("extra", "unused");
    assert_eq!(tmpl.render(&ctx).unwrap(), "Hello world!");
}

#[test]
fn from_source_allowing_unused_still_rejects_undeclared() {
    let err = Template::from_source_allowing_unused(
        "---\nparams: [name = str]\n---\n{{ name }} {{ oops }}",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("oops"),
        "undeclared params must still be rejected: {err}"
    );
}

// -- validate_declarations: type change detection --------------------------

#[test]
fn validate_declarations_detects_type_change() {
    // Template declares [a: str], but expected has [a: int] → type changed.
    let tmpl = Template::from_source("---\nparams: [a = str]\n---\n{{ a }}").unwrap();
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
    let tmpl =
        Template::from_source("---\nparams: [a = str, b = int]\n---\n{{ a }} {{ b }}").unwrap();
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
        "---\nparams: [a = str, new_var = int]\n---\n{{ a }}{# {{ new_var }} #}",
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
    let tmpl = Template::from_source("---\nparams: []\n---\nHello").unwrap();
    assert_eq!(tmpl.render(&Context::new()).unwrap(), "Hello");
}

#[test]
fn from_source_invalid_syntax() {
    // Should error on invalid template syntax, not crash
    let err = Template::from_source("---\nparams: []\n---\nHello {{ unclosed").unwrap_err();
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
    let err =
        Template::from_source("---\nparams: [x = str]\n---\n{{ x | badfilter }}").unwrap_err();
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
    let err =
        Template::from_source("---\nparams: [x = str]\n---\nline1\n{{ x |\nline3").unwrap_err();
    // Should be a syntax error regardless of whether line info was attached.
    assert!(
        err.to_string().contains("syntax error") || err.to_string().contains("unclosed"),
        "should be a syntax error: {err}"
    );
}

#[test]
fn syntax_error_display_includes_line() {
    let syn = crate::error::SyntaxError::new("bad token").at_line(5, "{% bad %}");
    assert_eq!(syn.to_string(), "line 5: bad token\n  --> {% bad %}");
}

// -- type mismatch with path diagnostics ----------------------------------

#[test]
fn type_mismatch_shows_nested_path() {
    let tmpl = Template::from_source(
        "---\nparams: [bugs = list<title = str, severity = int>]\n---\n{{ bugs }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set(
        "bugs",
        vec![crate::value::Value::Dict(std::collections::HashMap::from(
            [
                ("title".into(), crate::value::Value::Str("ok".into())),
                (
                    "severity".into(),
                    crate::value::Value::Str("should be int".into()),
                ),
            ],
        ))],
    );
    let err = tmpl.render(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("severity") || msg.contains("[0]"),
        "error should mention the nested field path: {msg}"
    );
}

// -- has_defaults flag optimization ----------------------------------------

#[test]
fn has_defaults_flag_set_correctly() {
    // No defaults
    let tmpl = Template::from_source("---\nparams: [a = str]\n---\n{{ a }}").unwrap();
    assert!(!tmpl.has_defaults, "no defaults should be false");

    // With defaults
    let tmpl = Template::from_source("---\nparams: [a = str := hello]\n---\n{{ a }}").unwrap();
    assert!(tmpl.has_defaults, "with default should be true");
}

// -- MissingParams is Vec<String> -----------------------------------------

#[test]
fn missing_params_are_vec() {
    let tmpl =
        Template::from_source("---\nparams: [a = str, b = int]\n---\n{{ a }} {{ b }}").unwrap();
    let err = tmpl.render(&Context::new()).unwrap_err();
    if let TemplateError::MissingParams(params) = &err {
        assert!(params.contains(&"a".to_string()));
        assert!(params.contains(&"b".to_string()));
        assert_eq!(params.len(), 2);
    } else {
        panic!("expected MissingParams, got: {err}");
    }
}

// -- render_serde ----------------------------------------------------------

#[cfg(feature = "serde")]
mod serde_tests {
    use super::*;

    #[derive(serde::Serialize)]
    struct FullData {
        name: String,
        count: i64,
    }

    #[derive(serde::Serialize)]
    struct PartialData {
        name: String,
        // `count` is deliberately omitted to test the missing-field path.
    }

    #[test]
    fn render_serde_happy_path() {
        let tmpl = Template::from_source(
            "---\nparams: [name = str, count = int]\n---\n{{ name }} has {{ count }}",
        )
        .unwrap();
        let data = FullData {
            name: "Alice".into(),
            count: 3,
        };
        let result = tmpl.render_serde(&data).unwrap();
        assert_eq!(result, "Alice has 3");
    }

    #[test]
    fn render_serde_missing_field_errors() {
        // Template declares `count: int` but `PartialData` has no `count`.
        let tmpl = Template::from_source(
            "---\nparams: [name = str, count = int]\n---\n{{ name }} has {{ count }}",
        )
        .unwrap();
        let data = PartialData {
            name: "Alice".into(),
        };
        let err = tmpl.render_serde(&data).unwrap_err();
        assert!(
            matches!(err, TemplateError::MissingParams(_)),
            "expected MissingParams, got: {err}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("count"),
            "error should mention missing 'count': {msg}"
        );
    }

    #[test]
    fn render_serde_no_declarations_missing_field_still_errors() {
        // Under mandatory validation, parsing without frontmatter or with undeclared variables errors out.
        Template::from_source("{{ name }} has {{ count }}")
            .expect_err("template without frontmatter should fail");
    }
}

// -- end-to-end hot-reload tests ------------------------------------------

mod hot_reload {
    use std::{collections::HashMap, fs};

    use super::*;

    #[test]
    fn reload_same_vars_renders_new_body() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("greeting.tmpl.md");

        // Initial version.
        fs::write(&path, "---\nparams: [name = str]\n---\nHello {{ name }}!").unwrap();
        let original = Template::from_file(&path).unwrap();
        let expected_decls = original.declarations().to_vec();

        let mut ctx = Context::new();
        ctx.set("name", "World");
        assert_eq!(original.render(&ctx).unwrap(), "Hello World!");

        // Modify ONLY the body on disk (vars unchanged).
        fs::write(&path, "---\nparams: [name = str]\n---\nGoodbye {{ name }}!").unwrap();
        let reloaded = Template::from_file(&path).unwrap();

        // Validation passes — same vars.
        reloaded.validate_declarations(&expected_decls).unwrap();

        // Renders the NEW body.
        assert_eq!(reloaded.render(&ctx).unwrap(), "Goodbye World!");
    }

    #[test]
    fn reload_mutated_vars_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("greeting.tmpl.md");

        // Initial.
        fs::write(&path, "---\nparams: [name = str]\n---\nHi {{ name }}!").unwrap();
        let original = Template::from_file(&path).unwrap();
        let expected_decls = original.declarations().to_vec();

        // Agent edits the vars — adds `count`.
        fs::write(
            &path,
            "---\nparams: [name = str, count = int]\n---\nHi {{ name }} x{{ count }}!",
        )
        .unwrap();
        let reloaded = Template::from_file(&path).unwrap();

        let err = reloaded.validate_declarations(&expected_decls).unwrap_err();
        assert!(
            matches!(err, TemplateError::DeclarationsMutated { .. }),
            "expected DeclarationsMutated, got: {err}"
        );
    }

    #[test]
    fn reload_removed_var_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.tmpl.md");

        // Initial: two vars.
        fs::write(
            &path,
            "---\nparams: [title = str, body = str]\n---\n# {{ title }}\n{{ body }}",
        )
        .unwrap();
        let original = Template::from_file(&path).unwrap();
        let expected_decls = original.declarations().to_vec();

        // Agent removes `body`.
        fs::write(&path, "---\nparams: [title = str]\n---\n# {{ title }}").unwrap();
        let reloaded = Template::from_file(&path).unwrap();

        let err = reloaded.validate_declarations(&expected_decls).unwrap_err();
        assert!(
            matches!(err, TemplateError::DeclarationsMutated { .. }),
            "expected DeclarationsMutated, got: {err}"
        );
        assert!(
            err.to_string().contains("body"),
            "should mention removed var 'body': {err}"
        );
    }

    #[test]
    fn reload_retyped_var_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("score.tmpl.md");

        // Initial: count as int.
        fs::write(&path, "---\nparams: [count = int]\n---\nCount: {{ count }}").unwrap();
        let original = Template::from_file(&path).unwrap();
        let expected_decls = original.declarations().to_vec();

        // Agent changes type from int to str.
        fs::write(&path, "---\nparams: [count = str]\n---\nCount: {{ count }}").unwrap();
        let reloaded = Template::from_file(&path).unwrap();

        let err = reloaded.validate_declarations(&expected_decls).unwrap_err();
        assert!(
            matches!(err, TemplateError::DeclarationsMutated { .. }),
            "expected DeclarationsMutated, got: {err}"
        );
        assert!(
            err.to_string().contains("retyped"),
            "should mention retyped: {err}"
        );
    }

    #[test]
    fn precompiled_from_source_matches_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("example.tmpl.md");
        let source = "---\nparams: [name = str]\n---\nHello {{ name }}!";
        fs::write(&path, source).unwrap();

        let from_file = Template::from_file(&path).unwrap();
        let from_source = Template::from_source(source).unwrap();

        let mut ctx = Context::new();
        ctx.set("name", "test");

        let file_output = from_file.render(&ctx).unwrap();
        let source_output = from_source.render(&ctx).unwrap();
        assert_eq!(
            file_output, source_output,
            "from_file and from_source should produce identical output"
        );
    }

    #[test]
    fn precompiled_renders_with_all_features() {
        // Pre-compile a template that uses most features:
        // for loop, idx(), conditionals, filters.
        let tmpl = Template::from_source(
            "---\nparams: [items = list<name = str, active = bool>]\n---\n\
             > {% for item in items %}{{ idx(item) }}. {% if item.active %}[✓]{% else %}[ ]{% /if %} {{ item.name | upper }}\n> {% /for %}",
        ).unwrap();

        let mut ctx = Context::new();
        ctx.set(
            "items",
            Value::List(vec![
                Value::Dict(HashMap::from([
                    ("name".into(), Value::Str("alpha".into())),
                    ("active".into(), Value::Bool(true)),
                ])),
                Value::Dict(HashMap::from([
                    ("name".into(), Value::Str("beta".into())),
                    ("active".into(), Value::Bool(false)),
                ])),
            ]),
        );

        let output = tmpl.render(&ctx).unwrap();
        assert_eq!(output, "0. [✓] ALPHA\n1. [ ] BETA\n");
    }
}

// -- source_hash tests --

mod source_hash {
    use super::*;

    #[test]
    fn same_source_same_hash() {
        let src = "---\nparams: [name = str]\n---\nHello {{ name }}!";
        let t1 = Template::from_source(src).unwrap();
        let t2 = Template::from_source(src).unwrap();
        assert_eq!(t1.source_hash(), t2.source_hash());
    }

    #[test]
    fn different_source_different_hash() {
        let t1 =
            Template::from_source("---\nparams: [name = str]\n---\nHello {{ name }}!").unwrap();
        let t2 =
            Template::from_source("---\nparams: [name = str]\n---\nGoodbye {{ name }}!").unwrap();
        assert_ne!(t1.source_hash(), t2.source_hash());
    }

    #[test]
    fn hash_changes_with_frontmatter() {
        let src1 = "---\nparams: [x = str]\n---\n{{ x }}";
        let src2 = "---\nparams: [y = str]\n---\n{{ y }}";
        let t1 = Template::from_source(src1).unwrap();
        let t2 = Template::from_source(src2).unwrap();
        assert_ne!(t1.source_hash(), t2.source_hash());
    }
}

#[cfg(feature = "serde")]
mod enum_integration {
    use serde::Serialize;

    use super::*;

    const ENUM_TEMPLATE: &str = "\
---
params:
  - severity = enum<Critical(reason = str), High, Low>
---
> {% match severity %}
> {% case Critical %}
CRITICAL: {{ severity.reason }}
> {% case High %}
HIGH
> {% case Low %}
LOW
> {% /match %}";

    #[derive(Serialize)]
    enum Severity {
        Critical { reason: String },
        High,
        Low,
    }

    #[test]
    fn to_value_struct_variant_renders_with_match() {
        let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
        let val = crate::to_value(&Severity::Critical {
            reason: "RCE".into(),
        })
        .unwrap();
        let mut ctx = Context::new();
        ctx.set("severity", val);
        assert_eq!(tmpl.render(&ctx).unwrap(), "CRITICAL: RCE\n");
    }

    #[test]
    fn to_value_unit_variant_renders_with_match() {
        let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
        let val = crate::to_value(&Severity::High).unwrap();
        let mut ctx = Context::new();
        ctx.set("severity", val);
        assert_eq!(tmpl.render(&ctx).unwrap(), "HIGH\n");
    }

    #[test]
    fn ctx_set_with_to_value() {
        let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
        let mut ctx = Context::new();
        ctx.set("severity", crate::to_value(&Severity::Low).unwrap());
        assert_eq!(tmpl.render(&ctx).unwrap(), "LOW\n");
    }

    #[test]
    fn ctx_macro_with_dict_for_struct_variant() {
        let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
        let ctx = crate::ctx! {
            severity: { __kind__: "Critical", reason: "buffer overflow" }
        };
        assert_eq!(tmpl.render(&ctx).unwrap(), "CRITICAL: buffer overflow\n");
    }

    #[test]
    fn ctx_macro_with_string_for_unit_variant() {
        let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
        let ctx = crate::ctx! { severity: "High" };
        assert_eq!(tmpl.render(&ctx).unwrap(), "HIGH\n");
    }

    #[test]
    fn manual_context_set_unit_variant() {
        let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
        let mut ctx = Context::new();
        ctx.set("severity", "Low");
        assert_eq!(tmpl.render(&ctx).unwrap(), "LOW\n");
    }

    #[test]
    fn manual_context_set_struct_variant() {
        let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();
        let mut ctx = Context::new();
        ctx.set(
            "severity",
            Value::dict([
                (crate::consts::ENUM_TAG_KEY, Value::Str("Critical".into())),
                ("reason", Value::Str("use-after-free".into())),
            ]),
        );
        assert_eq!(tmpl.render(&ctx).unwrap(), "CRITICAL: use-after-free\n");
    }

    #[test]
    fn to_value_struct_variant_with_serde_tag_attr() {
        // Macro-generated enums use #[serde(tag = "__kind__")] — verify this
        // still works (serde routes through serialize_struct, not
        // serialize_struct_variant).
        #[derive(Serialize)]
        #[serde(tag = "__kind__")]
        enum TaggedSeverity {
            Critical { reason: String },
            High,
        }

        let tmpl = Template::from_source(ENUM_TEMPLATE).unwrap();

        let val = crate::to_value(&TaggedSeverity::Critical {
            reason: "overflow".into(),
        })
        .unwrap();
        let mut ctx = Context::new();
        ctx.set("severity", val);
        assert_eq!(tmpl.render(&ctx).unwrap(), "CRITICAL: overflow\n");

        // Unit variant with #[serde(tag)] produces {"__kind__": "High"} dict
        let val = crate::to_value(&TaggedSeverity::High).unwrap();
        let mut ctx = Context::new();
        ctx.set("severity", val);
        assert_eq!(tmpl.render(&ctx).unwrap(), "HIGH\n");
    }

    #[test]
    fn multi_field_struct_variant() {
        #[derive(Serialize)]
        enum Result {
            Ok { msg: String, code: i64 },
            Err,
        }

        let tmpl = Template::from_source(
            "---\n\
             params:\n  - r = enum<Ok(msg = str, code = int), Err>\n\
             ---\n\
             > {% match r %}\n\
             > {% case Ok %}\n\
             {{ r.msg }} ({{ r.code }})\n\
             > {% case Err %}\n\
             ERROR\n\
             > {% /match %}",
        )
        .unwrap();

        let val = crate::to_value(&Result::Ok {
            msg: "success".into(),
            code: 200,
        })
        .unwrap();
        let mut ctx = Context::new();
        ctx.set("r", val);
        assert_eq!(tmpl.render(&ctx).unwrap(), "success (200)\n");

        // Also exercise the Err variant so it's genuinely used.
        let err_val = crate::to_value(&Result::Err).unwrap();
        let mut err_ctx = Context::new();
        err_ctx.set("r", err_val);
        assert_eq!(tmpl.render(&err_ctx).unwrap(), "ERROR\n");
    }
}

// -- Rule 11: import stem vs inline template name collision ----------------

#[test]
fn import_stem_conflicts_with_inline_template_name() {
    // If imports: has stem "helper" and there's a {% tmpl helper %} inline,
    // that's an error since they share the same namespace.
    let source = "---\nimports: [[helper](helper.tmpl.md)]\nparams: [x = str]\nallow_unused: true\n---\n> {% tmpl helper %}\n---\nparams: []\n---\ninner\n> {% /tmpl %}\n{{ x }}";
    let err = Template::from_source(source).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("import stem") && msg.contains("conflicts with inline template"),
        "Expected import stem collision error, got: {msg}"
    );
}

// -- Rule 12: param/const name vs inline template name collision -----------

#[test]
fn param_name_conflicts_with_inline_template_name() {
    // A declared param with the same name as an inline template is ambiguous.
    let source = concat!(
        "---\n",
        "params: [helper = str]\n",
        "---\n",
        "> {% tmpl helper %}\n",
        "---\n",
        "params: []\n",
        "---\n",
        "inner\n",
        "> {% /tmpl %}\n",
        "{{ helper }}\n",
    );
    let err = Template::from_source(source).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("inline template name 'helper'")
            && msg.contains("conflicts with a declared parameter or constant"),
        "Expected param/tmpl collision error, got: {msg}"
    );
}

#[test]
fn const_name_conflicts_with_inline_template_name() {
    // A declared const with the same name as an inline template is ambiguous.
    let source = concat!(
        "---\n",
        "consts:\n",
        "  - helper = str := \"value\"\n",
        "---\n",
        "> {% tmpl helper %}\n",
        "---\n",
        "params: []\n",
        "---\n",
        "inner\n",
        "> {% /tmpl %}\n",
        "{{ helper }}\n",
    );
    let err = Template::from_source(source).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("inline template name 'helper'")
            && msg.contains("conflicts with a declared parameter or constant"),
        "Expected const/tmpl collision error, got: {msg}"
    );
}

// -- Rule 3: param name vs const name collision ---------------------------

#[test]
fn param_name_conflicts_with_const_name() {
    let err = Template::from_source(
        "---\nparams:\n  - x = str\nconsts:\n  - x = str := \"hi\"\n---\n{{ x }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("parameter name conflicts with constant name"),
        "Expected param/const collision error, got: {msg}"
    );
}

#[test]
fn const_name_conflicts_with_param_name() {
    // Order reversed — const first, param second.
    let err = Template::from_source(
        "---\nconsts:\n  - x = str := \"hi\"\nparams:\n  - x = str\n---\n{{ x }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("parameter name conflicts with constant name"),
        "Expected const/param collision error, got: {msg}"
    );
}

#[test]
fn param_and_const_different_names_ok() {
    let tmpl = Template::from_source(
        "---\nparams:\n  - x = str\nconsts:\n  - Y = int := 42\n---\n{{ x }} {{ Y }}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "hello");
    assert_eq!(tmpl.render(&ctx).unwrap(), "hello 42");
}

// -- Rule 13: for-loop binding must not shadow declared names -------------

#[test]
fn for_binding_shadows_param_rejected() {
    let err = Template::from_source(
        "---\nparams:\n  - items = list<name = str>\n  - x = str\n---\n\
         > {% for x in items %}{{ x.name }}\n> {% /for %}\n{{ x }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'x'"),
        "Expected for-binding shadow error, got: {msg}"
    );
}

#[test]
fn for_binding_shadows_const_rejected() {
    let err = Template::from_source(
        "---\nconsts:\n  - x = str := \"hi\"\nparams:\n  - items = list<name = str>\n---\n\
         > {% for x in items %}{{ x.name }}\n> {% /for %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'x'"),
        "Expected for-binding shadow const error, got: {msg}"
    );
}

#[test]
fn for_binding_shadows_import_rejected() {
    let err = Template::from_source(
        "---\nimports:\n  - \"[shared](shared.tmpl.md)\"\nparams:\n  - items = list<name = str>\nallow_unused: true\n---\n\
         > {% for shared in items %}{{ shared.name }}\n> {% /for %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'shared'"),
        "Expected for-binding shadow import error, got: {msg}"
    );
}

#[test]
fn for_binding_shadows_inline_tmpl_rejected() {
    let err = Template::from_source(concat!(
        "---\n",
        "params: [items = list<name = str>]\n",
        "allow_unused: true\n",
        "---\n",
        "> {% tmpl greeting %}\n",
        "---\nparams: [name = str]\n---\n",
        "hi {{ name }}\n",
        "> {% /tmpl %}\n",
        "> {% for greeting in items %}{{ greeting.name }}\n",
        "> {% /for %}",
    ))
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'greeting'"),
        "Expected for-binding shadow tmpl error, got: {msg}"
    );
}

#[test]
fn for_binding_in_nested_if_shadows_param_rejected() {
    let err = Template::from_source(
        "---\nparams:\n  - items = list<name = str>\n  - x = str\n  - show = bool\n---\n\
         > {% if show %}\n\
         > {% for x in items %}{{ x.name }}\n\
         > {% /for %}\n\
         > {% /if %}\n{{ x }}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'x'"),
        "Expected nested for-binding shadow error, got: {msg}"
    );
}

// -- for-loop binding reuse is ALLOWED ------------------------------------

#[test]
fn sequential_for_loops_same_binding_allowed() {
    let tmpl = Template::from_source(
        "---\nparams:\n  - items = list<name = str>\nallow_unused: true\n---\n\
         > {% for x in items %}{{ x.name }}\n> {% /for %}\n\
         > {% for x in items %}{{ x.name }}\n> {% /for %}",
    )
    .unwrap();
    let ctx = crate::ctx! {
        items: [{ name: "a" }, { name: "b" }]
    };
    let output = tmpl.render(&ctx).unwrap();
    assert!(output.contains('a') && output.contains('b'));
}

#[test]
fn fresh_for_binding_allowed() {
    // A binding name that doesn't conflict with any declared name is fine.
    let tmpl = Template::from_source(
        "---\nparams:\n  - items = list<name = str>\nallow_unused: true\n---\n\
         > {% for item in items %}{{ item.name }}\n> {% /for %}",
    )
    .unwrap();
    let ctx = crate::ctx! { items: [{ name: "hello" }] };
    assert!(tmpl.render(&ctx).unwrap().contains("hello"));
}

#[test]
fn nested_for_loops_different_bindings_allowed() {
    let tmpl = Template::from_source(concat!(
        "---\n",
        "params: [items = list<children = list<name = str>>]\n",
        "allow_unused: true\n",
        "---\n",
        "> {% for item in items %}\n",
        ">   {% for child in item.children %}{{ child.name }}\n",
        ">   {% /for %}\n",
        "> {% /for %}",
    ))
    .unwrap();
    let ctx = crate::ctx! {
        items: [{ children: [{ name: "leaf" }] }]
    };
    assert!(tmpl.render(&ctx).unwrap().contains("leaf"));
}

// -- Blockquote prefix enforcement ----------------------------------------

#[test]
fn bare_stmt_tag_at_line_start_rejected() {
    let err = Template::from_source(
        "---\nparams: [x = str]\nallow_unused: true\n---\n{% if x %}yes\n> {% /if %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote-prefixed"),
        "Expected blockquote error, got: {msg}"
    );
}

#[test]
fn bare_for_tag_at_line_start_rejected() {
    let err = Template::from_source(
        "---\nparams: [items = list<name = str>]\nallow_unused: true\n---\n{% for item in items %}{{ item.name }}\n> {% /for %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote-prefixed"),
        "Expected blockquote error, got: {msg}"
    );
}

#[test]
fn indented_bare_stmt_tag_rejected() {
    let err = Template::from_source(
        "---\nparams: [x = str]\nallow_unused: true\n---\n  {% if x %}yes\n> {% /if %}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("blockquote-prefixed"),
        "Expected blockquote error for indented tag, got: {msg}"
    );
}

#[test]
fn midline_stmt_tag_allowed_without_prefix() {
    // {% %} tags in the middle of a line don't need >
    let tmpl = Template::from_source(
        "---\nparams: [x = str]\n---\ntext: {{ x }}{% match x case \"a\" %} is a{% /match %}",
    );
    // This should at least not fail on blockquote validation
    // (it may have other errors depending on x's type, but NOT a blockquote error)
    if let Err(e) = &tmpl {
        assert!(
            !e.to_string().contains("blockquote-prefixed"),
            "Mid-line tag should not require blockquote"
        );
    }
}

#[test]
fn expression_tag_at_line_start_allowed() {
    // {{ }} never needs >
    let tmpl = Template::from_source("---\nparams: [name = str]\n---\n{{ name }}").unwrap();
    let mut ctx = Context::new();
    ctx.set("name", "hello");
    assert_eq!(tmpl.render(&ctx).unwrap(), "hello");
}

#[test]
fn blockquote_prefixed_stmt_tag_works() {
    let tmpl = Template::from_source(
        "---\nparams: [x = bool]\n---\n> {% if x %}yes\n> {% else %}no\n> {% /if %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set("x", true);
    assert_eq!(tmpl.render(&ctx).unwrap(), "yes\n");
}
