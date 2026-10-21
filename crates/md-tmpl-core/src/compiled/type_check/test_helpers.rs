//! Shared declaration and compile helpers for type-check tests.

use super::*;
use crate::{
    compiled::{CompiledInclude, CompiledInlineTemplate, Segment},
    types::{VarDecl, VarType, VariantDecl},
};

/// Helper: build declarations from a shorthand.
pub(super) fn enum_decl(name: &str, variants: Vec<VariantDecl>) -> VarDecl {
    VarDecl {
        name: name.to_string(),
        var_type: VarType::Enum(variants),
        default_value: None,
    }
}

pub(super) fn variant(name: &str, fields: Vec<(&str, VarType)>) -> VariantDecl {
    VariantDecl {
        name: name.to_string(),
        fields: fields
            .into_iter()
            .map(|(n, t)| VarDecl {
                name: n.to_string(),
                var_type: t,
                default_value: None,
            })
            .collect(),
    }
}

pub(super) fn unit_variant(name: &str) -> VariantDecl {
    variant(name, vec![])
}

pub(super) fn compile_and_check(template: &str, decls: &[VarDecl]) -> Vec<String> {
    let (_fm, body) = crate::parse_frontmatter(template).expect("parse");
    let empty_aliases = crate::compat::HashMap::new();
    let (segments, _) = crate::compiled::compile(body, &empty_aliases).expect("compile");
    validate_field_accesses(&segments, decls)
}

pub(super) fn make_include(
    path: &str,
    with_vars: Vec<(&str, &str)>,
    for_each: Option<(&str, &str)>,
    declarations: Vec<VarDecl>,
    segments: Vec<Segment>,
) -> CompiledInclude {
    use std::sync::Arc;
    CompiledInclude {
        path: std::borrow::Cow::Owned(path.to_string()),
        with_vars: with_vars
            .into_iter()
            .map(|(k, v)| {
                (
                    std::borrow::Cow::Owned(k.to_string()),
                    std::borrow::Cow::Owned(v.to_string()),
                )
            })
            .collect(),
        for_each: for_each.map(|(b, l)| {
            (
                std::borrow::Cow::Owned(b.to_string()),
                std::borrow::Cow::Owned(l.to_string()),
            )
        }),
        inline_compiled: Some(CompiledInlineTemplate {
            segments: Arc::from(segments),
            declarations: Arc::from(declarations),
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
        sanitize_ctx: None,
        memo: crate::compiled::IncludeContextMemo::default(),
    }
}

pub(super) fn str_decl(name: &str) -> VarDecl {
    VarDecl {
        name: name.to_string(),
        var_type: VarType::Str,
        default_value: None,
    }
}

pub(super) fn int_decl(name: &str) -> VarDecl {
    VarDecl {
        name: name.to_string(),
        var_type: VarType::Int,
        default_value: None,
    }
}

pub(super) fn bool_decl(name: &str) -> VarDecl {
    VarDecl {
        name: name.to_string(),
        var_type: VarType::Bool,
        default_value: None,
    }
}
