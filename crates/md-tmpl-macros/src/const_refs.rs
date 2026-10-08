//! AST walker to collect constant and imported-constant keys referenced by
//! pre-compiled template segments (`Segment`).
//!
//! Used by `codegen` to prune unreferenced `consts` and `imported_consts` from
//! the runtime `Value` maps inside `__TEMPLATE` and `CompiledInlineTemplate`,
//! while keeping top-level Rust `pub const` / `LazyLock` items untouched for
//! direct Rust callers.

use std::collections::HashSet;

use md_tmpl_core::compiled::{CompiledExpr, CompiledPath, Condition, ConditionOperand, Segment};

/// Collect all root variable names (`p0`) and 2-part dotted keys (`p0.p1`)
/// referenced anywhere in `segments` (including nested includes and `with`/`for` clauses).
pub(crate) fn collect_referenced_const_keys(segments: &[Segment]) -> HashSet<String> {
    let mut keys = HashSet::new();
    collect_keys_from_segments(segments, &mut keys);
    keys
}

fn record_path_parts(parts: &[String], keys: &mut HashSet<String>) {
    if let Some(first) = parts.first() {
        keys.insert(first.clone());
        if let Some(second) = parts.get(1) {
            keys.insert(format!("{first}.{second}"));
        }
    }
}

fn record_path_keys(path: &CompiledPath, keys: &mut HashSet<String>) {
    let parts = path.parts();
    record_path_parts(parts, keys);
    // Also record stripped keys when the path uses `consts.` / `params.` / `opts.` / `options.`
    if let Some(first) = parts.first().map(String::as_str)
        && matches!(first, "consts" | "params" | "opts" | "options")
    {
        record_path_parts(&parts[1..], keys);
    }
}

fn record_head_path(trimmed: &str, keys: &mut HashSet<String>) {
    let head = trimmed
        .split(md_tmpl_core::consts::PIPE)
        .next()
        .unwrap_or(trimmed)
        .trim();
    let path = CompiledPath::compile(head);
    record_path_keys(&path, keys);
}

fn record_raw_expr_keys(raw: &str, keys: &mut HashSet<String>) {
    let trimmed = raw.trim();
    if let Some(inner) = md_tmpl_core::consts::strip_string_literal(trimmed) {
        let unescaped = md_tmpl_core::consts::unescape_string_literal(inner);
        if unescaped.contains(md_tmpl_core::consts::EXPR_START) {
            match md_tmpl_core::compiled::compile_body(&unescaped) {
                Ok(segs) => collect_keys_from_segments(&segs, keys),
                Err(err) => {
                    assert!(!err.to_string().is_empty());
                    record_head_path(&unescaped, keys);
                }
            }
        }
        return;
    }
    if trimmed.contains(md_tmpl_core::consts::EXPR_START) {
        match md_tmpl_core::compiled::compile_body(trimmed) {
            Ok(segs) => collect_keys_from_segments(&segs, keys),
            Err(err) => {
                assert!(!err.to_string().is_empty());
                record_head_path(trimmed, keys);
            }
        }
        return;
    }
    match ConditionOperand::compile(trimmed) {
        Ok(op) => collect_keys_from_operand(&op, keys),
        Err(err) => {
            assert!(!err.to_string().is_empty());
            record_head_path(trimmed, keys);
        }
    }
}

fn collect_keys_from_expr(expr: &CompiledExpr, keys: &mut HashSet<String>) {
    match expr {
        CompiledExpr::Path(p)
        | CompiledExpr::Len(p)
        | CompiledExpr::Kind(p)
        | CompiledExpr::Kinds(p)
        | CompiledExpr::Has(p) => record_path_keys(p, keys),
        CompiledExpr::Idx(_) | CompiledExpr::Literal(_) => {}
    }
}

fn collect_keys_from_operand(op: &ConditionOperand, keys: &mut HashSet<String>) {
    match op {
        ConditionOperand::Path { path, .. }
        | ConditionOperand::Len(path)
        | ConditionOperand::Kind(path)
        | ConditionOperand::Kinds(path)
        | ConditionOperand::Has(path) => record_path_keys(path, keys),
        ConditionOperand::InterpolatedStr(segs) => collect_keys_from_segments(segs, keys),
        ConditionOperand::Literal(_) | ConditionOperand::Idx(_) => {}
    }
}

fn collect_keys_from_condition(cond: &Condition, keys: &mut HashSet<String>) {
    match cond {
        Condition::Truthy(op) => collect_keys_from_operand(op, keys),
        Condition::Not(inner) => collect_keys_from_condition(inner, keys),
        Condition::And(l, r) | Condition::Or(l, r) => {
            collect_keys_from_condition(l, keys);
            collect_keys_from_condition(r, keys);
        }
        Condition::Comparison { left, right, .. } => {
            collect_keys_from_operand(left, keys);
            collect_keys_from_operand(right, keys);
        }
        Condition::MatchVariant { expr, variants, .. } => {
            record_path_keys(expr, keys);
            for v in variants {
                record_raw_expr_keys(v, keys);
            }
        }
    }
}

pub(crate) fn collect_keys_from_segments(segments: &[Segment], keys: &mut HashSet<String>) {
    for seg in segments {
        match seg {
            Segment::Static(_) | Segment::Raw(_) | Segment::Comment(_) => {}
            Segment::Expr { expr, .. } => collect_keys_from_expr(expr, keys),
            Segment::ForLoop {
                list_expr,
                body,
                else_body,
                ..
            } => {
                collect_keys_from_expr(list_expr, keys);
                collect_keys_from_segments(body, keys);
                collect_keys_from_segments(else_body, keys);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (cond, body) in branches {
                    collect_keys_from_condition(cond, keys);
                    collect_keys_from_segments(body, keys);
                }
                collect_keys_from_segments(else_body, keys);
            }
            Segment::Match { expr, arms, .. } => {
                record_path_keys(expr, keys);
                for arm in arms {
                    for v in &arm.variants {
                        record_raw_expr_keys(v, keys);
                    }
                    if let Some(ref g) = arm.guard {
                        collect_keys_from_condition(g, keys);
                    }
                    collect_keys_from_segments(&arm.body, keys);
                }
            }
            Segment::Panic(segs) => collect_keys_from_segments(segs, keys),
            Segment::Include(inc) => {
                record_raw_expr_keys(inc.path.as_ref(), keys);
                for (_, val_expr) in &inc.with_vars {
                    record_raw_expr_keys(val_expr, keys);
                }
                if let Some((_, list_expr)) = &inc.for_each {
                    record_raw_expr_keys(list_expr, keys);
                }
                if let Some(ref ic) = inc.inline_compiled {
                    collect_keys_from_segments(&ic.segments, keys);
                }
            }
        }
    }
}
