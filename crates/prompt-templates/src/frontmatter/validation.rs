//! Validation rules for parsed frontmatter.
//!
//! Checks collision rules between params, type aliases, and imports,
//! and generates implicit type aliases for compound parameter types.

use super::Frontmatter;
use crate::{error::TemplateError, types::VarType};

/// Validate frontmatter collision rules.
///
/// Rules checked:
/// - R1: No param name matches a type alias name (`PascalCase` comparison),
///   unless the param's type IS that alias.
/// - R2: No type alias shadows an import stem.
/// - R2b: No param name (`PascalCase`) shadows an import stem.
/// - R4: No unused type aliases (explicitly declared types that are never
///   referenced by any param declaration).
pub(crate) fn validate_collision_rules(fm: &Frontmatter) -> Result<(), TemplateError> {
    // R1: Type alias vs param/const name collision (PascalCase).
    for decl in fm.declarations.iter().chain(fm.consts.iter()) {
        let decl_pascal = crate::types::to_pascal_case(&decl.name);
        for (alias_name, alias_type) in &fm.type_aliases {
            if decl_pascal == *alias_name {
                // Allow if the declaration's type is exactly this alias.
                if decl.var_type == *alias_type {
                    continue;
                }
                let label = if fm.consts.contains(decl) {
                    "constant"
                } else {
                    "param"
                };
                return Err(TemplateError::syntax(format!(
                    "{}: {label} '{}' (PascalCase: '{}') conflicts with type alias '{}'",
                    crate::consts::ERR_TYPE_PARAM_CONFLICT,
                    decl.name,
                    decl_pascal,
                    alias_name,
                )));
            }
        }
    }

    // R3: param name vs const name (exact match).
    // Both occupy the same runtime scope, so a const would silently
    // shadow the param value provided by the caller.
    for param in &fm.declarations {
        for cst in &fm.consts {
            if param.name == cst.name {
                return Err(TemplateError::syntax(format!(
                    "{}: '{}' is declared as both a param and a constant",
                    crate::consts::ERR_PARAM_CONST_CONFLICT,
                    param.name,
                )));
            }
        }
    }

    // R2: Type alias shadows import stem.
    for import in &fm.imports {
        for alias_name in fm.type_aliases.keys() {
            if alias_name == &import.stem {
                return Err(TemplateError::syntax(format!(
                    "{}: '{}' shadows '{}'",
                    crate::consts::ERR_TYPE_SHADOWS_IMPORT,
                    alias_name,
                    import.stem,
                )));
            }
        }
    }

    // R2b: Param/const name (PascalCase) shadows import stem.
    for import in &fm.imports {
        for decl in fm.declarations.iter().chain(fm.consts.iter()) {
            let decl_pascal = crate::types::to_pascal_case(&decl.name);
            if decl_pascal == import.stem {
                let label = if fm.consts.contains(decl) {
                    "constant"
                } else {
                    "param"
                };
                return Err(TemplateError::syntax(format!(
                    "{}: {label} '{}' (PascalCase: '{}') shadows import '{}'",
                    crate::consts::ERR_PARAM_SHADOWS_IMPORT,
                    decl.name,
                    decl_pascal,
                    import.stem,
                )));
            }
        }
    }
    // R4: Unused type alias — any explicitly declared types: entry that is never
    // referenced by any param declaration. Skipped when `allow_unused: true`,
    // which enables type-library templates defining types for export.
    if !fm.allow_unused
        && !fm.type_aliases.is_empty()
        && (!fm.declarations.is_empty() || !fm.consts.is_empty())
    {
        for (alias_name, alias_type) in &fm.type_aliases {
            let is_used = fm
                .declarations
                .iter()
                .chain(fm.consts.iter())
                .any(|d| var_type_references_alias(&d.var_type, alias_type));
            if !is_used {
                return Err(TemplateError::syntax(format!(
                    "{}: '{alias_name}'",
                    crate::consts::ERR_UNUSED_TYPE_ALIAS,
                )));
            }
        }
    }

    Ok(())
}

/// Check if a [`VarType`] references a specific type alias (by structural equality).
fn var_type_references_alias(ty: &VarType, alias_type: &VarType) -> bool {
    if ty == alias_type {
        return true;
    }
    match ty {
        VarType::List(fields) | VarType::Dict(fields) => fields
            .iter()
            .any(|f| var_type_references_alias(&f.var_type, alias_type)),
        VarType::Enum(variants) => variants.iter().any(|v| {
            v.fields
                .iter()
                .any(|f| var_type_references_alias(&f.var_type, alias_type))
        }),
        _ => false,
    }
}

/// When a param or constant has a compound type (list, dict, enum), and there's no
/// explicit type alias for it, generate one from the name in `PascalCase`.
/// This allows imported templates to reference these types.
pub(crate) fn add_implicit_param_types(fm: &mut Frontmatter) {
    for decl in fm.declarations.iter().chain(fm.consts.iter()) {
        let pascal = crate::types::to_pascal_case(&decl.name);
        match &decl.var_type {
            VarType::List(_) | VarType::Dict(_) | VarType::Enum(_) => {
                fm.type_aliases
                    .entry(pascal)
                    .or_insert_with(|| decl.var_type.clone());
            }
            _ => {}
        }
    }
}
