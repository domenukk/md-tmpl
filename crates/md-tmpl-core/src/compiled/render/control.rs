//! For-loop and conditional (`if`/`elif`/`else`) rendering.

use alloc::{string::String, sync::Arc};

use super::expr::eval_compiled_expr_val;
#[cfg(feature = "std")]
use super::segments::render_segments_into;
#[cfg(not(feature = "std"))]
use super::segments::render_segments_into_no_std;
use crate::{
    compiled::{Condition, Segment},
    error::TemplateError,
    scope::{CompiledExpr, Scope},
    value::Value,
};

/// Register loop metadata for a for-loop binding.
///
/// After pushing a scope layer and inserting the binding variable,
/// call this to associate `{{ idx(binding) }}` metadata.
#[inline]
pub(crate) fn register_loop_meta(scope: &mut Scope<'_>, binding: &str, i: usize) {
    let index = i64::try_from(i).expect("loop index exceeds i64::MAX");
    scope.set_loop_meta(binding, crate::scope::LoopMeta { index });
}

/// Render a compiled for-loop.
#[cfg(feature = "std")]
#[inline]
pub(super) fn render_for_loop(
    binding: &str,
    list_expr: &CompiledExpr,
    body: &[Segment],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let list_ref = eval_compiled_expr_val(list_expr, scope)?;
    let items = if let Value::List(items) = &*list_ref {
        Arc::clone(items)
    } else {
        let expr_str = match list_expr {
            CompiledExpr::Path(p)
            | CompiledExpr::Len(p)
            | CompiledExpr::Kind(p)
            | CompiledExpr::Kinds(p)
            | CompiledExpr::Has(p) => p.as_str(),
            CompiledExpr::Idx(b) => b.as_ref(),
        };
        return Err(TemplateError::syntax(alloc::format!(
            "'{expr_str}' is not a list"
        )));
    };

    if items.is_empty() && !else_body.is_empty() {
        return render_segments_into(else_body, scope, base_dir, output);
    }

    for (i, item) in items.iter().enumerate() {
        scope.push_loop_binding(binding, item);
        register_loop_meta(scope, binding, i);
        render_segments_into(body, scope, base_dir, output)?;
        scope.pop_loop_binding();
    }

    Ok(())
}

/// Render a compiled for-loop (`no_std` variant).
#[cfg(not(feature = "std"))]
pub(super) fn render_for_loop_no_std(
    binding: &str,
    list_expr: &CompiledExpr,
    body: &[Segment],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let list_ref = eval_compiled_expr_val(list_expr, scope)?;
    let items = if let Value::List(items) = &*list_ref {
        Arc::clone(items)
    } else {
        let expr_str = match list_expr {
            CompiledExpr::Path(p)
            | CompiledExpr::Len(p)
            | CompiledExpr::Kind(p)
            | CompiledExpr::Kinds(p)
            | CompiledExpr::Has(p) => p.as_str(),
            CompiledExpr::Idx(b) => b.as_ref(),
        };
        return Err(TemplateError::syntax(alloc::format!(
            "'{expr_str}' is not a list"
        )));
    };

    if items.is_empty() && !else_body.is_empty() {
        return render_segments_into_no_std(else_body, scope, output);
    }

    for (i, item) in items.iter().enumerate() {
        scope.push_loop_binding(binding, item);
        register_loop_meta(scope, binding, i);
        render_segments_into_no_std(body, scope, output)?;
        scope.pop_loop_binding();
    }

    Ok(())
}

/// Render a compiled conditional (if/elif/else chain).
///
/// Evaluates each branch's [`Condition`] in order, rendering the body
/// of the first match. Falls through to `else_body` when no branch
/// matches.
#[cfg(feature = "std")]
#[inline]
pub(super) fn render_if(
    branches: &[(Condition, alloc::vec::Vec<Segment>)],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for (condition, body) in branches {
        if super::condition::eval_condition(condition, scope)? {
            return render_segments_into(body, scope, base_dir, output);
        }
    }

    if !else_body.is_empty() {
        render_segments_into(else_body, scope, base_dir, output)?;
    }

    Ok(())
}

/// Render a compiled conditional (`no_std` variant).
#[cfg(not(feature = "std"))]
pub(super) fn render_if_no_std(
    branches: &[(Condition, alloc::vec::Vec<Segment>)],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for (condition, body) in branches {
        if super::condition::eval_condition(condition, scope)? {
            return render_segments_into_no_std(body, scope, output);
        }
    }

    if !else_body.is_empty() {
        render_segments_into_no_std(else_body, scope, output)?;
    }

    Ok(())
}
