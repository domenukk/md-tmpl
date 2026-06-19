//! Static variable reference analysis.
//!
//! Walks a compiled segment tree to collect the set of root variable
//! names that are referenced, excluding loop bindings and literals.
//! Used for unused-variable detection.

use alloc::string::{String, ToString};

use super::{ComparisonOp, Condition, Segment};
use crate::{
    compat::HashSet,
    consts::{BUILTIN_FUNCTIONS, LIT_FALSE, LIT_TRUE},
    error::TemplateError,
    scope::{CompiledExpr, CompiledPath, ConditionOperand},
};

/// Collect all root parameter names referenced in a compiled segment tree.
///
/// Walk a pre-compiled segment list to identify all root parameters referenced.
///
/// This is the static analysis engine that determines if a compiled template
/// is safe to run by verifying that all referenced parameters are in the
/// template's declared parameters.
#[must_use]
pub fn collect_referenced_params(segments: &[Segment]) -> HashSet<String> {
    let mut vars = HashSet::new();
    let mut loop_bindings = HashSet::new();
    collect_refs_inner(segments, &mut vars, &mut loop_bindings);
    vars
}

/// Recursive inner walker.
fn collect_refs_inner(
    segments: &[Segment],
    vars: &mut HashSet<String>,
    loop_bindings: &mut HashSet<String>,
) {
    for seg in segments {
        match seg {
            Segment::Static(_) | Segment::Raw(_) => {}
            Segment::Comment(refs) => {
                for r in refs {
                    vars.insert(r.to_string());
                }
            }
            Segment::Expr { expr, .. } => {
                extract_expr_variables(expr, vars, loop_bindings);
            }
            Segment::ForLoop {
                binding,
                list_path,
                body,
                else_body,
            } => {
                if let Some(root) = extract_path_variable(list_path, loop_bindings) {
                    vars.insert(root);
                }
                // The binding is local — exclude from "referenced" set.
                loop_bindings.insert(binding.to_string());
                collect_refs_inner(body, vars, loop_bindings);
                loop_bindings.remove(binding.as_ref());
                // else_body runs when the list is empty — the loop binding is NOT in scope.
                collect_refs_inner(else_body, vars, loop_bindings);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (condition, branch_body) in branches {
                    extract_condition_variables(condition, vars, loop_bindings);
                    collect_refs_inner(branch_body, vars, loop_bindings);
                }
                collect_refs_inner(else_body, vars, loop_bindings);
            }
            Segment::Include(inc) => {
                // Track the include path itself if it's a variable (higher-order template).
                // We assume it's a variable if it doesn't look like a file path.
                if !inc.path.ends_with(".tmpl.md") && !inc.path.ends_with(".md") {
                    if let Some(root) = extract_root_variable(inc.path.as_ref(), loop_bindings) {
                        vars.insert(root);
                    }
                }

                for (_, val_expr) in &inc.with_vars {
                    if let Some(root) = extract_root_variable(val_expr.as_ref(), loop_bindings) {
                        vars.insert(root);
                    }
                }
                if let Some((_, list_expr)) = &inc.for_each
                    && let Some(root) = extract_root_variable(list_expr.as_ref(), loop_bindings)
                {
                    vars.insert(root);
                }
            }
            Segment::Match { expr, arms } => {
                if let Some(root) = extract_path_variable(expr, loop_bindings) {
                    vars.insert(root);
                }
                for (_, arm_body) in arms {
                    collect_refs_inner(arm_body, vars, loop_bindings);
                }
            }
        }
    }
}

/// Extract the root variable name from an expression path, skipping
/// literals, function names, and loop bindings.
///
/// Examples:
/// - `"name"` → `Some("name")`
/// - `"item.label"` → `Some("item")` (but if `item` is a loop binding, `None`)
/// - `"idx(item)"` → extracts `item` as the arg, then checks loop bindings
/// - `"'literal'"` → `None`
/// - `"42"` → `None`
fn extract_root_variable(expr: &str, loop_bindings: &HashSet<String>) -> Option<String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    // Handle function calls: func(arg)
    if let Some(open) = expr.find(crate::consts::PAREN_OPEN)
        && expr.ends_with(crate::consts::PAREN_CLOSE)
    {
        let func_name = expr[..open].trim();
        if BUILTIN_FUNCTIONS.contains(&func_name) {
            let arg = expr[open + 1..expr.len() - 1].trim();
            let root = arg
                .split(crate::consts::PATH_SEP)
                .next()
                .unwrap_or(arg)
                .trim();
            if !root.is_empty() && !loop_bindings.contains(root) && !is_literal(root) {
                return Some(root.to_string());
            }
            return None;
        }
    }

    // Handle pipe expressions: take the part before the first `|`.
    let base = expr
        .split(crate::consts::PIPE)
        .next()
        .unwrap_or(expr)
        .trim();

    // Strip `.length` suffix if present (it's a pseudo-field, not a real one).
    let base = base
        .strip_suffix(crate::consts::PSEUDO_FIELD_LENGTH)
        .unwrap_or(base);

    let root = base
        .split(crate::consts::PATH_SEP)
        .next()
        .unwrap_or(base)
        .trim();

    if root.is_empty() || is_literal(root) || loop_bindings.contains(root) {
        return None;
    }

    Some(root.to_string())
}

/// Returns `true` if the token looks like a literal (string, number, bool).
fn is_literal(token: &str) -> bool {
    crate::consts::strip_string_literal(token).is_some()
        || token == LIT_TRUE
        || token == LIT_FALSE
        || token.parse::<i64>().is_ok()
        || token.parse::<f64>().is_ok()
}

/// Comparison operators used in condition strings, ordered longest-first
/// so `<=` / `>=` / `==` / `!=` are matched before `<` / `>`.
const COMPARISON_OPS: &[(&str, ComparisonOp)] = &[
    ("==", ComparisonOp::Eq),
    ("!=", ComparisonOp::Ne),
    ("<=", ComparisonOp::Le),
    (">=", ComparisonOp::Ge),
    ("<", ComparisonOp::Lt),
    (">", ComparisonOp::Gt),
];

/// Parse a raw condition string into a [`Condition`] at compile time.
///
/// Recognises:
/// - `left op right` — comparison (`==`, `!=`, `<`, etc.)
/// - `func(arg)` — builtin function truthiness (e.g. `has(x)`)
/// - `expr` — plain path truthiness
///
/// # Errors
///
/// Returns [`TemplateError`] if the condition operand cannot be parsed.
pub(super) fn parse_condition(condition: &str) -> Result<Condition, TemplateError> {
    let condition = condition.trim();

    for &(op_str, op) in COMPARISON_OPS {
        if let Some(idx) = condition.find(op_str) {
            let left_str = condition[..idx].trim();
            let right_str = condition[idx + op_str.len()..].trim();
            let left = ConditionOperand::compile(left_str)?;
            let right = ConditionOperand::compile(right_str)?;
            return Ok(Condition::Comparison { left, op, right });
        }
    }

    // Parse as a full ConditionOperand which handles function calls
    // (e.g. `has(x)`, `len(items)`) as well as plain paths.
    let operand = ConditionOperand::compile(condition)?;
    Ok(Condition::Truthy(operand))
}

fn extract_path_variable(path: &CompiledPath, loop_bindings: &HashSet<String>) -> Option<String> {
    let root = &path.parts()[0];
    if loop_bindings.contains(root) {
        None
    } else {
        Some(root.clone())
    }
}

fn extract_expr_variables(
    expr: &CompiledExpr,
    vars: &mut HashSet<String>,
    loop_bindings: &HashSet<String>,
) {
    match expr {
        CompiledExpr::Path(path)
        | CompiledExpr::Len(path)
        | CompiledExpr::Kind(path)
        | CompiledExpr::Has(path) => {
            if let Some(root) = extract_path_variable(path, loop_bindings) {
                vars.insert(root);
            }
        }
        CompiledExpr::Idx(_) => {}
    }
}

fn extract_operand_variables(
    operand: &ConditionOperand,
    vars: &mut HashSet<String>,
    loop_bindings: &HashSet<String>,
) {
    match operand {
        ConditionOperand::Literal(_) | ConditionOperand::Idx(_) => {}
        ConditionOperand::Path { path, .. }
        | ConditionOperand::Len(path)
        | ConditionOperand::Kind(path)
        | ConditionOperand::Has(path) => {
            if let Some(root) = extract_path_variable(path, loop_bindings) {
                vars.insert(root);
            }
        }
    }
}

/// Extract variable references from a pre-parsed condition.
fn extract_condition_variables(
    condition: &Condition,
    vars: &mut HashSet<String>,
    loop_bindings: &HashSet<String>,
) {
    match condition {
        Condition::Truthy(operand) => {
            extract_operand_variables(operand, vars, loop_bindings);
        }
        Condition::Comparison { left, right, .. } => {
            extract_operand_variables(left, vars, loop_bindings);
            extract_operand_variables(right, vars, loop_bindings);
        }
    }
}
