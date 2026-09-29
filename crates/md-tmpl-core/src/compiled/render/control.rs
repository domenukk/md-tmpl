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
    scope::{CompiledExpr, ConditionOperand, Scope},
    value::Value,
};

/// Register loop metadata for a for-loop binding.
///
/// After pushing a scope layer and inserting the binding variable,
/// call this to associate `{{ idx(binding) }}` metadata.
#[inline]
pub(crate) fn register_loop_meta(scope: &mut Scope<'_>, binding: &str, i: usize, len: usize) {
    let index = i64::try_from(i).expect("loop index exceeds i64::MAX");
    scope.set_loop_meta(binding, crate::scope::LoopMeta { index, len });
}

/// Borrowed view of a [`Segment::ForLoop`] for rendering.
#[derive(Copy, Clone)]
pub(super) struct ForLoopRef<'a> {
    pub binding: &'a str,
    pub list_expr: &'a CompiledExpr,
    pub filters: &'a [crate::compiled::ParsedFilter],
    pub body: &'a [Segment],
    pub else_body: &'a [Segment],
}

/// Render a compiled for-loop.
#[cfg(feature = "std")]
#[inline]
pub(super) fn render_for_loop(
    for_loop: ForLoopRef<'_>,
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let ForLoopRef {
        binding,
        list_expr,
        filters,
        body,
        else_body,
    } = for_loop;
    let mut list_ref = eval_compiled_expr_val(list_expr, scope)?;
    for f in filters {
        let filtered = crate::filter::apply_filter_parsed(f, &list_ref)?;
        list_ref = alloc::borrow::Cow::Owned(filtered);
    }
    let items = if let Value::List(items) = &*list_ref {
        Arc::clone(items)
    } else {
        let expr_str = match list_expr {
            CompiledExpr::Path(p)
            | CompiledExpr::Len(p)
            | CompiledExpr::Kind(p)
            | CompiledExpr::Kinds(p)
            | CompiledExpr::Has(p) => p.as_str(),
            CompiledExpr::Literal(_) => "literal",
            CompiledExpr::Idx(b) => b.as_ref(),
        };
        return Err(TemplateError::syntax(alloc::format!(
            "'{expr_str}' is not a list"
        )));
    };

    if items.is_empty() {
        if !else_body.is_empty() {
            return render_segments_into(else_body, scope, base_dir, output);
        }
        return Ok(());
    }

    let slot_idx = scope.begin_loop(binding);
    for (i, item) in items.iter().enumerate() {
        let index = i64::try_from(i).expect("loop index exceeds i64::MAX");
        scope.update_loop_slot(slot_idx, item, index, items.len());
        if let Err(err) = render_segments_into(body, scope, base_dir, output) {
            scope.pop_loop_binding();
            return Err(err);
        }
    }
    scope.pop_loop_binding();

    Ok(())
}

/// Render a compiled for-loop (`no_std` variant).
#[cfg(not(feature = "std"))]
pub(super) fn render_for_loop_no_std(
    for_loop: ForLoopRef<'_>,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let ForLoopRef {
        binding,
        list_expr,
        filters,
        body,
        else_body,
    } = for_loop;
    let mut list_ref = eval_compiled_expr_val(list_expr, scope)?;
    for f in filters {
        let filtered = crate::filter::apply_filter_parsed(f, &list_ref)?;
        list_ref = alloc::borrow::Cow::Owned(filtered);
    }
    let items = if let Value::List(items) = &*list_ref {
        Arc::clone(items)
    } else {
        let expr_str = match list_expr {
            CompiledExpr::Path(p)
            | CompiledExpr::Len(p)
            | CompiledExpr::Kind(p)
            | CompiledExpr::Kinds(p)
            | CompiledExpr::Has(p) => p.as_str(),
            CompiledExpr::Literal(_) => "literal",
            CompiledExpr::Idx(b) => b.as_ref(),
        };
        return Err(TemplateError::syntax(alloc::format!(
            "'{expr_str}' is not a list"
        )));
    };

    if items.is_empty() {
        if !else_body.is_empty() {
            return render_segments_into_no_std(else_body, scope, output);
        }
        return Ok(());
    }

    let slot_idx = scope.begin_loop(binding);
    for (i, item) in items.iter().enumerate() {
        let index = i64::try_from(i).expect("loop index exceeds i64::MAX");
        scope.update_loop_slot(slot_idx, item, index, items.len());
        if let Err(err) = render_segments_into_no_std(body, scope, output) {
            scope.pop_loop_binding();
            return Err(err);
        }
    }
    scope.pop_loop_binding();

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
            // If the condition is a simple `has(x)` guard on an option param,
            // narrow so inner kind()/match see the unwrapped value.
            let narrowed = extract_has_option_path(condition, scope);
            if let Some(path) = narrowed {
                scope.narrow_option(path);
            }
            let result = render_segments_into(body, scope, base_dir, output);
            if let Some(path) = narrowed {
                scope.unnarrow_option(path);
            }
            return result;
        }
    }

    if !else_body.is_empty() {
        let neg_narrowed = branches
            .first()
            .and_then(|(cond, _)| extract_negated_has_option_path(cond, scope));
        if let Some(path) = neg_narrowed {
            scope.narrow_option(path);
        }
        let result = render_segments_into(else_body, scope, base_dir, output);
        if let Some(path) = neg_narrowed {
            scope.unnarrow_option(path);
        }
        return result;
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
            let narrowed = extract_has_option_path(condition, scope);
            if let Some(path) = narrowed {
                scope.narrow_option(path);
            }
            let result = render_segments_into_no_std(body, scope, output);
            if let Some(path) = narrowed {
                scope.unnarrow_option(path);
            }
            return result;
        }
    }

    if !else_body.is_empty() {
        let neg_narrowed = branches
            .first()
            .and_then(|(cond, _)| extract_negated_has_option_path(cond, scope));
        if let Some(path) = neg_narrowed {
            scope.narrow_option(path);
        }
        let result = render_segments_into_no_std(else_body, scope, output);
        if let Some(path) = neg_narrowed {
            scope.unnarrow_option(path);
        }
        return result;
    }

    Ok(())
}

/// If `condition` is a bare `has(x)` or `x` on an option-typed param, return the path.
///
/// Used to narrow the option so `kind()`/inner `match` blocks see the
/// unwrapped enum value instead of `"Some"`.
fn extract_has_option_path<'a>(condition: &'a Condition, scope: &Scope<'_>) -> Option<&'a str> {
    let Condition::Truthy(ConditionOperand::Has(path) | ConditionOperand::Path { path, .. }) =
        condition
    else {
        return None;
    };
    let path_str = path.as_str();
    if scope.is_option_path(path_str) {
        Some(path_str)
    } else {
        None
    }
}

fn extract_negated_has_option_path<'a>(
    condition: &'a Condition,
    scope: &Scope<'_>,
) -> Option<&'a str> {
    if let Condition::Not(inner) = condition {
        extract_has_option_path(inner, scope)
    } else {
        None
    }
}
