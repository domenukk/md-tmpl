//! Rendering of compiled `{% include %}` directives.

use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;

use crate::{compiled::CompiledInclude, error::TemplateError, parser, scope::Scope};

/// Render a compiled include directive.
///
/// Includes still load files at runtime (since the included file might
/// change), but the host template's structure is pre-compiled.
#[cfg(feature = "std")]
pub(super) fn render_include(
    inc: &CompiledInclude,
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // Build an IncludeDirective from the compiled data, borrowing from the
    // owned strings.
    let with_vars: alloc::vec::Vec<(&str, &str)> = inc
        .with_vars
        .iter()
        .map(|(k, v)| (k.as_ref(), v.as_ref()))
        .collect();

    let for_each = inc.for_each.as_ref().map(|(b, l)| (b.as_ref(), l.as_ref()));

    let directive = parser::IncludeDirective {
        path: inc.path.as_ref(),
        with_vars,
        for_each,
    };

    // Depth tracking is handled inside resolve_include.
    crate::include::resolve_include_into(
        &directive,
        scope,
        base_dir,
        inc.inline_compiled.as_ref(),
        output,
    )
}

/// Render a compiled include directive under `no_std`.
///
/// Supports three resolution strategies (in order):
///
/// 1. **Pre-compiled inline AST** (`inline_compiled`) — e.g. from `{% tmpl %}`.
/// 2. **Inline template** — defined via `{% tmpl name %}` in the current file.
/// 3. **`Value::Tmpl` parameter** — a template passed as a typed parameter.
///
/// Filesystem-based includes are not available under `no_std` and produce
/// a descriptive error.
#[cfg(not(feature = "std"))]
pub(super) fn render_include_no_std(
    inc: &CompiledInclude,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    scope.enter_include()?;

    let result = render_include_no_std_inner(inc, scope, output);
    scope.exit_include();

    result.map_err(|e| match e {
        TemplateError::Syntax(ref syn) if syn.message.contains(&*inc.path) => e,
        TemplateError::IncludeNotFound(_) => e,
        TemplateError::Syntax(syn) => {
            TemplateError::syntax(alloc::format!("in include '{}': {}", inc.path, syn.message))
        }
        TemplateError::UndefinedVariable(name) => TemplateError::syntax(alloc::format!(
            "in include '{}': undefined variable '{name}'",
            inc.path
        )),
        other => other,
    })
}

/// Inner include resolution for `no_std`.
#[cfg(not(feature = "std"))]
fn render_include_no_std_inner(
    inc: &CompiledInclude,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    use crate::value::Value;

    // Build an IncludeDirective from the compiled data.
    let with_vars: alloc::vec::Vec<(&str, &str)> = inc
        .with_vars
        .iter()
        .map(|(k, v)| (k.as_ref(), v.as_ref()))
        .collect();
    let for_each = inc.for_each.as_ref().map(|(b, l)| (b.as_ref(), l.as_ref()));
    let directive = parser::IncludeDirective {
        path: inc.path.as_ref(),
        with_vars,
        for_each,
    };

    // 0. Pre-compiled inline AST.
    if let Some(compiled) = &inc.inline_compiled {
        scope.push_consts(
            (*compiled.consts).clone(),
            (*compiled.imported_consts).clone(),
        );
        let result = validate_and_render_no_std(
            &compiled.segments,
            &compiled.declarations,
            &directive,
            scope,
            output,
        );
        scope.pop_consts();
        return result;
    }

    // 1. Inline templates from the current scope.
    if let Some(compiled) = scope.get_inline_template(directive.path).cloned() {
        scope.push_consts(
            (*compiled.consts).clone(),
            (*compiled.imported_consts).clone(),
        );
        let result = validate_and_render_no_std(
            &compiled.segments,
            &compiled.declarations,
            &directive,
            scope,
            output,
        );
        scope.pop_consts();
        return result;
    }

    // 2. Value::Tmpl parameter.
    // NOLINT: resolution failure means path is not a tmpl() param — fall through to filesystem
    if let Ok(Value::Tmpl(tmpl)) = scope.resolve_path_str(directive.path) {
        let tmpl = tmpl.clone();
        scope.push_inline_templates(tmpl.inline_templates().clone());
        scope.push_consts((*tmpl.consts()).clone(), (*tmpl.imported_consts()).clone());

        let result = validate_and_render_no_std(
            tmpl.segments(),
            tmpl.declarations(),
            &directive,
            scope,
            output,
        );

        scope.pop_consts();
        scope.pop_inline_templates();
        return result;
    }

    // 3. No match — filesystem includes not available under no_std.
    Err(TemplateError::IncludeNotFound(alloc::format!(
        "cannot resolve '{}': filesystem includes require the `std` feature",
        directive.path
    )))
}

/// Common validation + render path for `no_std` includes.
#[cfg(not(feature = "std"))]
fn validate_and_render_no_std(
    segments: &[crate::compiled::Segment],
    declarations: &[crate::types::VarDecl],
    directive: &parser::IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    use crate::{
        include_core::{
            build_overrides, inject_defaults_into_layer, validate_include_contract,
            validate_include_types,
        },
        value::Value,
    };

    validate_include_contract(declarations, directive)?;
    let overrides = build_overrides(directive, scope)?;
    validate_include_types(declarations, &overrides, directive)?;

    scope.push_declarations(declarations);

    let res = if let Some((binding, list_expr)) = &directive.for_each {
        // Iterated include.
        let list_value = crate::parser::eval_expr(list_expr.trim(), scope)?;
        let Value::List(items) = list_value else {
            scope.pop_declarations(declarations);
            return Err(TemplateError::syntax(alloc::format!(
                "'{list_expr}' is not a list"
            )));
        };

        for (i, item) in items.iter().enumerate() {
            {
                let layer = scope.push_layer();
                layer.insert(binding.to_string(), item.clone());
                for (k, v) in &overrides {
                    layer.insert(k.clone(), v.clone());
                }
                inject_defaults_into_layer(layer, declarations, &overrides);
            }
            super::control::register_loop_meta(scope, binding, i);
            super::segments::render_segments_into_no_std(segments, scope, output)?;
            scope.pop_layer();
        }
        Ok(())
    } else {
        // Simple include.
        let has_defaults = declarations.iter().any(|d| d.default_value.is_some());
        let needs_layer = !directive.with_vars.is_empty() || has_defaults;
        if needs_layer {
            let layer = scope.push_layer();
            for (k, v) in &overrides {
                layer.insert(k.clone(), v.clone());
            }
            inject_defaults_into_layer(layer, declarations, &overrides);
        }
        let r = super::segments::render_segments_into_no_std(segments, scope, output);
        if needs_layer {
            scope.pop_layer();
        }
        r
    };
    scope.pop_declarations(declarations);
    res
}
