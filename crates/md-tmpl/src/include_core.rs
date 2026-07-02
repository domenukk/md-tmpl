//! Core include helpers that do NOT depend on `std`.
//!
//! These are used by both the full `include.rs` (under `std`) and by
//! `render_include_no_std` (under `no_std`) to validate and render
//! includes backed by `Value::Tmpl` parameters or inline templates.

use alloc::string::{String, ToString};

use crate::{
    compat::HashMap, error::TemplateError, parser::IncludeDirective, scope::Scope, types::VarDecl,
    value::Value,
};

/// Validate that all variables declared by an included template are explicitly
/// provided via `with` overrides or `for` bindings.
///
/// Uses the shared `find_missing_include_params` from the type checker to
/// avoid duplicating the contract-checking logic.
pub(crate) fn validate_include_contract(
    declarations: &[VarDecl],
    directive: &IncludeDirective<'_>,
) -> Result<(), TemplateError> {
    if declarations.is_empty() {
        return Ok(());
    }

    let provided = directive
        .with_vars
        .iter()
        .map(|&(key, _)| key)
        .chain(directive.for_each.as_ref().map(|&(b, _)| b));

    let missing = crate::compiled::type_check::find_missing_include_params(declarations, provided);
    if missing.is_empty() {
        return Ok(());
    }

    // Single-pass: collect descriptions and fix hints together.
    let (descs, hints): (alloc::vec::Vec<_>, alloc::vec::Vec<_>) = missing
        .iter()
        .map(|d| {
            (
                alloc::format!("{}: {}", d.name, d.var_type),
                alloc::format!("{}={}", d.name, d.name),
            )
        })
        .unzip();

    Err(TemplateError::syntax(alloc::format!(
        "include '{}' requires explicit parameters: {}. \
         Use 'with {}' to pass them",
        directive.path,
        descs.join(", "),
        hints.join(", "),
    )))
}

/// Type-check resolved `with` override values against the included
/// template's frontmatter declarations.
///
/// Checks all variables that have both a declaration and a resolved value
/// in `overrides`. For-each bindings are not checked here because their
/// type depends on the list item structure.
pub(crate) fn validate_include_types(
    declarations: &[VarDecl],
    overrides: &HashMap<String, Value>,
    directive: &IncludeDirective<'_>,
) -> Result<(), TemplateError> {
    for decl in declarations {
        if let Some(value) = overrides.get(&decl.name)
            && let Err(e) = decl.var_type.check(value)
        {
            let detail = if e.path.is_empty() {
                alloc::string::String::new()
            } else {
                alloc::format!(" (at .{})", e.path)
            };
            return Err(TemplateError::TypeMismatch {
                name: alloc::format!(
                    "include '{}' variable '{}'{}",
                    directive.path,
                    decl.name,
                    detail
                ),
                expected: e.expected,
                actual: e.actual,
                actual_value: e.actual_value,
            });
        }
    }
    Ok(())
}

pub(crate) fn build_overrides(
    directive: &IncludeDirective<'_>,
    scope: &Scope<'_>,
) -> Result<HashMap<String, Value>, TemplateError> {
    let mut overrides = HashMap::new();
    for &(key, val_expr) in &directive.with_vars {
        let value = if let Some(inner) = crate::consts::strip_string_literal(val_expr) {
            if inner.contains(crate::consts::EXPR_START) {
                // Interpolated string: compile {{ expr }} references and render.
                let segments = crate::compiled::compile_body(inner)?;
                let rendered = crate::compiled::render_interpolated_str(&segments, scope)?;
                Value::Str(rendered)
            } else {
                Value::Str(inner.to_string())
            }
        } else {
            // Evaluate as a full expression — supports paths, functions, filters.
            crate::parser::eval_expr(val_expr, scope)?
        };
        overrides.insert(key.to_string(), value);
    }
    Ok(overrides)
}

/// Inject default values from declarations into a scope layer for any
/// params that were not explicitly provided via `with` overrides.
pub(crate) fn inject_defaults_into_layer(
    layer: &mut HashMap<String, Value>,
    declarations: &[VarDecl],
    overrides: &HashMap<String, Value>,
) {
    for decl in declarations {
        if !overrides.contains_key(&decl.name) {
            if let Some(ref default) = decl.default_value {
                layer.insert(decl.name.clone(), default.clone());
            }
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::{
        compat::HashMap,
        parser::IncludeDirective,
        types::{VarDecl, VarType},
        value::Value,
    };

    /// Helper: build a simple `IncludeDirective` with the given path, `with_vars`,
    /// and optional `for_each` binding.
    fn directive<'a>(
        path: &'a str,
        with_vars: Vec<(&'a str, &'a str)>,
        for_each: Option<(&'a str, &'a str)>,
    ) -> IncludeDirective<'a> {
        IncludeDirective {
            path,
            with_vars,
            for_each,
        }
    }

    /// Helper: build a `VarDecl` without a default value.
    fn decl(name: &str, var_type: VarType) -> VarDecl {
        VarDecl {
            name: name.into(),
            var_type,
            default_value: None,
        }
    }

    /// Helper: build a `VarDecl` with a default value.
    fn decl_with_default(name: &str, var_type: VarType, default: Value) -> VarDecl {
        VarDecl {
            name: name.into(),
            var_type,
            default_value: Some(default),
        }
    }

    // ── validate_include_contract ────────────────────────────────────

    #[test]
    fn contract_empty_declarations_always_ok() {
        let d = directive("tmpl.md", vec![], None);
        assert!(validate_include_contract(&[], &d).is_ok());
    }

    #[test]
    fn contract_all_params_via_with_vars() {
        let declarations = vec![decl("name", VarType::Str), decl("count", VarType::Int)];
        let d = directive(
            "tmpl.md",
            vec![("name", "\"Alice\""), ("count", "42")],
            None,
        );
        assert!(validate_include_contract(&declarations, &d).is_ok());
    }

    #[test]
    fn contract_param_provided_via_for_each_binding() {
        let declarations = vec![decl("item", VarType::Str)];
        let d = directive("tmpl.md", vec![], Some(("item", "items")));
        assert!(validate_include_contract(&declarations, &d).is_ok());
    }

    #[test]
    fn contract_mixed_with_and_for_each() {
        let declarations = vec![decl("item", VarType::Str), decl("label", VarType::Str)];
        let d = directive(
            "tmpl.md",
            vec![("label", "\"hello\"")],
            Some(("item", "items")),
        );
        assert!(validate_include_contract(&declarations, &d).is_ok());
    }

    #[test]
    fn contract_missing_required_param_errors() {
        let declarations = vec![decl("name", VarType::Str), decl("count", VarType::Int)];
        // Only provide "name", missing "count".
        let d = directive("tmpl.md", vec![("name", "\"Alice\"")], None);
        let err = validate_include_contract(&declarations, &d).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("count"),
            "error should mention the missing param 'count': {msg}"
        );
        assert!(
            msg.contains("tmpl.md"),
            "error should mention the include path: {msg}"
        );
    }

    #[test]
    fn contract_missing_all_params_errors() {
        let declarations = vec![decl("a", VarType::Str), decl("b", VarType::Int)];
        let d = directive("tmpl.md", vec![], None);
        let err = validate_include_contract(&declarations, &d).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains('a'), "should mention 'a': {msg}");
        assert!(msg.contains('b'), "should mention 'b': {msg}");
    }

    #[test]
    fn contract_param_with_default_not_counted_as_missing() {
        // find_missing_include_params skips params that have a default_value,
        // so validate_include_contract should return Ok when all missing params
        // have defaults.
        let declarations = vec![decl_with_default(
            "greeting",
            VarType::Str,
            Value::Str("hello".into()),
        )];
        let d = directive("tmpl.md", vec![], None);
        assert!(
            validate_include_contract(&declarations, &d).is_ok(),
            "params with defaults should not be flagged as missing"
        );
    }

    #[test]
    fn contract_error_includes_fix_hint() {
        let declarations = vec![decl("name", VarType::Str)];
        let d = directive("tmpl.md", vec![], None);
        let err = validate_include_contract(&declarations, &d).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("with name=name"),
            "error should include a fix hint: {msg}"
        );
    }

    // ── validate_include_types ──────────────────────────────────────

    #[test]
    fn types_matching_str_ok() {
        let declarations = vec![decl("name", VarType::Str)];
        let mut overrides = HashMap::new();
        overrides.insert("name".into(), Value::Str("Alice".into()));
        let d = directive("tmpl.md", vec![("name", "\"Alice\"")], None);
        assert!(validate_include_types(&declarations, &overrides, &d).is_ok());
    }

    #[test]
    fn types_matching_int_ok() {
        let declarations = vec![decl("count", VarType::Int)];
        let mut overrides = HashMap::new();
        overrides.insert("count".into(), Value::Int(42));
        let d = directive("tmpl.md", vec![("count", "42")], None);
        assert!(validate_include_types(&declarations, &overrides, &d).is_ok());
    }

    #[test]
    fn types_matching_bool_ok() {
        let declarations = vec![decl("flag", VarType::Bool)];
        let mut overrides = HashMap::new();
        overrides.insert("flag".into(), Value::Bool(true));
        let d = directive("tmpl.md", vec![("flag", "true")], None);
        assert!(validate_include_types(&declarations, &overrides, &d).is_ok());
    }

    #[test]
    fn types_mismatch_str_declared_int_provided() {
        let declarations = vec![decl("name", VarType::Str)];
        let mut overrides = HashMap::new();
        overrides.insert("name".into(), Value::Int(123));
        let d = directive("tmpl.md", vec![("name", "123")], None);
        let err = validate_include_types(&declarations, &overrides, &d).unwrap_err();
        assert!(
            matches!(err, TemplateError::TypeMismatch { .. }),
            "expected TypeMismatch, got: {err}"
        );
    }

    #[test]
    fn types_mismatch_int_declared_str_provided() {
        let declarations = vec![decl("count", VarType::Int)];
        let mut overrides = HashMap::new();
        overrides.insert("count".into(), Value::Str("not-a-number".into()));
        let d = directive("tmpl.md", vec![("count", "\"not-a-number\"")], None);
        let err = validate_include_types(&declarations, &overrides, &d).unwrap_err();
        assert!(matches!(err, TemplateError::TypeMismatch { .. }));
    }

    #[test]
    fn types_param_not_in_overrides_is_skipped() {
        let declarations = vec![decl("name", VarType::Str), decl("count", VarType::Int)];
        // Only override "name" — "count" is absent from overrides.
        let mut overrides = HashMap::new();
        overrides.insert("name".into(), Value::Str("Alice".into()));
        let d = directive("tmpl.md", vec![("name", "\"Alice\"")], None);
        assert!(validate_include_types(&declarations, &overrides, &d).is_ok());
    }

    #[test]
    fn types_empty_declarations_ok() {
        let overrides = HashMap::new();
        let d = directive("tmpl.md", vec![], None);
        assert!(validate_include_types(&[], &overrides, &d).is_ok());
    }

    #[test]
    fn types_mismatch_error_mentions_path_and_variable() {
        let declarations = vec![decl("greeting", VarType::Str)];
        let mut overrides = HashMap::new();
        overrides.insert("greeting".into(), Value::Bool(false));
        let d = directive("header.tmpl.md", vec![("greeting", "false")], None);
        let err = validate_include_types(&declarations, &overrides, &d).unwrap_err();
        match err {
            TemplateError::TypeMismatch { ref name, .. } => {
                assert!(
                    name.contains("header.tmpl.md"),
                    "name should mention the include path: {name}"
                );
                assert!(
                    name.contains("greeting"),
                    "name should mention the variable: {name}"
                );
            }
            other => panic!("expected TypeMismatch, got: {other}"),
        }
    }

    #[test]
    fn types_multiple_params_first_mismatch_errors() {
        let declarations = vec![decl("a", VarType::Str), decl("b", VarType::Int)];
        let mut overrides = HashMap::new();
        overrides.insert("a".into(), Value::Int(1)); // mismatch
        overrides.insert("b".into(), Value::Int(2)); // ok
        let d = directive("t.md", vec![("a", "1"), ("b", "2")], None);
        let err = validate_include_types(&declarations, &overrides, &d).unwrap_err();
        assert!(matches!(err, TemplateError::TypeMismatch { .. }));
    }

    // ── inject_defaults_into_layer ──────────────────────────────────

    #[test]
    fn defaults_injected_for_unprovided_param() {
        let mut layer = HashMap::new();
        let declarations = vec![decl_with_default(
            "greeting",
            VarType::Str,
            Value::Str("hello".into()),
        )];
        let overrides = HashMap::new(); // no overrides
        inject_defaults_into_layer(&mut layer, &declarations, &overrides);
        assert_eq!(layer.get("greeting"), Some(&Value::Str("hello".into())));
    }

    #[test]
    fn defaults_not_injected_when_already_overridden() {
        let mut layer = HashMap::new();
        let declarations = vec![decl_with_default(
            "greeting",
            VarType::Str,
            Value::Str("default".into()),
        )];
        let mut overrides = HashMap::new();
        overrides.insert("greeting".into(), Value::Str("custom".into()));
        inject_defaults_into_layer(&mut layer, &declarations, &overrides);
        assert!(
            !layer.contains_key("greeting"),
            "should NOT inject default when override exists"
        );
    }

    #[test]
    fn defaults_nothing_injected_when_no_default_value() {
        let mut layer = HashMap::new();
        let declarations = vec![decl("required_param", VarType::Str)]; // no default
        let overrides = HashMap::new();
        inject_defaults_into_layer(&mut layer, &declarations, &overrides);
        assert!(layer.is_empty(), "layer should remain empty");
    }

    #[test]
    fn defaults_multiple_params_mixed() {
        let mut layer = HashMap::new();
        let declarations = vec![
            // Has default, not overridden -> should be injected
            decl_with_default("a", VarType::Str, Value::Str("default_a".into())),
            // Has default, IS overridden -> should NOT be injected
            decl_with_default("b", VarType::Int, Value::Int(99)),
            // No default, not overridden -> nothing to inject
            decl("c", VarType::Bool),
            // Has default (int), not overridden -> should be injected
            decl_with_default("d", VarType::Int, Value::Int(0)),
        ];
        let mut overrides = HashMap::new();
        overrides.insert("b".into(), Value::Int(42));
        inject_defaults_into_layer(&mut layer, &declarations, &overrides);

        assert_eq!(layer.get("a"), Some(&Value::Str("default_a".into())));
        assert!(
            !layer.contains_key("b"),
            "b is overridden, should not inject"
        );
        assert!(!layer.contains_key("c"), "c has no default");
        assert_eq!(layer.get("d"), Some(&Value::Int(0)));
        assert_eq!(layer.len(), 2);
    }

    #[test]
    fn defaults_empty_declarations_no_effect() {
        let mut layer = HashMap::new();
        let overrides = HashMap::new();
        inject_defaults_into_layer(&mut layer, &[], &overrides);
        assert!(layer.is_empty());
    }

    #[test]
    fn defaults_layer_preserves_existing_entries() {
        let mut layer = HashMap::new();
        layer.insert("existing".into(), Value::Str("keep me".into()));
        let declarations = vec![decl_with_default(
            "new_param",
            VarType::Str,
            Value::Str("injected".into()),
        )];
        let overrides = HashMap::new();
        inject_defaults_into_layer(&mut layer, &declarations, &overrides);
        assert_eq!(layer.get("existing"), Some(&Value::Str("keep me".into())));
        assert_eq!(layer.get("new_param"), Some(&Value::Str("injected".into())));
    }
}
