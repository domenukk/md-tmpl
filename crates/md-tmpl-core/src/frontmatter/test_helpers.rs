//! Shared helpers for the frontmatter and declaration parsing tests.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use super::{parse_declarations, parse_frontmatter, parse_type_annotation};
use crate::{
    compat::HashMap,
    error::TemplateError,
    types::{VarDecl, VarType, VariantDecl},
    value::Value,
};

/// Helper: parse a type annotation with empty aliases/imports.
pub(super) fn parse_type(s: &str) -> Result<VarType, String> {
    let aliases = HashMap::new();
    let imports = HashMap::new();
    parse_type_annotation(s, &aliases, &imports)
}

/// Helper: parse declarations (params, not constants) with empty aliases/imports.
pub(super) fn parse_decls(rest: &str) -> Result<Vec<VarDecl>, TemplateError> {
    let aliases = HashMap::new();
    let imports = HashMap::new();
    let consts = HashMap::new();
    parse_declarations(rest, &aliases, &imports, false, &consts).map(|(decls, _)| decls)
}

/// Helper: parse constant declarations with empty aliases/imports.
pub(super) fn parse_consts(rest: &str) -> Result<Vec<VarDecl>, TemplateError> {
    let aliases = HashMap::new();
    let imports = HashMap::new();
    let consts = HashMap::new();
    parse_declarations(rest, &aliases, &imports, true, &consts).map(|(decls, _)| decls)
}

/// A `Stage` enum alias used by the alias/nesting D2 cases.
fn stage_aliases() -> HashMap<String, VarType> {
    let mut aliases = HashMap::new();
    aliases.insert(
        "Stage".to_string(),
        VarType::Enum(vec![
            VariantDecl {
                name: "Design".into(),
                fields: vec![],
            },
            VariantDecl {
                name: "Build".into(),
                fields: vec![],
            },
            VariantDecl {
                name: "Deploy".into(),
                fields: vec![],
            },
        ]),
    );
    aliases
}

/// Parse declarations with the `Stage` alias available.
pub(super) fn parse_decls_stage(rest: &str) -> Result<Vec<VarDecl>, TemplateError> {
    let aliases = stage_aliases();
    let imports = HashMap::new();
    let consts = HashMap::new();
    parse_declarations(rest, &aliases, &imports, false, &consts).map(|(decls, _)| decls)
}

/// Helper: extract the sole declaration's default value from a template whose
/// `params:` line is `params: [<decl>]`.
pub(super) fn default_of(decl: &str) -> Value {
    let source = format!(
        "---
params: [{decl}]
---
body"
    );
    let (fm, _) = parse_frontmatter(&source)
        .unwrap_or_else(|e| panic!("failed to parse frontmatter for `{decl}`: {e}"));
    fm.declarations[0]
        .default_value
        .clone()
        .unwrap_or_else(|| panic!("declaration `{decl}` had no default value"))
}

/// Extract md-tmpl's logical scalar for a single block list item `- {item}`,
/// mirroring the full frontmatter pipeline: inline-comment stripping
/// (`join_continuation_lines`) followed by outer YAML-quote removal +
/// unescaping (as `parse_declarations` does).
pub(super) fn mdtmpl_list_scalar(item: &str) -> String {
    let block = format!("- {item}");
    let logical = crate::frontmatter::params::join_continuation_lines(&block);
    let line = logical
        .first()
        .expect("md-tmpl produced no logical line for list item");
    let scalar = line
        .strip_prefix("- ")
        .expect("logical line should retain its list marker")
        .trim();
    match crate::consts::strip_string_literal(scalar) {
        Some(inner) => crate::consts::unescape_string_literal(inner),
        None => scalar.to_string(),
    }
}

/// Extract the scalar a real YAML parser sees for `- {item}`.
pub(super) fn yaml_list_scalar(item: &str) -> String {
    let doc = format!("- {item}");
    let seq: Vec<String> =
        serde_yaml::from_str(&doc).expect("serde_yaml failed to parse the sequence");
    seq.into_iter()
        .next()
        .expect("serde_yaml produced an empty sequence")
}
