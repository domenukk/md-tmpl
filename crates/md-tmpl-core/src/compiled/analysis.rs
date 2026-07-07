//! Static variable reference analysis.
//!
//! Walks a compiled segment tree to collect the set of root variable
//! names that are referenced, excluding loop bindings and literals.
//! Used for unused-variable detection.

use alloc::{
    borrow::Cow,
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};

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
                list_expr,
                body,
                else_body,
            } => {
                extract_expr_variables(list_expr, vars, loop_bindings);
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
                if inc.path.contains(crate::consts::EXPR_START) {
                    let mut s: &str = inc.path.as_ref();
                    while let Some(start) = s.find(crate::consts::EXPR_START) {
                        s = &s[start + crate::consts::EXPR_START.len()..];
                        if let Some(end) = s.find(crate::consts::EXPR_END) {
                            let expr = &s[..end];
                            if let Some(root) = extract_root_variable(expr, loop_bindings) {
                                vars.insert(root);
                            }
                            s = &s[end + crate::consts::EXPR_END.len()..];
                        } else {
                            break;
                        }
                    }
                } else if !inc.path.ends_with(".tmpl.md") && !inc.path.ends_with(".md") {
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
            Segment::Match { expr, arms, .. } => {
                if let Some(root) = extract_path_variable(expr, loop_bindings) {
                    vars.insert(root);
                }
                for arm in arms {
                    if let Some(ref guard) = arm.guard {
                        extract_condition_variables(guard, vars, loop_bindings);
                    }
                    collect_refs_inner(&arm.body, vars, loop_bindings);
                }
            }
            Segment::Panic(segments) => {
                collect_refs_inner(segments, vars, loop_bindings);
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
            let mut arg = expr[open + 1..expr.len() - 1].trim();
            if let Some(s) = arg.strip_prefix(crate::consts::PREFIX_CONSTS_DOT) {
                arg = s.trim();
            } else if let Some(s) = arg.strip_prefix(crate::consts::PREFIX_OPTS_DOT) {
                arg = s.trim();
            } else if let Some(s) = arg.strip_prefix(crate::consts::PREFIX_OPTIONS_DOT) {
                arg = s.trim();
            } else if let Some(s) = arg.strip_prefix(crate::consts::PREFIX_PARAMS_DOT) {
                arg = s.trim();
            }
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
    let base = if let Some(s) = base.strip_prefix(crate::consts::PREFIX_CONSTS_DOT) {
        s.trim()
    } else if let Some(s) = base.strip_prefix(crate::consts::PREFIX_OPTS_DOT) {
        s.trim()
    } else if let Some(s) = base.strip_prefix(crate::consts::PREFIX_OPTIONS_DOT) {
        s.trim()
    } else if let Some(s) = base.strip_prefix(crate::consts::PREFIX_PARAMS_DOT) {
        s.trim()
    } else {
        base
    };

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

// ---------------------------------------------------------------------------
// Recursive descent condition parser
//
// Grammar:
//   condition  = or_expr
//   or_expr    = and_expr ( "||" and_expr )*
//   and_expr   = unary_expr ( "&&" unary_expr )*
//   unary_expr = "!" unary_expr | "(" or_expr ")" | primary
//   primary    = match_as_cond | comparison | truthiness
//   match_as_cond = "match" path "case" variant_list
//   comparison = operand cmp_op operand
//   truthiness = operand
// ---------------------------------------------------------------------------

/// Comparison operators in condition strings, ordered longest-first
/// so `<=` / `>=` / `==` / `!=` are matched before `<` / `>`.
const COMPARISON_OPS: &[(&str, ComparisonOp)] = &[
    (crate::consts::OP_EQ, ComparisonOp::Eq),
    (crate::consts::OP_NE, ComparisonOp::Ne),
    (crate::consts::OP_LE, ComparisonOp::Le),
    (crate::consts::OP_GE, ComparisonOp::Ge),
    (crate::consts::OP_LT, ComparisonOp::Lt),
    (crate::consts::OP_GT, ComparisonOp::Gt),
    (crate::consts::KW_IN_SPACED, ComparisonOp::In),
];

/// Parse a raw condition string into a [`Condition`] at compile time.
///
/// Supports `&&`, `||`, `!`, parenthesised grouping, comparison operators,
/// `x in y`, `match expr case Variant`, and plain truthiness.
///
/// # Errors
///
/// Returns [`TemplateError`] if the condition cannot be parsed.
pub(super) fn parse_condition(condition: &str) -> Result<Condition, TemplateError> {
    let condition = condition.trim();
    if condition.is_empty() {
        return Err(TemplateError::syntax("empty condition".to_string()));
    }
    parse_or_expr(condition)
}

/// Parse an `or_expr`: `and_expr ( "||" and_expr )*`.
fn parse_or_expr(s: &str) -> Result<Condition, TemplateError> {
    let parts = split_top_level(s, crate::consts::OP_OR)?;
    if parts.len() == 1 {
        return parse_and_expr(parts[0]);
    }
    let mut result = parse_and_expr(parts[0])?;
    for part in &parts[1..] {
        let right = parse_and_expr(part)?;
        result = Condition::Or(Box::new(result), Box::new(right));
    }
    Ok(result)
}

/// Parse an `and_expr`: `unary_expr ( "&&" unary_expr )*`.
fn parse_and_expr(s: &str) -> Result<Condition, TemplateError> {
    let parts = split_top_level(s, crate::consts::OP_AND)?;
    if parts.len() == 1 {
        return parse_unary_expr(parts[0]);
    }
    let mut result = parse_unary_expr(parts[0])?;
    for part in &parts[1..] {
        let right = parse_unary_expr(part)?;
        result = Condition::And(Box::new(result), Box::new(right));
    }
    Ok(result)
}

/// Parse a `unary_expr`: `"!" unary_expr | "(" or_expr ")" | primary`.
fn parse_unary_expr(s: &str) -> Result<Condition, TemplateError> {
    let s = s.trim();

    // Unary `!`: `!expr` or `!(expr)`
    if let Some(rest) = s.strip_prefix(crate::consts::OP_NOT) {
        let inner = parse_unary_expr(rest)?;
        return Ok(Condition::Not(Box::new(inner)));
    }

    // Parenthesised group: `(expr)`
    if s.starts_with(crate::consts::PAREN_OPEN) {
        if let Some(inner) = strip_balanced_parens(s) {
            return parse_or_expr(inner);
        }
        // `(` without matching `)` — produce clear error.
        return Err(TemplateError::syntax(alloc::format!(
            "unclosed '(' in condition: '{s}' — missing matching ')'"
        )));
    }

    parse_primary(s)
}

/// Parse a `primary`: match-as-condition, comparison, or truthiness.
fn parse_primary(s: &str) -> Result<Condition, TemplateError> {
    let s = s.trim();

    // Match-as-condition: `match expr case Variant | Variant`
    if let Some(rest) = s.strip_prefix(crate::consts::TAG_MATCH_PREFIX) {
        return parse_match_as_condition(rest.trim());
    }

    // Try to find a comparison operator.
    // We need to be careful: only split at top-level (not inside parens).
    for &(op_str, op) in COMPARISON_OPS {
        if let Some(idx) = find_top_level_op(s, op_str) {
            let left_str = s[..idx].trim();
            let right_str = s[idx + op_str.len()..].trim();
            let left = ConditionOperand::compile(left_str)?;
            let right = ConditionOperand::compile(right_str)?;
            return Ok(Condition::Comparison { left, op, right });
        }
    }

    // Truthiness
    let operand = ConditionOperand::compile(s)?;
    Ok(Condition::Truthy(operand))
}

/// Parse `expr case Variant [| Variant]*` into a `MatchVariant` condition.
fn parse_match_as_condition(s: &str) -> Result<Condition, TemplateError> {
    let case_pos = s.find(crate::consts::KW_CASE_SPACED).ok_or_else(|| {
        TemplateError::syntax(
            "match-as-condition: expected 'case' keyword after expression".to_string(),
        )
    })?;
    let expr_str = s[..case_pos].trim();
    let variant_str = s[case_pos + crate::consts::KW_CASE_SPACED.len()..].trim();
    if expr_str.is_empty() {
        return Err(TemplateError::syntax(
            "match-as-condition: empty expression".to_string(),
        ));
    }
    if variant_str.is_empty() {
        return Err(TemplateError::syntax(
            "match-as-condition: empty variant name after 'case'".to_string(),
        ));
    }
    let expr = CompiledPath::compile(expr_str);
    let variants: Vec<Cow<'static, str>> = variant_str
        .split(crate::consts::VARIANT_SEP)
        .map(|v| Cow::Owned(v.trim().to_string()))
        .collect();
    // is_option is determined by checking variant names (same as match blocks).
    let is_option = variants.iter().any(|v| {
        v.as_ref() == crate::consts::OPTION_SOME || v.as_ref() == crate::consts::OPTION_NONE
    });
    Ok(Condition::MatchVariant {
        expr,
        variants,
        is_option,
    })
}

/// Split `s` at top-level occurrences of `delim`, respecting parenthesis nesting.
///
/// Returns the sub-expressions as trimmed `&str` slices.
fn split_top_level<'a>(s: &'a str, delim: &str) -> Result<Vec<&'a str>, TemplateError> {
    let mut parts: Vec<&'a str> = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;
    let bytes = s.as_bytes();
    let delim_bytes = delim.as_bytes();
    let dlen = delim_bytes.len();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            crate::consts::PAREN_OPEN_BYTE => {
                depth += 1;
                i += 1;
            }
            crate::consts::PAREN_CLOSE_BYTE => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'\'' | b'"' => {
                // Skip over string literals
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // skip closing quote
                }
            }
            _ if depth == 0 && i + dlen <= bytes.len() && &bytes[i..i + dlen] == delim_bytes => {
                // Ensure the delimiter is surrounded by spaces for &&/||
                // (the grammar uses " && " and " || " with spaces)
                parts.push(s[start..i].trim());
                i += dlen;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }

    let last = s[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    } else if !parts.is_empty() {
        // A delimiter was found but nothing follows it — dangling operator.
        return Err(TemplateError::syntax(alloc::format!(
            "dangling '{delim}' operator: missing right-hand expression"
        )));
    }

    if parts.is_empty() {
        return Err(TemplateError::syntax(
            "empty expression in condition".to_string(),
        ));
    }

    Ok(parts)
}

/// Strip balanced outer parentheses from `s` if they match.
///
/// Returns `Some(inner)` if `s` starts with `(` and the matching `)` is the
/// last character. Returns `None` otherwise.
fn strip_balanced_parens(s: &str) -> Option<&str> {
    if !s.starts_with(crate::consts::PAREN_OPEN) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut depth: u32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            crate::consts::PAREN_OPEN_BYTE => depth += 1,
            crate::consts::PAREN_CLOSE_BYTE => {
                depth -= 1;
                if depth == 0 {
                    if i == bytes.len() - 1 {
                        return Some(&s[1..i]);
                    }
                    return None; // `)` is not at the end
                }
            }
            _ => {}
        }
    }
    None
}

/// Find a top-level occurrence of `op` in `s` (not inside parens or string
/// literals). Returns the byte index of the start of the operator.
fn find_top_level_op(s: &str, op: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let op_bytes = op.as_bytes();
    let olen = op_bytes.len();
    let mut depth: u32 = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            crate::consts::PAREN_OPEN_BYTE => {
                depth += 1;
                i += 1;
            }
            crate::consts::PAREN_CLOSE_BYTE => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            _ if depth == 0 && i + olen <= bytes.len() && &bytes[i..i + olen] == op_bytes => {
                return Some(i);
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

fn extract_path_variable(path: &CompiledPath, loop_bindings: &HashSet<String>) -> Option<String> {
    let mut root = path.parts()[0].as_str();
    if root == "consts" || root == "opts" || root == "options" || root == "params" {
        if path.parts().len() > 1 {
            root = path.parts()[1].as_str();
        } else {
            return None;
        }
    }
    if loop_bindings.contains(root) {
        None
    } else {
        Some(root.to_string())
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
        | CompiledExpr::Kinds(path)
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
    loop_bindings: &mut HashSet<String>,
) {
    match operand {
        ConditionOperand::Literal(_) | ConditionOperand::Idx(_) => {}
        ConditionOperand::InterpolatedStr(segments) => {
            let mut loop_bindings_clone = loop_bindings.clone();
            collect_refs_inner(segments, vars, &mut loop_bindings_clone);
        }
        ConditionOperand::Path { path, .. }
        | ConditionOperand::Len(path)
        | ConditionOperand::Kind(path)
        | ConditionOperand::Kinds(path)
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
    loop_bindings: &mut HashSet<String>,
) {
    match condition {
        Condition::Truthy(operand) => {
            extract_operand_variables(operand, vars, loop_bindings);
        }
        Condition::Not(inner) => {
            extract_condition_variables(inner, vars, loop_bindings);
        }
        Condition::Comparison { left, right, .. } => {
            extract_operand_variables(left, vars, loop_bindings);
            extract_operand_variables(right, vars, loop_bindings);
        }
        Condition::And(left, right) | Condition::Or(left, right) => {
            extract_condition_variables(left, vars, loop_bindings);
            extract_condition_variables(right, vars, loop_bindings);
        }
        Condition::MatchVariant { expr, .. } => {
            if let Some(root) = extract_path_variable(expr, loop_bindings) {
                vars.insert(root);
            }
        }
    }
}
