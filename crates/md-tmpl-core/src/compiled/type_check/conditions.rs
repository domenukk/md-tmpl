//! Condition validation, `in` comparisons, `has()` narrowing, and type
//! compatibility checks.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use super::{environment::TypeEnv, paths::validate_compiled_path};
use crate::{
    compiled::{
        ComparisonOp, Condition,
        type_resolve::{
            operand_to_str, resolve_compiled_path_type, resolve_operand_type, validate_operand,
        },
    },
    scope::ConditionOperand,
    types::{VarDecl, VarType, VariantDecl},
};

pub(super) fn validate_condition(
    condition: &Condition,
    env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    match condition {
        Condition::Truthy(operand) => {
            validate_operand(operand, env, errors);
        }
        Condition::Not(inner) => {
            validate_condition(inner, env, errors);
        }
        Condition::And(left, right) | Condition::Or(left, right) => {
            validate_condition(left, env, errors);
            validate_condition(right, env, errors);
        }
        Condition::Comparison { left, op, right } => {
            if matches!(op, ComparisonOp::In) {
                validate_in_comparison(left, right, env, errors);
                return;
            }

            let left_is_enum =
                resolve_operand_type(left, env).is_some_and(|ty| matches!(ty, VarType::Enum(_)));
            let right_is_enum =
                resolve_operand_type(right, env).is_some_and(|ty| matches!(ty, VarType::Enum(_)));

            if left_is_enum || right_is_enum {
                let enum_side = if left_is_enum { left } else { right };
                errors.push(format!(
                    "cannot compare enum '{}' with '==' — use {{% match %}} instead",
                    operand_to_str(enum_side)
                ));
                return;
            }

            validate_operand(left, env, errors);
            validate_operand(right, env, errors);
        }
        Condition::MatchVariant { expr, variants, .. } => {
            validate_compiled_path(expr, env, errors);
            // Validate the expression type and variant names.
            if let Some(resolved_type) = resolve_compiled_path_type(expr, env) {
                match resolved_type {
                    VarType::Enum(declared) => {
                        for v in variants {
                            let name = v.as_ref();
                            if name != crate::consts::MATCH_DEFAULT
                                && !declared.iter().any(|d| d.name == name)
                            {
                                let valid: Vec<&str> =
                                    declared.iter().map(|d| d.name.as_str()).collect();
                                errors.push(format!(
                                    "match-as-condition on '{}': unknown variant '{name}' \
                                     (declared variants: {})",
                                    expr.as_str(),
                                    valid.join(", ")
                                ));
                            }
                        }
                    }
                    VarType::Option(_) => {
                        // option(T) — only Some/None variants are valid.
                        for v in variants {
                            let name = v.as_ref();
                            if name != crate::consts::OPTION_SOME
                                && name != crate::consts::OPTION_NONE
                                && name != crate::consts::MATCH_DEFAULT
                            {
                                errors.push(format!(
                                    "match-as-condition on '{}': unknown variant '{name}' \
                                     (option type supports only 'Some' and 'None')",
                                    expr.as_str(),
                                ));
                            }
                        }
                    }
                    other => {
                        errors.push(format!(
                            "match-as-condition on '{}': expected enum or option type, got {other}",
                            expr.as_str(),
                        ));
                    }
                }
            }
        }
    }
}

fn in_types_compatible(a: &VarType, b: &VarType) -> bool {
    match (a, b) {
        (VarType::Str, VarType::Enum(_)) | (VarType::Enum(_), VarType::Str) => true,
        _ => types_compatible(a, b),
    }
}

fn resolve_operand_vartype(operand: &ConditionOperand, env: &TypeEnv<'_>) -> Option<VarType> {
    match operand {
        ConditionOperand::Literal(lit) => match lit {
            crate::value::Value::Str(_) => Some(VarType::Str),
            crate::value::Value::Int(_) => Some(VarType::Int),
            crate::value::Value::Float(_) => Some(VarType::Float),
            crate::value::Value::Bool(_) => Some(VarType::Bool),
            _ => None,
        },
        ConditionOperand::InterpolatedStr(_) | ConditionOperand::Kind(_) => Some(VarType::Str),
        ConditionOperand::Kinds(_) => Some(VarType::List(vec![VarDecl {
            name: String::new(),
            var_type: VarType::Str,
            default_value: None,
        }])),
        ConditionOperand::Len(_) | ConditionOperand::Idx(_) => Some(VarType::Int),
        ConditionOperand::Has(_) => Some(VarType::Bool),
        ConditionOperand::Path { path, .. } => resolve_compiled_path_type(path, env).cloned(),
    }
}

fn validate_in_comparison(
    left: &ConditionOperand,
    right: &ConditionOperand,
    env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    validate_operand(left, env, errors);
    validate_operand(right, env, errors);

    // Static enum variant check: if right is kinds(EnumPath), check left literal string(s).
    if let ConditionOperand::Kinds(path) = right {
        if let Some(VarType::Enum(variants)) = resolve_compiled_path_type(path, env) {
            if let ConditionOperand::Literal(crate::value::Value::Str(str_val)) = left {
                if !variants.iter().any(|v| v.name == *str_val) {
                    errors.push(format!(
                        "static string \"{str_val}\" is not a valid variant of enum '{}'",
                        path.as_str()
                    ));
                }
            }
        }
    }

    let left_ty = resolve_operand_vartype(left, env);
    let right_ty = resolve_operand_vartype(right, env);

    let (Some(l_ty), Some(r_ty)) = (&left_ty, &right_ty) else {
        return;
    };

    match r_ty {
        VarType::Str => {
            let valid = match l_ty {
                VarType::Str => true,
                VarType::List(fields)
                    if !fields.is_empty() && fields[0].var_type == VarType::Str =>
                {
                    true
                }
                _ => false,
            };
            if !valid {
                errors.push(format!(
                    "type mismatch for 'in': checking substring in string requires string or list of strings on left, got {l_ty}"
                ));
            }
        }
        VarType::List(fields) => {
            let elem_ty = if fields.is_empty() {
                &VarType::Str // fallback for empty list
            } else {
                &fields[0].var_type
            };
            match l_ty {
                VarType::List(left_fields) => {
                    let left_elem = if left_fields.is_empty() {
                        &VarType::Str
                    } else {
                        &left_fields[0].var_type
                    };
                    if !in_types_compatible(left_elem, elem_ty) {
                        errors.push(format!(
                            "list element type mismatch in subset check: expected list of {elem_ty}, got list of {left_elem}"
                        ));
                    }
                }
                scalar => {
                    if !in_types_compatible(scalar, elem_ty) {
                        errors.push(format!(
                            "element type mismatch for 'in': expected {elem_ty}, got {scalar}"
                        ));
                    }
                }
            }
        }
        other => {
            errors.push(format!(
                "cannot use 'in' with right operand '{}': expected list or string, got {other}",
                operand_to_str(right)
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// has() flow-sensitive narrowing
// ---------------------------------------------------------------------------

/// If the condition is `has(path)` and `path` resolves to an option type,
/// return `(path_str, narrowed_type)` where `narrowed_type` is the inner
/// type of the option (transparent unwrap).
///
/// This enables `{% if has(x) %} {{ x }} {% /if %}` to type-check.
fn extract_has_narrowing(condition: &Condition, env: &TypeEnv<'_>) -> Option<(String, VarType)> {
    let Condition::Truthy(ConditionOperand::Has(path)) = condition else {
        return None;
    };

    let path_str = path.as_str();

    // Resolve the type for this path.
    let ty = resolve_compiled_path_type(path, env)?;

    if !ty.is_option() {
        return None;
    }

    // Narrow option to its inner type.
    match ty {
        // New-style option(T): unwrap to T directly.
        VarType::Option(inner) => Some((path_str.to_string(), inner.as_ref().clone())),
        // Legacy enum-based option: extract just the Some variant.
        VarType::Enum(variants) => {
            let some_only: Vec<VariantDecl> = variants
                .iter()
                .filter(|v| v.name == crate::consts::OPTION_SOME)
                .cloned()
                .collect();
            if some_only.is_empty() {
                return None;
            }
            Some((path_str.to_string(), VarType::Enum(some_only)))
        }
        _ => None,
    }
}

/// Extract all has()-based narrowings from a condition tree.
///
/// For `&&` chains like `has(a) && has(b)`, this returns both narrowings.
/// For `||` and `!`, no narrowings are extracted (they would be unsound).
pub(super) fn extract_all_has_narrowings(
    condition: &Condition,
    env: &TypeEnv<'_>,
) -> Vec<(String, VarType)> {
    let mut narrowings = Vec::new();
    collect_and_narrowings(condition, env, &mut narrowings);
    narrowings
}

/// Recursively collect has()-based narrowings from `&&` chains.
fn collect_and_narrowings(
    condition: &Condition,
    env: &TypeEnv<'_>,
    out: &mut Vec<(String, VarType)>,
) {
    match condition {
        Condition::And(left, right) => {
            collect_and_narrowings(left, env, out);
            collect_and_narrowings(right, env, out);
        }
        other => {
            if let Some(narrowing) = extract_has_narrowing(other, env) {
                out.push(narrowing);
            }
        }
    }
}

/// If a branch condition is exactly `!has(path)`, then in
/// every fall-through position (any later branch and the final `{% else %}`)
/// the option is guaranteed present. Return the same `(path_str, inner_type)`
/// narrowing that a positive `has(path)` would produce.
///
/// Only the single, top-level negation form is handled: compound conditions
/// like `!has(a) && !has(b)` cannot be soundly narrowed on fall-through, so
/// they contribute nothing.
pub(super) fn extract_not_has_narrowing(
    condition: &Condition,
    env: &TypeEnv<'_>,
) -> Option<(String, VarType)> {
    let Condition::Not(inner) = condition else {
        return None;
    };
    extract_has_narrowing(inner, env)
}

/// Check if two types are compatible for include parameter passing.
///
/// Types are compatible if they are structurally equal. Containers with
/// empty field lists (which may arise internally) are treated as compatible
/// with any same-kind type. Note: untyped `list()` and `struct()` are
/// rejected at parse time, so this case only applies to internal types.
pub(super) fn types_compatible(provided: &VarType, expected: &VarType) -> bool {
    match (provided, expected) {
        // Exact scalar match.
        (VarType::Str, VarType::Str)
        | (VarType::Int, VarType::Int)
        | (VarType::Float, VarType::Float)
        | (VarType::Bool, VarType::Bool) => true,

        // Untyped containers are compatible with any same-kind type.
        (VarType::List(a), VarType::List(b)) | (VarType::Struct(a), VarType::Struct(b)) => {
            a.is_empty() || b.is_empty() || a == b
        }

        // Enum types: compare variant names and field types.
        (VarType::Enum(a), VarType::Enum(b)) => a == b,

        // Option types: compatible if inner types are compatible.
        (VarType::Option(a), VarType::Option(b)) => types_compatible(a, b),

        // Template types: compare signatures.
        (VarType::Tmpl(a), VarType::Tmpl(b)) => a == b,

        _ => false,
    }
}
