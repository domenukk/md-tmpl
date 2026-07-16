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
    compat::HashMap, compiled::CompiledInlineTemplate, error::TemplateError,
    parser::IncludeDirective, scope::Scope, types::VarDecl, value::Value,
};

/// Bundles the template-specific data needed when rendering an include.
struct IncludeRenderContext<'a> {
    segments: &'a [crate::compiled::Segment],
    declarations: &'a [VarDecl],
    overrides: &'a HashMap<String, Value>,
    include_base: Option<&'a Path>,
}

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

    let interpolated_path;
    let effective_directive = if directive.path.contains(crate::consts::EXPR_START) {
        match interpolate_include_path(directive.path, scope) {
            Ok(path) => {
                if !crate::consts::is_valid_resolved_path(&path)
                    || path.contains(crate::consts::EXPR_START)
                {
                    scope.exit_include();
                    return Err(TemplateError::syntax(format!(
                        "include path '{path}' must start with './', '../', or '/'"
                    )));
                }
                interpolated_path = path;
                IncludeDirective {
                    path: &interpolated_path,
                    with_vars: directive.with_vars.clone(),
                    for_each: directive.for_each,
                }
            }
            Err(e) => {
                scope.exit_include();
                return Err(e);
            }
        }
    } else {
        directive.clone()
    };

    let result = resolve_include_inner_into(
        &effective_directive,
        scope,
        base_dir,
        inline_compiled,
        output,
    );
    scope.exit_include();
    // Wrap errors with include path context for debugging nested includes.
    result.map_err(|e| wrap_include_error(e, effective_directive.path))
}

fn interpolate_include_path(path: &str, scope: &Scope<'_>) -> Result<String, TemplateError> {
    let mut result = String::new();
    let mut remaining = path;

    while let Some(start_idx) = remaining.find(crate::consts::EXPR_START) {
        result.push_str(&remaining[..start_idx]);
        let after_start = &remaining[start_idx + crate::consts::EXPR_START.len()..];

        let Some(end_idx) = after_start.find(crate::consts::EXPR_END) else {
            return Err(TemplateError::syntax(format!(
                "unclosed '{}' in include path '{path}'",
                crate::consts::EXPR_START
            )));
        };

        let expr = after_start[..end_idx].trim();
        if expr.is_empty() {
            return Err(TemplateError::syntax(format!(
                "empty expression '{}{}' in include path '{path}'",
                crate::consts::EXPR_START,
                crate::consts::EXPR_END
            )));
        }

        let val_str = if let Some(lit) = crate::consts::strip_string_literal(expr) {
            crate::consts::unescape_string_literal(lit)
        } else {
            let val = scope.resolve_path_str(expr).map_err(|_| {
                TemplateError::syntax(format!(
                    "unresolvable expression '{}{expr}{}' in include path '{path}'",
                    crate::consts::EXPR_START,
                    crate::consts::EXPR_END
                ))
            })?;
            val.to_string()
        };

        result.push_str(&val_str);
        remaining = &after_start[end_idx + crate::consts::EXPR_END.len()..];
    }

    result.push_str(remaining);
    Ok(result)
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
        return resolve_from_precompiled(compiled, directive, scope, base_dir, output);
    }

    // 1. Check inline templates first (defined via {% tmpl name %})
    //    in the CURRENT file's scope.
    //    Clone the compiled template to release the immutable borrow on scope,
    //    allowing mutable access for build_overrides/rendering.
    if let Some(compiled) = scope.get_inline_template(directive.path).cloned() {
        return resolve_from_precompiled(&compiled, directive, scope, base_dir, output);
    }

    // 1b. Check if path is a variable resolving to a template (higher-order).
    // NOLINT: resolution failure means path is not a tmpl() param — fall through to filesystem
    if let Ok(Value::Tmpl(tmpl)) = scope.resolve_path_str(directive.path) {
        let tmpl = tmpl.clone();
        return resolve_from_tmpl_value(&tmpl, directive, scope, output);
    }

    // 2. Fall through to filesystem lookup.
    resolve_from_filesystem(directive, scope, base_dir, output)
}

/// Resolve an include from a precompiled inline template.
fn resolve_from_precompiled(
    compiled: &CompiledInlineTemplate,
    directive: &IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    scope.push_consts(
        (*compiled.consts).clone(),
        (*compiled.imported_consts).clone(),
    );
    let result = validate_and_render_into(
        &compiled.segments,
        &compiled.declarations,
        directive,
        scope,
        base_dir,
        output,
    );
    scope.pop_consts();
    result
}

/// Resolve an include from a `Value::Tmpl` (higher-order template parameter).
fn resolve_from_tmpl_value(
    tmpl: &std::sync::Arc<crate::template::Template>,
    directive: &IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    scope.push_inline_templates(tmpl.inline_templates().clone());
    scope.push_consts((*tmpl.consts()).clone(), (*tmpl.imported_consts()).clone());

    let result = validate_and_render_into(
        tmpl.segments(),
        tmpl.declarations(),
        directive,
        scope,
        tmpl.base_dir(),
        output,
    );

    scope.pop_consts();
    scope.pop_inline_templates();
    result
}

/// Resolve an include from the filesystem (cache or disk).
fn resolve_from_filesystem(
    directive: &IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let base = base_dir.ok_or_else(|| {
        TemplateError::IncludeNotFound(format!(
            "cannot resolve '{}': no base directory",
            directive.path
        ))
    })?;

    let include_path = base.join(directive.path);

    // Try the template cache first — avoids re-reading and re-compiling
    // unchanged include files.
    if let Some(cache) = scope.cache() {
        let cached = cache.resolve_include(&include_path, scope.compile_env())?;
        scope.push_consts(cached.consts.clone(), cached.imported_consts.clone());
        let result = validate_and_render_into(
            &cached.segments,
            &cached.declarations,
            directive,
            scope,
            Some(cached.base_dir.as_path()),
            output,
        );
        scope.pop_consts();
        return result;
    }

    // No cache — read and compile from disk.
    let source = std::fs::read_to_string(&include_path).map_err(|err| {
        TemplateError::IncludeNotFound(format!("{}: {err}", include_path.display()))
    })?;

    let include_base = include_path.parent().unwrap_or(base);
    // Propagate compile-time env values to included files so their
    // `env:` frontmatter declarations are resolved with the same values.
    let env_pairs: Vec<(&str, crate::value::Value)> = scope
        .compile_env()
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    let (fm, body) =
        crate::frontmatter::parse_frontmatter_with_base_dir(&source, include_base, &env_pairs)?;
    let (segments, included_inline_templates) = crate::compiled::compile(body, &fm.type_aliases)?;

    // Scope the included file's own inline templates: push them for rendering,
    // then pop after. This ensures each file's {% tmpl %} definitions are
    // visible only within that file's body.
    scope.push_inline_templates(included_inline_templates);

    let mut include_consts = HashMap::new();
    for d in &fm.consts {
        if let Some(v) = d.default_value.clone() {
            include_consts.insert(d.name.clone(), v);
        }
    }
    // Inject resolved env values as constants (mirrors compile_inner behavior).
    for d in &fm.env {
        if let Some(ref v) = d.default_value {
            include_consts
                .entry(d.name.clone())
                .or_insert_with(|| v.clone());
        }
    }
    scope.push_consts(include_consts, fm.imported_consts);

    let result = validate_and_render_into(
        &segments,
        &fm.declarations,
        directive,
        scope,
        Some(include_base),
        output,
    );
    scope.pop_consts();
    scope.pop_inline_templates();
    result
}

/// Common resolution path writing directly into `output`.
fn validate_and_render_into(
    segments: &[crate::compiled::Segment],
    declarations: &[VarDecl],
    directive: &IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    validate_include_contract(declarations, directive)?;
    let overrides = build_overrides(directive, scope)?;
    validate_include_types(declarations, &overrides, directive)?;

    scope.push_declarations(declarations);

    let ctx = IncludeRenderContext {
        segments,
        declarations,
        overrides: &overrides,
        include_base: base_dir,
    };

    let res = if let Some((binding, list_expr)) = &directive.for_each {
        render_iterated_include_into(&ctx, binding, list_expr, scope, output)
    } else {
        render_simple_include_into(&ctx, scope, directive, output)
    };
    scope.pop_declarations(declarations);
    res
}

/// Validate that all variables declared by an included template are explicitly
/// provided via `with` overrides or `for` bindings.
///
/// Delegates to the shared implementation in `include_core`.
fn validate_include_contract(
    declarations: &[VarDecl],
    directive: &IncludeDirective<'_>,
) -> Result<(), TemplateError> {
    crate::include_core::validate_include_contract(declarations, directive)
}

/// Type-check resolved `with` override values against the included
/// template's frontmatter declarations.
///
/// Delegates to the shared implementation in `include_core`.
fn validate_include_types(
    declarations: &[VarDecl],
    overrides: &HashMap<String, Value>,
    directive: &IncludeDirective<'_>,
) -> Result<(), TemplateError> {
    crate::include_core::validate_include_types(declarations, overrides, directive)
}

/// Build the override variable map from `with key=expr` clauses.
///
/// Delegates to the shared implementation in `include_core`.
fn build_overrides(
    directive: &IncludeDirective<'_>,
    scope: &Scope<'_>,
) -> Result<HashMap<String, Value>, TemplateError> {
    crate::include_core::build_overrides(directive, scope)
}

/// Render an iterated include directly into `output`.
fn render_iterated_include_into(
    ctx: &IncludeRenderContext<'_>,
    binding: &str,
    list_expr: &str,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let list_value = crate::parser::eval_expr(list_expr.trim(), scope)?;
    let Value::List(items) = list_value else {
        return Err(TemplateError::syntax(format!(
            "'{list_expr}' is not a list"
        )));
    };

    for (i, item) in items.iter().enumerate() {
        {
            let layer = scope.push_layer();
            layer.insert(binding.to_string(), item.clone());
            for (k, v) in ctx.overrides {
                layer.insert(k.clone(), v.clone());
            }
            // Inject defaults for declared params not explicitly provided.
            inject_defaults_into_layer(layer, ctx.declarations, ctx.overrides);
        }
        crate::compiled::register_loop_meta(scope, binding, i);
        crate::compiled::render_segments_into(ctx.segments, scope, ctx.include_base, output)?;
        scope.pop_layer();
    }

    Ok(())
}

/// Render a simple include directly into `output`.
fn render_simple_include_into(
    ctx: &IncludeRenderContext<'_>,
    scope: &mut Scope<'_>,
    directive: &IncludeDirective<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let has_defaults = ctx.declarations.iter().any(|d| d.default_value.is_some());
    let needs_layer = !directive.with_vars.is_empty() || has_defaults;
    if needs_layer {
        let layer = scope.push_layer();
        for (k, v) in ctx.overrides {
            layer.insert(k.clone(), v.clone());
        }
        // Inject defaults for declared params not explicitly provided.
        inject_defaults_into_layer(layer, ctx.declarations, ctx.overrides);
    }
    crate::compiled::render_segments_into(ctx.segments, scope, ctx.include_base, output)?;
    if needs_layer {
        scope.pop_layer();
    }
    Ok(())
}

/// Inject default values from declarations into a scope layer for any
/// params that were not explicitly provided via `with` overrides.
///
/// Delegates to the shared implementation in `include_core`.
fn inject_defaults_into_layer(
    layer: &mut HashMap<String, Value>,
    declarations: &[VarDecl],
    overrides: &HashMap<String, Value>,
) {
    crate::include_core::inject_defaults_into_layer(layer, declarations, overrides);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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
            r"---
name: header
params: []
---
# Header",
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
            r"---
name: greeting
params: [name = str]
---
Hello {{ name }}!",
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
            r"---
name: row
params: [item = str]
---
- {{ item.label }}
",
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
            Value::List(Arc::new(vec![
                Value::Struct(Arc::new(HashMap::from([(
                    "label".into(),
                    Value::Str("first".into()),
                )]))),
                Value::Struct(Arc::new(HashMap::from([(
                    "label".into(),
                    Value::Str("second".into()),
                )]))),
            ])),
        );
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(
            result,
            r"- first
- second
"
        );
    }

    #[test]
    fn contract_rejects_missing_params() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("needs_vars.tmpl.md"),
            r"---
name: needs_vars
params: [title = str, count = int]
---
{{ title }} ({{ count }})",
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
            r"---
name: greeting
params: [name = str]
---
Hello {{ name }}!",
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
            r"---
name: row
params: [item = str]
---
- {{ item.label }}
",
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
            Value::List(Arc::new(vec![Value::Struct(Arc::new(HashMap::from([(
                "label".into(),
                Value::Str("test".into()),
            )])))])),
        );
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(
            result,
            "- test
"
        );
    }

    #[test]
    fn contract_no_params_always_ok() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("static.tmpl.md"),
            r"---
name: static
params: []
---
Static content",
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
            r"---
params: [count = int]
---
{{ count }}",
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
            r"---
params: [count = int]
---
{{ count }}",
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

    /// Regression: included templates with default parameter values must have
    /// those defaults injected into the scope when rendering. Previously only
    /// explicit `with` overrides were set, so params with defaults but no
    /// explicit `with` clause caused "undefined variable" errors.
    #[test]
    fn include_injects_defaults_for_unprovided_params() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("greet.tmpl.md"),
            r#"---
params:
  - name = str
  - greeting = str := "Hi"
---
{{ greeting }} {{ name }}!"#,
        )
        .unwrap();

        // Only pass `name` — `greeting` should use its default "Hi".
        let directive = IncludeDirective {
            path: "greet.tmpl.md",
            with_vars: vec![("name", "\"World\"")],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(result, "Hi World!");
    }

    /// Regression: default injection must also work for iterated includes.
    #[test]
    fn include_for_each_injects_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("row.tmpl.md"),
            r#"---
params:
  - item = str
  - prefix = str := "-"
---
{{ prefix }} {{ item.label }}
"#,
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
            Value::List(Arc::new(vec![Value::Struct(Arc::new(HashMap::from([(
                "label".into(),
                Value::Str("alpha".into()),
            )])))])),
        );
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(
            result,
            "- alpha
"
        );
    }

    /// Regression: when an override IS provided, it must take precedence
    /// over the default value.
    #[test]
    fn include_override_takes_precedence_over_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("greet.tmpl.md"),
            r#"---
params:
  - name = str
  - greeting = str := "Hi"
---
{{ greeting }} {{ name }}!"#,
        )
        .unwrap();

        // Explicitly override `greeting` — should use "Hey", not the default "Hi".
        let directive = IncludeDirective {
            path: "greet.tmpl.md",
            with_vars: vec![("name", "\"World\""), ("greeting", "\"Hey\"")],
            for_each: None,
        };
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let result = resolve_include(&directive, &mut scope, Some(dir.path()), None).unwrap();
        assert_eq!(result, "Hey World!");
    }
}
