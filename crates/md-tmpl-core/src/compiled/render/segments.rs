//! Segment-tree walking: the core render dispatch loop.
//!
//! Walks the compiled segment tree and produces output text by
//! evaluating expressions, loops, conditionals, and includes at
//! render time.

use alloc::string::String;

use super::expr::eval_compiled_expr_into;
#[cfg(feature = "std")]
use super::{
    control::{render_for_loop, render_if},
    include::render_include,
    matching::render_match,
};
#[cfg(not(feature = "std"))]
use super::{
    control::{render_for_loop_no_std, render_if_no_std},
    include::render_include_no_std,
    matching::render_match_no_std,
};
use crate::{compiled::Segment, error::TemplateError, scope::Scope};

/// Estimated output size for a single expression or include tag.
const ESTIMATED_EXPR_SIZE: usize = 32;

/// Estimated iteration count for for-loop capacity estimation.
const ESTIMATED_LOOP_ITERATIONS: usize = 4;

/// Estimate the total static text size for `String::with_capacity`.
#[must_use]
pub fn estimate_output_capacity(segments: &[Segment]) -> usize {
    let mut size: usize = 0;
    for seg in segments {
        match seg {
            Segment::Static(s) | Segment::Raw(s) => size += s.len(),
            Segment::Expr { .. } | Segment::Include(_) | Segment::Panic(_) => {
                size += ESTIMATED_EXPR_SIZE;
            }
            Segment::Comment(_) => {} // No output.
            Segment::ForLoop {
                body, else_body, ..
            } => {
                size += estimate_output_capacity(body) * ESTIMATED_LOOP_ITERATIONS;
                size += estimate_output_capacity(else_body);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                // At most one branch is rendered — use the max, not the sum.
                let mut max_branch = estimate_output_capacity(else_body);
                for (_, branch_body) in branches {
                    max_branch = max_branch.max(estimate_output_capacity(branch_body));
                }
                size += max_branch;
            }
            Segment::Match { arms, .. } => {
                // At most one arm is rendered — use the max, not the sum.
                let mut max_arm = 0;
                for arm in arms {
                    max_arm = max_arm.max(estimate_output_capacity(&arm.body));
                }
                size += max_arm;
            }
        }
    }
    size
}

/// Render pre-compiled segments into a string.
///
/// # Errors
///
/// Returns [`TemplateError`] on variable resolution, filter, or include
/// errors.
#[cfg(feature = "std")]
pub(crate) fn render_segments(
    segments: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
) -> Result<String, TemplateError> {
    let mut output = String::with_capacity(estimate_output_capacity(segments));
    render_segments_into(segments, scope, base_dir, &mut output)?;
    Ok(output)
}

/// Render segments into an existing output buffer.
#[cfg(feature = "std")]
#[inline]
pub(crate) fn render_segments_into(
    segments: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for segment in segments {
        match segment {
            Segment::Static(text) | Segment::Raw(text) => output.push_str(text),
            Segment::Expr { expr, filters } => {
                eval_compiled_expr_into(expr, filters, scope, output)?;
            }
            Segment::ForLoop {
                binding,
                list_expr,
                body,
                else_body,
            } => {
                render_for_loop(
                    binding.as_ref(),
                    list_expr,
                    body,
                    else_body,
                    scope,
                    base_dir,
                    output,
                )?;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                render_if(branches, else_body, scope, base_dir, output)?;
            }
            Segment::Match {
                expr,
                arms,
                is_option,
            } => {
                render_match(expr, arms, *is_option, scope, base_dir, output)?;
            }
            Segment::Include(inc) => {
                render_include(inc, scope, base_dir, output)?;
            }
            Segment::Panic(segments) => {
                let msg = render_segments(segments, scope, base_dir)?;
                return Err(TemplateError::panic(msg));
            }
            Segment::Comment(_) => {
                // Comments produce no output.
            }
        }
    }
    Ok(())
}

/// Render segments into an existing output buffer (`no_std` variant).
///
/// Include directives are not supported and produce a runtime error.
#[cfg(not(feature = "std"))]
pub(crate) fn render_segments_into_no_std(
    segments: &[Segment],
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for segment in segments {
        match segment {
            Segment::Static(text) | Segment::Raw(text) => output.push_str(text),
            Segment::Expr { expr, filters } => {
                eval_compiled_expr_into(expr, filters, scope, output)?;
            }
            Segment::ForLoop {
                binding,
                list_expr,
                body,
                else_body,
            } => {
                render_for_loop_no_std(
                    binding.as_ref(),
                    list_expr,
                    body,
                    else_body,
                    scope,
                    output,
                )?;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                render_if_no_std(branches, else_body, scope, output)?;
            }
            Segment::Match {
                expr,
                arms,
                is_option,
            } => {
                render_match_no_std(expr, arms, *is_option, scope, output)?;
            }
            Segment::Include(inc) => {
                render_include_no_std(inc, scope, output)?;
            }
            Segment::Panic(segments) => {
                let mut msg = String::new();
                render_segments_into_no_std(segments, scope, &mut msg)?;
                return Err(TemplateError::panic(msg));
            }
            Segment::Comment(_) => {
                // Comments produce no output.
            }
        }
    }
    Ok(())
}

/// Render interpolated string segments (from `{{ expr }}` inside quoted strings)
/// against an **immutable** scope.
///
/// This is a lightweight renderer that handles only `Static`/`Raw` and `Expr`
/// segments — the only segment types produced by `compile_body` on a simple
/// interpolated string literal (no control flow tags allowed inside string
/// literals). Any other segment type is a compile-time logic error and will
/// produce a syntax error.
pub(crate) fn render_interpolated_str(
    segments: &[Segment],
    scope: &Scope<'_>,
) -> Result<String, TemplateError> {
    let mut output = String::new();
    for segment in segments {
        match segment {
            Segment::Static(text) | Segment::Raw(text) => output.push_str(text),
            Segment::Expr { expr, filters } => {
                eval_compiled_expr_into(expr, filters, scope, &mut output)?;
            }
            Segment::Comment(_) => {}
            _ => {
                return Err(TemplateError::syntax(
                    "control-flow tags are not allowed inside interpolated strings",
                ));
            }
        }
    }
    Ok(output)
}
