//! Template include resolution.
//!
//! Handles loading and rendering of included template files, with support
//! for variable overrides (`with`) and iterated includes (`for ... in`).
//!
//! **Explicit passing rule**: If an included template declares variables in
//! its frontmatter, every declared variable MUST be explicitly provided via
//! `with key=expr` or `for binding in list`. Implicit scope inheritance is
//! not allowed for declared variables.

use std::path::Path;

use crate::{
    compiled::CompiledInlineTemplate, error::TemplateError, parser::IncludeDirective, scope::Scope,
    value::Value,
};

/// Resolve an include directive: load the file, optionally iterate, render.
///
/// Validates that all variables declared by the included template are
/// explicitly provided via `with` overrides or `for` bindings.
#[cfg(test)]
pub(crate) fn resolve_include(
    directive: &IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    inline_compiled: Option<&CompiledInlineTemplate>,
) -> Result<String, TemplateError> {
    let mut output = String::new();
    resolve_include_into(directive, scope, base_dir, inline_compiled, &mut output)?;
    Ok(output)
}

/// Resolve an include directive and write directly into an output buffer.
pub(crate) fn resolve_include_into(
    directive: &IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    inline_compiled: Option<&CompiledInlineTemplate>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // Track include depth to prevent infinite recursion.
    scope.enter_include()?;

    let result = resolve_include_inner_into(directive, scope, base_dir, inline_compiled, output);
    scope.exit_include();
    // Wrap errors with include path context for debugging nested includes.
    result.map_err(|e| wrap_include_error(e, directive.path))
}

/// Add include file path context to an error, avoiding double-wrapping.
fn wrap_include_error(err: TemplateError, include_path: &str) -> TemplateError {
    match err {
        // Don't wrap errors that already mention the include path.
        TemplateError::Syntax(ref syn) if syn.message.contains(include_path) => err,
        TemplateError::IncludeNotFound(_) => err,
        TemplateError::Syntax(syn) => {
            TemplateError::syntax(format!("in include '{include_path}': {}", syn.message))
        }
        TemplateError::UndefinedVariable(name) => TemplateError::syntax(format!(
            "in include '{include_path}': undefined variable '{name}'"
        )),
        other => other,
    }
}

/// Inner include resolution — called after depth is checked.
fn resolve_include_inner_into(
    directive: &IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    inline_compiled: Option<&CompiledInlineTemplate>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // 0. Use compile-time precompiled AST if present.
    if let Some(compiled) = inline_compiled {
        return validate_and_render_into(
            &compiled.segments,
            &compiled.declarations,
            directive,
            scope,
            base_dir,
            output,
        );
    }

    // 1. Check inline templates first (defined via {% tmpl name %}).
    //    Clone the compiled template to release the immutable borrow on scope,
    //    allowing mutable access for build_overrides/rendering.
    if let Some(compiled) = scope.get_inline_template(directive.path).cloned() {
        return validate_and_render_into(
            &compiled.segments,
            &compiled.declarations,
            directive,
            scope,
            base_dir,
            output,
        );
    }

    // 2. Fall through to filesystem lookup.
    let base = base_dir.ok_or_else(|| {
        TemplateError::IncludeNotFound(format!(
            "cannot resolve '{}': no base directory",
            directive.path
        ))
    })?;

    let include_path = base.join(directive.path);

    // 2a. Try the template cache first — avoids re-reading and re-compiling
    //     unchanged include files.
    if let Some(cache) = scope.cache() {
        let cached = cache.resolve_include(&include_path)?;
        return validate_and_render_into(
            &cached.segments,
            &cached.declarations,
            directive,
            scope,
            Some(cached.base_dir.as_path()),
            output,
        );
    }

    // 2b. No cache — read and compile from disk.
    let source = std::fs::read_to_string(&include_path).map_err(|err| {
        TemplateError::IncludeNotFound(format!("{}: {err}", include_path.display()))
    })?;

    let include_base = include_path.parent().unwrap_or(base);
    let (fm, body) = crate::frontmatter::parse_frontmatter(&source)?;
    let (segments, _inline_templates) = crate::compiled::compile(body)?;

    validate_and_render_into(
        &segments,
        &fm.declarations,
        directive,
        scope,
        Some(include_base),
        output,
    )
}

/// Common resolution path writing directly into `output`.
fn validate_and_render_into(
    segments: &[crate::compiled::Segment],
    declarations: &[crate::types::VarDecl],
    directive: &IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    validate_include_contract(declarations, directive)?;
    let overrides = build_overrides(directive, scope)?;
    validate_include_types(declarations, &overrides, directive)?;

    if let Some((binding, list_expr)) = &directive.for_each {
        render_iterated_include_into(
            segments, binding, list_expr, &overrides, scope, base_dir, output,
        )
    } else {
        render_simple_include_into(segments, &overrides, scope, base_dir, directive, output)
    }
}

/// Validate that all variables declared by an included template are explicitly
/// provided via `with` overrides or `for` bindings.
///
/// This enforces the explicit passing rule: included templates cannot silently
/// inherit variables from the parent scope. Every declared variable must be
/// accounted for in the include directive.
fn validate_include_contract(
    declarations: &[crate::types::VarDecl],
    directive: &IncludeDirective<'_>,
) -> Result<(), TemplateError> {
    if declarations.is_empty() {
        return Ok(());
    }

    // Collect explicitly provided variable names.
    let mut provided: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for &(key, _) in &directive.with_vars {
        provided.insert(key);
    }
    if let Some((binding, _)) = &directive.for_each {
        provided.insert(binding);
    }

    let missing: Vec<String> = declarations
        .iter()
        .filter(|decl| !provided.contains(decl.name.as_str()))
        .map(|decl| format!("{}: {}", decl.name, decl.var_type))
        .collect();

    if !missing.is_empty() {
        return Err(TemplateError::syntax(format!(
            "include '{}' requires explicit parameters: {}. \
             Use 'with {}' to pass them",
            directive.path,
            missing.join(", "),
            declarations
                .iter()
                .filter(|d| !provided.contains(d.name.as_str()))
                .map(|d| format!("{}={}", d.name, d.name))
                .collect::<Vec<_>>()
                .join(", "),
        )));
    }

    Ok(())
}

/// Type-check resolved `with` override values against the included
/// template's frontmatter declarations.
///
/// Checks all variables that have both a declaration and a resolved value
/// in `overrides`. For-each bindings are not checked here because their
/// type depends on the list item structure.
fn validate_include_types(
    declarations: &[crate::types::VarDecl],
    overrides: &std::collections::HashMap<String, Value>,
    directive: &IncludeDirective<'_>,
) -> Result<(), TemplateError> {
    for decl in declarations {
        if let Some(value) = overrides.get(&decl.name)
            && let Err(e) = decl.var_type.check(value)
        {
            let detail = if e.path.is_empty() {
                String::new()
            } else {
                format!(" (at .{})", e.path)
            };
            return Err(TemplateError::TypeMismatch {
                name: format!(
                    "include '{}' variable '{}'{}",
                    directive.path, decl.name, detail
                ),
                expected: e.expected,
                actual: e.actual,
                actual_value: e.actual_value,
            });
        }
    }
    Ok(())
}

/// Build the override variable map from `with key=expr` clauses.
fn build_overrides(
    directive: &IncludeDirective<'_>,
    scope: &Scope<'_>,
) -> Result<std::collections::HashMap<String, Value>, TemplateError> {
    let mut overrides = std::collections::HashMap::new();
    for &(key, val_expr) in &directive.with_vars {
        let value = if val_expr.starts_with('"') || val_expr.starts_with('\'') {
            Value::Str(val_expr.trim_matches('"').trim_matches('\'').to_string())
        } else {
            // Evaluate as a full expression — supports paths, functions, filters.
            crate::parser::eval_expr(val_expr, scope)?
        };
        overrides.insert(key.to_string(), value);
    }
    Ok(overrides)
}

/// Render an iterated include directly into `output`.
fn render_iterated_include_into(
    segments: &[crate::compiled::Segment],
    binding: &str,
    list_expr: &str,
    overrides: &std::collections::HashMap<String, Value>,
    scope: &mut Scope<'_>,
    include_base: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let list_value = crate::parser::eval_expr(list_expr.trim(), scope)?;
    let Value::List(items) = list_value else {
        return Err(TemplateError::syntax(format!(
            "'{list_expr}' is not a list"
        )));
    };

    for (i, item) in items.into_iter().enumerate() {
        {
            let layer = scope.push_layer();
            layer.insert(binding.to_string(), item);
            for (k, v) in overrides {
                layer.insert(k.clone(), v.clone());
            }
        }
        crate::compiled::register_loop_meta(scope, binding, i);
        crate::compiled::render_segments_into(segments, scope, include_base, output)?;
        scope.pop_layer();
    }

    Ok(())
}

/// Render a simple include directly into `output`.
fn render_simple_include_into(
    segments: &[crate::compiled::Segment],
    overrides: &std::collections::HashMap<String, Value>,
    scope: &mut Scope<'_>,
    include_base: Option<&Path>,
    directive: &IncludeDirective<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let has_overrides = !directive.with_vars.is_empty();
    if has_overrides {
        let layer = scope.push_layer();
        for (k, v) in overrides {
            layer.insert(k.clone(), v.clone());
        }
    }
    crate::compiled::render_segments_into(segments, scope, include_base, output)?;
    if has_overrides {
        scope.pop_layer();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    #[test]
    fn include_without_base_dir_errors() {
        let directive = IncludeDirective {
            path: "missing.tmpl.md",
            with_vars: vec![],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let err = resolve_include(&directive, &mut scope, None, None)
            .expect_err("include without base dir should fail");
        assert!(
            err.to_string().contains("no base directory"),
            "should mention missing base directory: {err}"
        );
    }

    #[test]
    fn include_missing_file_errors() {
        let directive = IncludeDirective {
            path: "nonexistent.tmpl.md",
            with_vars: vec![],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_include(&directive, &mut scope, Some(dir.path()), None)
            .expect_err("include of nonexistent file should fail");
        assert!(
            err.to_string().contains("nonexistent.tmpl.md"),
            "should mention missing file: {err}"
        );
    }

    #[test]
    fn include_simple_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("header.tmpl.md"),
            "---\nname: header\nparams: []\n---\n# Header",
        )
        .unwrap();

        let directive = IncludeDirective {
            path: "header.tmpl.md",
            with_vars: vec![],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(result, "# Header");
    }

    #[test]
    fn include_with_vars() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("greeting.tmpl.md"),
            "---\nname: greeting\nparams: [name = str]\n---\nHello {{ name }}!",
        )
        .unwrap();

        let directive = IncludeDirective {
            path: "greeting.tmpl.md",
            with_vars: vec![("name", "\"World\"")],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn include_for_each() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("row.tmpl.md"),
            "---\nname: row\nparams: [item = str]\n---\n- {{ item.label }}\n",
        )
        .unwrap();

        let directive = IncludeDirective {
            path: "row.tmpl.md",
            with_vars: vec![],
            for_each: Some(("item", "items")),
        };

        let mut ctx = Context::new();
        ctx.set(
            "items",
            Value::List(vec![
                Value::Dict(std::collections::HashMap::from([(
                    "label".into(),
                    Value::Str("first".into()),
                )])),
                Value::Dict(std::collections::HashMap::from([(
                    "label".into(),
                    Value::Str("second".into()),
                )])),
            ]),
        );
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert!(result.contains("- first"));
        assert!(result.contains("- second"));
    }

    #[test]
    fn contract_rejects_missing_params() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("needs_vars.tmpl.md"),
            "---\nname: needs_vars\nparams: [title = str, count = int]\n---\n{{ title }} ({{ count }})",
        )
        .unwrap();

        // Include without providing declared parameters → error.
        let directive = IncludeDirective {
            path: "needs_vars.tmpl.md",
            with_vars: vec![],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let err = resolve_include(&directive, &mut scope, Some(dir.path()), None)
            .expect_err("include without required params should fail");
        let err = err.to_string();
        assert!(err.contains("title"), "error should mention 'title': {err}");
        assert!(err.contains("count"), "error should mention 'count': {err}");
    }

    #[test]
    fn contract_accepts_with_vars() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("greeting.tmpl.md"),
            "---\nname: greeting\nparams: [name = str]\n---\nHello {{ name }}!",
        )
        .unwrap();

        // Provide the declared variable via `with` → OK.
        let directive = IncludeDirective {
            path: "greeting.tmpl.md",
            with_vars: vec![("name", "\"World\"")],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn contract_accepts_for_each_binding() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("row.tmpl.md"),
            "---\nname: row\nparams: [item = str]\n---\n- {{ item.label }}\n",
        )
        .unwrap();

        // `for item in items` provides the `item` binding → OK.
        let directive = IncludeDirective {
            path: "row.tmpl.md",
            with_vars: vec![],
            for_each: Some(("item", "items")),
        };

        let mut ctx = Context::new();
        ctx.set(
            "items",
            Value::List(vec![Value::Dict(std::collections::HashMap::from([(
                "label".into(),
                Value::Str("test".into()),
            )]))]),
        );
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert!(result.contains("- test"));
    }

    #[test]
    fn contract_no_params_always_ok() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("static.tmpl.md"),
            "---\nname: static\nparams: []\n---\nStatic content",
        )
        .unwrap();

        let directive = IncludeDirective {
            path: "static.tmpl.md",
            with_vars: vec![],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(result, "Static content");
    }

    #[test]
    fn include_type_mismatch_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("typed.tmpl.md"),
            "---\nparams: [count = int]\n---\n{{ count }}",
        )
        .unwrap();

        let directive = IncludeDirective {
            path: "typed.tmpl.md",
            with_vars: vec![("count", "name")], // passes a string as int
            for_each: None,
        };
        let mut ctx = Context::new();
        ctx.set("name", "not an int");
        let mut scope = Scope::new(&ctx);
        let err = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap_err();
        assert!(
            matches!(err, TemplateError::TypeMismatch { .. }),
            "expected TypeMismatch, got: {err}"
        );
    }

    #[test]
    fn include_correct_types_pass() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("typed.tmpl.md"),
            "---\nparams: [count = int]\n---\n{{ count }}",
        )
        .unwrap();

        let directive = IncludeDirective {
            path: "typed.tmpl.md",
            with_vars: vec![("count", "num")],
            for_each: None,
        };
        let mut ctx = Context::new();
        ctx.set("num", 42);
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(result, "42");
    }
}
