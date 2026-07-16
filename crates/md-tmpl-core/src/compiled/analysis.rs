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

/// Collect all unquoted case labels from match arms.
///
/// Used by the unused-param checker: if a declared param name appears as an
/// unquoted case label (e.g. `{% case expected %}`), the runtime reads that
/// param's value for comparison, so it counts as "used". The reference
/// collector already picks these up as variable references (correctly), but
/// the unused-param checker needs an explicit set to distinguish genuine
/// usage from false positives.
#[must_use]
pub fn collect_unquoted_case_labels(segments: &[Segment]) -> HashSet<String> {
    let mut labels = HashSet::new();
    collect_labels_inner(segments, &mut labels);
    labels
}

/// Recursive walker for unquoted case labels.
fn collect_labels_inner(segments: &[Segment], labels: &mut HashSet<String>) {
    for seg in segments {
        match seg {
            Segment::Match { arms, .. } => {
                for arm in arms {
                    for variant in &arm.variants {
                        if crate::consts::strip_string_literal(variant.as_ref()).is_none() {
                            labels.insert(variant.to_string());
                        }
                    }
                    collect_labels_inner(&arm.body, labels);
                }
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                collect_labels_inner(body, labels);
                collect_labels_inner(else_body, labels);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (_, branch_body) in branches {
                    collect_labels_inner(branch_body, labels);
                }
                collect_labels_inner(else_body, labels);
            }
            Segment::Panic(body) => {
                collect_labels_inner(body, labels);
            }
            _ => {}
        }
    }
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
                    // A quoted `with` value is a string literal that may embed
                    // `{{ expr }}` interpolation (see `include_core::build_overrides`);
                    // collect those references the same way body expressions are.
                    // An unquoted value is a plain expression (path/function/filter).
                    if let Some(inner) = crate::consts::strip_string_literal(val_expr.as_ref()) {
                        extract_interpolation_refs(inner, vars, loop_bindings);
                    } else if let Some(root) =
                        extract_root_variable(val_expr.as_ref(), loop_bindings)
                    {
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
                // Use extract_root_variable to handle kind(x) and other function calls.
                if let Some(root) = extract_root_variable(expr.as_str(), loop_bindings) {
                    vars.insert(root);
                }
                for arm in arms {
                    // Scan quoted case labels for {{ expr }} interpolation refs.
                    // Unquoted labels are either enum variant names or param-ref
                    // matches — the collector cannot distinguish them without
                    // type context, so param-ref labels are handled separately
                    // by the unused-param checker (which has the declared param set).
                    for variant in &arm.variants {
                        if let Some(inner) = crate::consts::strip_string_literal(variant.as_ref()) {
                            extract_interpolation_refs(inner, vars, loop_bindings);
                        }
                    }
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
            if !loop_bindings.contains(root) && !is_literal(root) && is_valid_identifier(root) {
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

    if is_literal(root) || loop_bindings.contains(root) || !is_valid_identifier(root) {
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

/// Returns `true` if `token` is a syntactically valid identifier
/// (`[A-Za-z_][A-Za-z0-9_]*`).
///
/// Mirrors the TypeScript reference implementation so both engines agree on
/// which tokens can name a variable. This rejects malformed roots such as a
/// fragment of a quoted path (`"web/about`) that results from splitting a
/// string literal on `.`, preventing spurious "undeclared variable" errors.
fn is_valid_identifier(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Extract variable references from `{{ expr }}` interpolations inside a string.
fn extract_interpolation_refs(
    s: &str,
    vars: &mut HashSet<String>,
    loop_bindings: &HashSet<String>,
) {
    let mut remaining: &str = s;
    while let Some(start) = remaining.find(crate::consts::EXPR_START) {
        remaining = &remaining[start + crate::consts::EXPR_START.len()..];
        if let Some(end) = remaining.find(crate::consts::EXPR_END) {
            let expr = &remaining[..end];
            if let Some(root) = extract_root_variable(expr, loop_bindings) {
                vars.insert(root);
            }
            remaining = &remaining[end + crate::consts::EXPR_END.len()..];
        } else {
            break;
        }
    }
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
                // Skip over string literals. A backslash escapes the next byte
                // so an escaped quote (`\"`/`\'`) does not close the literal,
                // matching `split_at_depth_zero`'s escape handling.
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
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
                // A backslash escapes the next byte so an escaped quote does
                // not close the literal (consistent with `split_at_depth_zero`).
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    // -- is_valid_identifier --------------------------------------------------

    #[test]
    fn valid_identifiers_accepted() {
        assert!(is_valid_identifier("name"));
        assert!(is_valid_identifier("_private"));
        assert!(is_valid_identifier("item2"));
        assert!(is_valid_identifier("CamelCase"));
    }

    #[test]
    fn invalid_identifiers_rejected() {
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("2cool"));
        assert!(!is_valid_identifier("has space"));
        assert!(!is_valid_identifier("a.b"));
        assert!(!is_valid_identifier("web/about"));
        // A quoted-path fragment produced by splitting a literal on `.`.
        assert!(!is_valid_identifier("\"web/about"));
    }

    // -- extract_root_variable ------------------------------------------------

    #[test]
    fn plain_variable_returns_root() {
        let lb = bindings(&[]);
        assert_eq!(extract_root_variable("name", &lb).as_deref(), Some("name"));
    }

    #[test]
    fn dotted_path_returns_first_segment() {
        let lb = bindings(&[]);
        assert_eq!(
            extract_root_variable("item.label", &lb).as_deref(),
            Some("item")
        );
    }

    #[test]
    fn prefixed_path_strips_prefix() {
        let lb = bindings(&[]);
        assert_eq!(
            extract_root_variable("consts.API_URL", &lb).as_deref(),
            Some("API_URL")
        );
        assert_eq!(
            extract_root_variable("params.count", &lb).as_deref(),
            Some("count")
        );
    }

    #[test]
    fn pipe_expression_uses_base() {
        let lb = bindings(&[]);
        assert_eq!(
            extract_root_variable("name | upper", &lb).as_deref(),
            Some("name")
        );
    }

    #[test]
    fn function_call_extracts_argument() {
        let lb = bindings(&[]);
        assert_eq!(
            extract_root_variable("idx(item)", &lb).as_deref(),
            Some("item")
        );
        assert_eq!(
            extract_root_variable("len(items)", &lb).as_deref(),
            Some("items")
        );
    }

    #[test]
    fn loop_binding_excluded() {
        let lb = bindings(&["item"]);
        assert_eq!(extract_root_variable("item.label", &lb), None);
        assert_eq!(extract_root_variable("idx(item)", &lb), None);
    }

    #[test]
    fn literals_return_none() {
        let lb = bindings(&[]);
        assert_eq!(extract_root_variable("\"literal\"", &lb), None);
        assert_eq!(extract_root_variable("'literal'", &lb), None);
        assert_eq!(extract_root_variable("42", &lb), None);
        assert_eq!(extract_root_variable("3.14", &lb), None);
        assert_eq!(extract_root_variable("true", &lb), None);
        assert_eq!(extract_root_variable("false", &lb), None);
    }

    #[test]
    fn string_literal_with_dots_is_not_a_variable() {
        // The core regression: a quoted path with dots must not be split at
        // the dot and reported as a variable named `"web/about`.
        let lb = bindings(&[]);
        assert_eq!(extract_root_variable("\"web/about.tmpl.md\"", &lb), None);
        assert_eq!(extract_root_variable("'a.b.c'", &lb), None);
        // Same guard inside a function argument.
        assert_eq!(extract_root_variable("len(\"a.b\")", &lb), None);
    }

    // -- extract_interpolation_refs -------------------------------------------

    #[test]
    fn interpolation_refs_collected_from_string() {
        let lb = bindings(&[]);
        let mut vars: HashSet<String> = HashSet::new();
        extract_interpolation_refs("{{ dir }}/about.md", &mut vars, &lb);
        assert!(vars.contains("dir"));
        assert_eq!(vars.len(), 1);
    }

    #[test]
    fn interpolation_refs_skip_loop_bindings_and_literals() {
        let lb = bindings(&["row"]);
        let mut vars: HashSet<String> = HashSet::new();
        extract_interpolation_refs("{{ row.name }}-{{ \"x.y\" }}-{{ total }}", &mut vars, &lb);
        assert!(vars.contains("total"));
        assert!(!vars.contains("row"));
        assert_eq!(vars.len(), 1);
    }
}
