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
    // R4: Unused type alias check.
    if !fm.allow_unused
        && !fm.type_aliases.is_empty()
        && (!fm.declarations.is_empty() || !fm.consts.is_empty())
    {
        for (alias_name, alias_type) in &fm.type_aliases {
            if matches!(alias_type, VarType::Enum(_)) {
                continue;
            }
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
        VarType::List(fields) | VarType::Struct(fields) => fields
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
            VarType::List(_) | VarType::Struct(_) | VarType::Enum(_) => {
                fm.type_aliases
                    .entry(pascal)
                    .or_insert_with(|| decl.var_type.clone());
            }
            _ => {}
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::{
        compat::HashMap,
        frontmatter::Import,
        types::{VarDecl, VarType},
    };

    /// Helper: build a minimal valid `Frontmatter` with no conflicts.
    fn empty_fm() -> Frontmatter {
        Frontmatter {
            name: None,
            description: None,
            declarations: vec![],
            params: vec![],
            has_params: false,
            allow_unused: false,
            type_aliases: HashMap::new(),
            imports: vec![],
            consts: vec![],
            imported_consts: HashMap::new(),
            imported_enum_type_keys: vec![],
        }
    }

    fn decl(name: &str, var_type: VarType) -> VarDecl {
        VarDecl {
            name: name.into(),
            var_type,
            default_value: None,
        }
    }

    fn list_type(fields: Vec<VarDecl>) -> VarType {
        VarType::List(fields)
    }

    fn import(stem: &str, path: &str) -> Import {
        Import {
            stem: stem.into(),
            #[cfg(feature = "std")]
            path: std::path::PathBuf::from(path),
            #[cfg(not(feature = "std"))]
            path: path.into(),
        }
    }

    // -----------------------------------------------------------------------
    // validate_collision_rules — valid cases
    // -----------------------------------------------------------------------

    #[test]
    fn valid_no_conflicts() {
        let mut fm = empty_fm();
        fm.declarations = vec![decl("user_name", VarType::Str)];
        assert!(validate_collision_rules(&fm).is_ok());
    }

    #[test]
    fn valid_empty_frontmatter() {
        let fm = empty_fm();
        assert!(validate_collision_rules(&fm).is_ok());
    }

    // -----------------------------------------------------------------------
    // R1: PascalCase param name vs type alias collision
    // -----------------------------------------------------------------------

    #[test]
    fn r1_param_pascal_conflicts_with_type_alias() {
        let mut fm = empty_fm();
        // param `code_review` → PascalCase `CodeReview`
        fm.declarations = vec![decl("code_review", VarType::Str)];
        // type alias `CodeReview` mapped to a *different* type
        let alias_type = list_type(vec![decl("title", VarType::Str)]);
        fm.type_aliases.insert("CodeReview".into(), alias_type);
        let err = validate_collision_rules(&fm).unwrap_err();
        assert!(
            err.to_string().contains("conflicts with type alias"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn r1_exception_param_type_is_alias_type() {
        let mut fm = empty_fm();
        // param `code_review` has type `list(title = str)`
        let the_type = list_type(vec![decl("title", VarType::Str)]);
        fm.declarations = vec![decl("code_review", the_type.clone())];
        // type alias `CodeReview` → same type (allowed exception)
        fm.type_aliases.insert("CodeReview".into(), the_type);
        assert!(
            validate_collision_rules(&fm).is_ok(),
            "R1 exception: should allow when param type IS the alias type"
        );
    }

    #[test]
    fn r1_const_pascal_conflicts_with_type_alias() {
        let mut fm = empty_fm();
        // const `code_review` → PascalCase `CodeReview`
        fm.consts = vec![decl("code_review", VarType::Str)];
        let alias_type = list_type(vec![decl("x", VarType::Int)]);
        fm.type_aliases.insert("CodeReview".into(), alias_type);
        let err = validate_collision_rules(&fm).unwrap_err();
        assert!(
            err.to_string().contains("conflicts with type alias"),
            "unexpected error: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // R2: type alias name shadows import stem
    // -----------------------------------------------------------------------

    #[test]
    fn r2_type_alias_shadows_import_stem() {
        let mut fm = empty_fm();
        fm.imports = vec![import("Utils", "utils.tmpl.md")];
        let alias_type = list_type(vec![decl("x", VarType::Str)]);
        fm.type_aliases.insert("Utils".into(), alias_type);
        let err = validate_collision_rules(&fm).unwrap_err();
        assert!(
            err.to_string().contains("shadows"),
            "unexpected error: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // R2b: param PascalCase name shadows import stem
    // -----------------------------------------------------------------------

    #[test]
    fn r2b_param_pascal_shadows_import_stem() {
        let mut fm = empty_fm();
        fm.imports = vec![import("CodeReview", "cr.tmpl.md")];
        // param `code_review` → PascalCase `CodeReview`
        fm.declarations = vec![decl("code_review", VarType::Str)];
        let err = validate_collision_rules(&fm).unwrap_err();
        assert!(
            err.to_string().contains("shadows import"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn r2b_const_pascal_shadows_import_stem() {
        let mut fm = empty_fm();
        fm.imports = vec![import("MyConst", "mc.tmpl.md")];
        fm.consts = vec![decl("my_const", VarType::Int)];
        let err = validate_collision_rules(&fm).unwrap_err();
        assert!(
            err.to_string().contains("shadows import"),
            "unexpected error: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // R3: param name vs const name (exact match)
    // -----------------------------------------------------------------------

    #[test]
    fn r3_param_and_const_same_name() {
        let mut fm = empty_fm();
        fm.declarations = vec![decl("level", VarType::Str)];
        fm.consts = vec![decl("level", VarType::Int)];
        let err = validate_collision_rules(&fm).unwrap_err();
        assert!(
            err.to_string().contains("both a param and a constant"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn r3_no_conflict_different_names() {
        let mut fm = empty_fm();
        fm.declarations = vec![decl("name", VarType::Str)];
        fm.consts = vec![decl("version", VarType::Int)];
        assert!(validate_collision_rules(&fm).is_ok());
    }

    // -----------------------------------------------------------------------
    // R4: unused type alias
    // -----------------------------------------------------------------------

    #[test]
    fn r4_unused_type_alias_rejected() {
        let mut fm = empty_fm();
        let alias_type = list_type(vec![decl("x", VarType::Str)]);
        fm.type_aliases.insert("Unused".into(), alias_type);
        // A param exists but uses a different type (Str, not the alias)
        fm.declarations = vec![decl("name", VarType::Str)];
        let err = validate_collision_rules(&fm).unwrap_err();
        assert!(
            err.to_string().contains("unused type alias"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn r4_suppressed_with_allow_unused() {
        let mut fm = empty_fm();
        let alias_type = list_type(vec![decl("x", VarType::Str)]);
        fm.type_aliases.insert("Unused".into(), alias_type);
        fm.declarations = vec![decl("name", VarType::Str)];
        fm.allow_unused = true;
        assert!(
            validate_collision_rules(&fm).is_ok(),
            "allow_unused should suppress R4"
        );
    }

    #[test]
    fn r4_used_type_alias_accepted() {
        let mut fm = empty_fm();
        let alias_type = list_type(vec![decl("title", VarType::Str)]);
        fm.type_aliases.insert("Items".into(), alias_type.clone());
        // The param uses the exact same type as the alias
        fm.declarations = vec![decl("items", alias_type)];
        assert!(
            validate_collision_rules(&fm).is_ok(),
            "alias referenced by param should be accepted"
        );
    }

    #[test]
    fn r4_skipped_when_no_params_and_no_consts() {
        // Pure type-library template: only types, no params or consts
        let mut fm = empty_fm();
        let alias_type = list_type(vec![decl("x", VarType::Str)]);
        fm.type_aliases.insert("SomeType".into(), alias_type);
        // No declarations and no consts → R4 should NOT fire
        assert!(
            validate_collision_rules(&fm).is_ok(),
            "R4 should be skipped when there are no params/consts"
        );
    }

    // -----------------------------------------------------------------------
    // add_implicit_param_types
    // -----------------------------------------------------------------------

    #[test]
    fn implicit_alias_generated_for_list_param() {
        let mut fm = empty_fm();
        let list_ty = list_type(vec![decl("title", VarType::Str)]);
        fm.declarations = vec![decl("task_reports", list_ty.clone())];
        add_implicit_param_types(&mut fm);
        assert_eq!(
            fm.type_aliases.get("TaskReports"),
            Some(&list_ty),
            "should generate PascalCase alias for list param"
        );
    }

    #[test]
    fn no_implicit_alias_for_str_param() {
        let mut fm = empty_fm();
        fm.declarations = vec![decl("user_name", VarType::Str)];
        add_implicit_param_types(&mut fm);
        assert!(
            !fm.type_aliases.contains_key("UserName"),
            "should NOT generate alias for scalar str param"
        );
    }

    #[test]
    fn no_implicit_alias_for_int_param() {
        let mut fm = empty_fm();
        fm.declarations = vec![decl("retry_count", VarType::Int)];
        add_implicit_param_types(&mut fm);
        assert!(
            !fm.type_aliases.contains_key("RetryCount"),
            "should NOT generate alias for scalar int param"
        );
    }

    #[test]
    fn existing_alias_not_overwritten() {
        let mut fm = empty_fm();
        let existing_type = list_type(vec![decl("old", VarType::Int)]);
        let param_type = list_type(vec![decl("new", VarType::Str)]);
        fm.type_aliases
            .insert("TaskReports".into(), existing_type.clone());
        fm.declarations = vec![decl("task_reports", param_type)];
        add_implicit_param_types(&mut fm);
        assert_eq!(
            fm.type_aliases.get("TaskReports"),
            Some(&existing_type),
            "existing alias should NOT be overwritten"
        );
    }

    #[test]
    fn implicit_alias_generated_for_dict_param() {
        let mut fm = empty_fm();
        let dict_ty = VarType::Struct(vec![decl("key", VarType::Str)]);
        fm.declarations = vec![decl("server_config", dict_ty.clone())];
        add_implicit_param_types(&mut fm);
        assert_eq!(
            fm.type_aliases.get("ServerConfig"),
            Some(&dict_ty),
            "should generate PascalCase alias for dict param"
        );
    }

    #[test]
    fn implicit_alias_generated_for_enum_param() {
        let mut fm = empty_fm();
        let enum_ty = VarType::Enum(vec![
            crate::types::VariantDecl {
                name: "High".into(),
                fields: vec![],
            },
            crate::types::VariantDecl {
                name: "Low".into(),
                fields: vec![],
            },
        ]);
        fm.declarations = vec![decl("severity_level", enum_ty.clone())];
        add_implicit_param_types(&mut fm);
        assert_eq!(
            fm.type_aliases.get("SeverityLevel"),
            Some(&enum_ty),
            "should generate PascalCase alias for enum param"
        );
    }

    #[test]
    fn implicit_alias_generated_for_const_with_compound_type() {
        let mut fm = empty_fm();
        let list_ty = list_type(vec![decl("label", VarType::Str)]);
        fm.consts = vec![decl("default_items", list_ty.clone())];
        add_implicit_param_types(&mut fm);
        assert_eq!(
            fm.type_aliases.get("DefaultItems"),
            Some(&list_ty),
            "should generate PascalCase alias for compound const type"
        );
    }

    #[test]
    fn implicit_alias_not_generated_for_scalar_const() {
        let mut fm = empty_fm();
        fm.consts = vec![decl("max_retries", VarType::Int)];
        add_implicit_param_types(&mut fm);
        assert!(
            !fm.type_aliases.contains_key("MaxRetries"),
            "should NOT generate alias for scalar const"
        );
    }
}
