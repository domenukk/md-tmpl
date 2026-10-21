//! Include directive validation.
//!
//! Cross-boundary type checking for `{% include %}`: parent-scope expression
//! validation, param contract enforcement, `with`-value type matching, and a
//! cycle-safe walk of the included template body.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use super::{
    conditions::types_compatible, environment::TypeEnv, paths::validate_path, walker::walk_segments,
};
use crate::{
    compat::HashSet,
    compiled::{CompiledInclude, type_resolve::resolve_path_type},
    types::VarDecl,
};

/// Validate an include directive with full cross-boundary type checking:
///
/// 1. Check that `with` / `for` expressions are valid in the parent scope.
/// 2. If the included template's compiled body is available (`inline_compiled`):
///    a. **Contract**: all declared params must be provided.
///    b. **Type matching**: `with` value types must match the included template's
///    declared types.
///    c. **Body walk**: recursively validate the included body — with cycle
///    detection to handle recursive templates.
pub(super) fn validate_include(
    inc: &CompiledInclude,
    env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    // 1. Check that `with` expressions are valid in parent scope. Only the base
    //    operand is a path; a trailing `| filter` chain is not part of it, and
    //    scalar literals (`true`, `5`, `"x"`) need no lookup.
    for (_, val_expr) in &inc.with_vars {
        validate_with_operand(val_expr, env, errors);
    }
    if let Some((_, list_expr)) = &inc.for_each {
        validate_with_operand(list_expr, env, errors);
    }

    // 2. Cross-boundary checks when the included template is available.
    let Some(compiled) = &inc.inline_compiled else {
        return;
    };

    // 2a. Contract: all declared params must be provided.
    validate_include_contract(inc, &compiled.declarations, errors);

    // 2b. Type matching: check that provided types match expected types.
    validate_include_type_match(inc, &compiled.declarations, env, errors);

    // 2c. Walk the included body — but only if not a cycle.
    // Use Arc pointer as identity: same file/content = same Arc = deduped,
    // different files with the same name = different Arcs = both walked.
    let identity_key = format!(
        "{}@{:p}",
        inc.path,
        alloc::sync::Arc::as_ptr(&compiled.segments)
    );
    if visited.insert(identity_key.clone()) {
        // First visit: build child TypeEnv from included template's declarations.
        let mut child_env = TypeEnv::from_declarations(&compiled.declarations);
        // Propagate opaque roots — included templates may also reference
        // imported constants (e.g. `config.NOTEBOOK_FILENAME`) from their own imports.
        child_env.opaque_roots.clone_from(&env.opaque_roots);
        for k in compiled.imported_consts.keys() {
            child_env.opaque_roots.insert(k.clone());
            // Register the import stem (the segment before the first `.`) as
            // an opaque root too. The parent may register a stem with typed
            // const info as a (non-opaque) declaration, which — being a
            // borrowed, lifetime-bound type — is not propagated across the
            // include boundary. Without the bare stem, paths like
            // `artist.SEVERITY_LADDER` (and for-loops over them) inside the
            // inlined body would be flagged as undeclared.
            if let Some((stem, _)) = k.split_once(crate::consts::PATH_SEP) {
                child_env.opaque_roots.insert(stem.to_string());
            }
        }
        for k in compiled.consts.keys() {
            child_env.opaque_roots.insert(k.clone());
        }
        walk_segments(&compiled.segments, &mut child_env, errors, visited);
        visited.remove(&identity_key);
    }
    // Cycle: boundary checks (2a, 2b) were done — body already validated on
    // first encounter, so we skip recursing to avoid infinite loops.
}

/// Validate the base operand of a `with` / `for` expression (`base | filters`).
fn validate_with_operand(expr: &str, env: &TypeEnv<'_>, errors: &mut Vec<String>) {
    let (base, _) = crate::parser::split_pipe_aware(expr.trim());
    let base = base.trim();
    if crate::parser::parse_scalar_literal(base).is_some() {
        return;
    }
    validate_path(base, env, errors);
}

/// Find which declared parameters are NOT provided by the include directive.
///
/// Shared between compile-time (`type_check`) and runtime (`include.rs`)
/// contract checking. Returns references to missing declarations.
pub(crate) fn find_missing_include_params<I>(
    declarations: &[VarDecl],
    provided_keys: I,
) -> Vec<&VarDecl>
where
    I: Iterator<Item: AsRef<str>>,
{
    // For typical 1–5 params, linear scan beats HashSet overhead.
    let provided: Vec<String> = provided_keys.map(|k| k.as_ref().to_string()).collect();
    declarations
        .iter()
        .filter(|d| d.default_value.is_none() && !provided.iter().any(|p| p == &d.name))
        .collect()
}

/// Check that all declared parameters in the included template are provided
/// via `with` or `for`. Pushes formatted errors into the error list.
fn validate_include_contract(
    inc: &CompiledInclude,
    included_declarations: &[VarDecl],
    errors: &mut Vec<String>,
) {
    let provided = inc
        .with_vars
        .iter()
        .map(|(k, _)| k.as_ref().to_string())
        .chain(inc.for_each.iter().map(|(b, _)| b.as_ref().to_string()));

    let missing = find_missing_include_params(included_declarations, provided);
    if !missing.is_empty() {
        // Single-pass: collect descriptions and fix hints together.
        let (descs, hints): (Vec<_>, Vec<_>) = missing
            .iter()
            .map(|d| {
                (
                    format!("{}: {}", d.name, d.var_type),
                    format!("{}={}", d.name, d.name),
                )
            })
            .unzip();
        errors.push(format!(
            "include '{}': missing required param(s): {}. \
             Use 'with {}' to pass them",
            inc.path,
            descs.join(", "),
            hints.join(", "),
        ));
    }
}

/// Type-check that provided `with` variables have compatible types with
/// the included template's declarations.
fn validate_include_type_match(
    inc: &CompiledInclude,
    included_declarations: &[VarDecl],
    parent_env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    for (key, val_expr) in &inc.with_vars {
        let Some(included_decl) = included_declarations
            .iter()
            .find(|d| d.name == key.as_ref())
        else {
            continue; // Extra vars are OK (not declared in included template).
        };

        // Skip literals (checked at render time against the declaration) and
        // filtered expressions (the filter may change the type).
        let val = val_expr.trim();
        let (base, filters) = crate::parser::split_pipe_aware(val);
        if !filters.trim().is_empty()
            || crate::parser::parse_scalar_literal(base.trim()).is_some()
            || val.starts_with(crate::consts::ANGLE_OPEN)
        {
            continue;
        }

        // Resolve the type of the expression in the parent env.
        if let Some(parent_type) = resolve_path_type(val, parent_env) {
            if !types_compatible(parent_type, &included_decl.var_type) {
                errors.push(format!(
                    "include '{}': type mismatch for '{}': \
                     parent provides '{}' but included template expects '{}'",
                    inc.path, key, parent_type, included_decl.var_type,
                ));
            }
        }
        // If unresolvable, validate_path already reported the error.
    }
}
