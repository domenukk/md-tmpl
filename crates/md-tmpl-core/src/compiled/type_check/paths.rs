//! Path validation and field resolution.
//!
//! Validates dotted paths like `outcome.evidence` against the type
//! environment, resolving each field access against the current (possibly
//! narrowed) type and producing helpful diagnostics on failure.

use alloc::{string::String, vec::Vec};

use super::environment::TypeEnv;
use crate::{
    scope::CompiledPath,
    types::{VarType, VariantDecl},
};

/// Validate a dotted path like `outcome.evidence` against the type env.
pub(super) fn validate_path(path: &str, env: &TypeEnv<'_>, errors: &mut Vec<String>) {
    // Skip function calls like `idx(item)`, `len(list)`, `str(x)`.
    if path.contains(crate::consts::PAREN_OPEN) {
        return;
    }
    // Skip string literals.
    if crate::consts::strip_string_literal(path).is_some() {
        return;
    }
    // Skip numeric literals.
    if path.bytes().next().is_some_and(|b| b.is_ascii_digit()) {
        return;
    }
    let compiled = CompiledPath::compile(path);
    validate_compiled_path(&compiled, env, errors);
}

pub(crate) fn validate_compiled_path(
    path: &CompiledPath,
    env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    let root = &path.parts()[0];

    let Some(root_type) = env.lookup(root) else {
        // Import stems and const names are opaque — valid but not typed.
        if env.is_opaque(root) {
            return;
        }
        errors.push(format!("'{root}': undeclared variable"));
        return;
    };

    let mut current_type = root_type;
    let mut traversed = root.clone();

    for field in &path.parts()[1..] {
        traversed.push(crate::consts::PATH_SEP);
        traversed.push_str(field);

        // Check narrowed overrides first (e.g. "task.cat" narrowed by match).
        if let Some(narrowed) = env.narrowed.get(&traversed) {
            current_type = narrowed;
            continue;
        }

        if let VarType::Enum(variants) = current_type {
            if variants.iter().any(|v| v.name == *field) {
                continue;
            }
        }

        match resolve_field(current_type, field) {
            FieldResult::Ok(ty) => {
                current_type = ty;
            }
            FieldResult::NotAvailable { reason } => {
                if let VarType::Option(_) = current_type {
                    errors.push(format!(
                        "'{traversed}': cannot access field '{field}' on {current_type} — \
                         option values must be checked before access. \
                         Try guard: {{% if has({traversed}) %}}{{{{{traversed}.{field}}}}}{{% /if %}} or \
                         match: {{% match {traversed} %}}{{% case Some %}}{{{{{traversed}.{field}}}}}{{% /match %}}"
                    ));
                } else {
                    errors.push(format!("'{traversed}': {reason}"));
                }
                return; // Stop checking deeper fields.
            }
            FieldResult::Terminal => {
                // Reached a leaf type (e.g. .tag → str). No deeper access possible.
                return;
            }
        }
    }
}

/// Result of resolving a field on a type.
pub(crate) enum FieldResult<'a> {
    /// Field found, here's the resolved type (borrowed from the `VarType` tree).
    Ok(&'a VarType),
    /// Field not available — with a human-readable reason.
    NotAvailable { reason: String },
    /// Reached a terminal/leaf type — stop resolving deeper but no error.
    /// Used for built-in fields like `.tag` that return a known scalar.
    Terminal,
}

/// Resolve a single field access on a type.
pub(crate) fn resolve_field<'a>(ty: &'a VarType, field: &str) -> FieldResult<'a> {
    match ty {
        VarType::Enum(variants) => resolve_enum_field(variants, field),

        VarType::Struct(fields) => {
            if fields.is_empty() {
                // Untyped dict (no declared fields) — allow any field.
                FieldResult::Terminal
            } else if let Some(d) = fields.iter().find(|d| d.name == field) {
                FieldResult::Ok(&d.var_type)
            } else {
                let mut declared: Vec<&str> = fields.iter().map(|d| d.name.as_str()).collect();
                declared.sort_unstable();
                let mut best: Option<(&str, usize)> = None;
                for candidate in &declared {
                    let dist = crate::error::levenshtein_distance(field, candidate);
                    if dist > 0 && dist <= 2 && best.is_none_or(|b| dist < b.1) {
                        best = Some((*candidate, dist));
                    }
                }
                let suggestion = best
                    .map(|(s, _)| format!(" Did you mean '{s}'?"))
                    // NOLINT: None means no close match found — empty suggestion is intentional
                    .unwrap_or_default();
                FieldResult::NotAvailable {
                    reason: format!(
                        "field '{field}' does not exist on dict.{suggestion} \
                         (declared fields: {})",
                        declared.join(", ")
                    ),
                }
            }
        }

        VarType::List(fields) => {
            // List itself doesn't support field access — iterate first.
            if fields.is_empty() {
                FieldResult::Terminal
            } else {
                let declared: Vec<&str> = fields.iter().map(|d| d.name.as_str()).collect();
                FieldResult::NotAvailable {
                    reason: format!(
                        "cannot access field '{field}' on list — \
                         use {{% for %}} to iterate (element fields: {})",
                        declared.join(", ")
                    ),
                }
            }
        }

        // Scalars and templates have no fields — always an error.
        VarType::Str | VarType::Int | VarType::Float | VarType::Bool | VarType::Tmpl(_) => {
            FieldResult::NotAvailable {
                reason: format!("cannot access field '{field}' on {ty}"),
            }
        }

        // option(T) — field access on an option is not valid; use has() + match.
        VarType::Option(_) => FieldResult::NotAvailable {
            reason: format!(
                "cannot access field '{field}' on {ty} — \
                 use {{% if has(...) %}} or {{% match %}} to unwrap first"
            ),
        },
    }
}

/// Resolve a field on an enum type.
///
/// A field is accessible **only** if it exists on **all** active (narrowed)
/// variants. Use `str(expr)` to get the variant name as a string.
fn resolve_enum_field<'a>(variants: &'a [VariantDecl], field: &str) -> FieldResult<'a> {
    if variants.is_empty() {
        return FieldResult::Terminal;
    }

    // Check that ALL active variants have this field.
    let mut resolved_type: Option<&VarType> = None;
    let mut missing_on: Vec<&str> = Vec::new();

    for variant in variants {
        match variant.fields.iter().find(|d| d.name == field) {
            Some(decl) => {
                resolved_type = Some(&decl.var_type);
            }
            None => {
                missing_on.push(&variant.name);
            }
        }
    }

    if missing_on.is_empty() {
        // All variants have the field.
        match resolved_type {
            Some(ty) => FieldResult::Ok(ty),
            None => FieldResult::Terminal, // shouldn't happen — but be safe.
        }
    } else if missing_on.len() == variants.len() {
        // No variant has this field.
        let mut all_fields: Vec<&str> = variants
            .iter()
            .flat_map(|v| v.fields.iter().map(|f| f.name.as_str()))
            .collect();
        all_fields.sort_unstable();
        all_fields.dedup();
        let mut best: Option<(&str, usize)> = None;
        for candidate in &all_fields {
            let dist = crate::error::levenshtein_distance(field, candidate);
            if dist > 0 && dist <= 2 && best.is_none_or(|b| dist < b.1) {
                best = Some((*candidate, dist));
            }
        }
        let suggestion = best
            .map(|(s, _)| format!(" Did you mean '{s}'?"))
            // NOLINT: None means no close match found — empty suggestion is intentional
            .unwrap_or_default();
        let variant_names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
        FieldResult::NotAvailable {
            reason: format!(
                "field '{field}' does not exist on any variant.{suggestion} ({})",
                variant_names.join(", ")
            ),
        }
    } else {
        // Some variants have it, some don't.
        let hint = if variants.len() > 1 {
            format!(
                "field '{field}' is not available on variant(s) {} — \
                 use {{% match %}} to narrow the type first",
                missing_on.join(", ")
            )
        } else {
            format!(
                "field '{field}' is not available on variant '{}'",
                missing_on[0]
            )
        };
        FieldResult::NotAvailable { reason: hint }
    }
}
