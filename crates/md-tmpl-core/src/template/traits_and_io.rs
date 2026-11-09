//! Declaration validation, trait implementations, static analysis, and file loading for [`Template`].

use alloc::{format, string::String, vec::Vec};
#[cfg(feature = "std")]
use std::path::Path;

use super::{
    Template,
    analysis::{
        check_bare_enum_access, check_internal_key_access, check_name_collisions,
        check_static_enum_in_conditions, check_undeclared_variables, check_unused_params,
        collect_enum_type_keys,
    },
};
use crate::{
    compat::{HashMap, HashSet},
    compiled::{self, Segment},
    error::TemplateError,
    frontmatter::Frontmatter,
    types::VarDecl,
};

impl Template {
    /// Validate that a (possibly reloaded) template's variable declarations
    /// match an expected set.
    ///
    /// Call this after re-loading a template from disk to ensure that
    /// nobody (e.g. an autonomous agent editing markdown files at runtime)
    /// has modified the `params:` block in the frontmatter.
    ///
    /// The template body may be changed freely — only the variable
    /// declarations must remain stable.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::DeclarationsMutated`] with a human-readable
    /// diff if the declarations don't match.
    pub fn validate_declarations(&self, expected: &[VarDecl]) -> Result<(), TemplateError> {
        let current: HashMap<&str, &crate::types::VarType> = self
            .declared_variables
            .iter()
            .map(|d| (d.name.as_str(), &d.var_type))
            .collect();
        let expected_map: HashMap<&str, &crate::types::VarType> = expected
            .iter()
            .map(|d| (d.name.as_str(), &d.var_type))
            .collect();

        let current_names: HashSet<&str> = current.keys().copied().collect();
        let expected_names: HashSet<&str> = expected_map.keys().copied().collect();

        let missing: Vec<&str> = expected_names.difference(&current_names).copied().collect();
        let extra: Vec<&str> = current_names.difference(&expected_names).copied().collect();

        // Check for type changes on variables that exist in both.
        let retyped: Vec<String> = current_names
            .intersection(&expected_names)
            .filter_map(|name| {
                let cur_type = current[name];
                let exp_type = expected_map[name];
                if cur_type == exp_type {
                    None
                } else {
                    Some(format!("{name}: {exp_type} → {cur_type}"))
                }
            })
            .collect();

        if missing.is_empty() && extra.is_empty() && retyped.is_empty() {
            return Ok(());
        }

        let mut parts = Vec::new();
        if !missing.is_empty() {
            parts.push(format!("removed: {}", missing.join(", ")));
        }
        if !extra.is_empty() {
            parts.push(format!("added: {}", extra.join(", ")));
        }
        if !retyped.is_empty() {
            parts.push(format!("retyped: {}", retyped.join(", ")));
        }

        Err(TemplateError::DeclarationsMutated {
            details: parts.join("; "),
        })
    }
}

// ---------------------------------------------------------------------------
// Trait impls for embedding Template in macro-generated structs
// ---------------------------------------------------------------------------

/// Two templates are considered equal if they were compiled from the same
/// source (compared via non-cryptographic 64-bit hash).
///
/// **Note:** This is an approximate comparison — different sources that
/// produce the same hash would incorrectly compare as equal.  Do not use
/// `Template` as a `HashMap` key or rely on `Eq` for deduplication in
/// security-sensitive contexts.  For exact source comparison, compare
/// [`body()`](Self::body) and [`declarations()`](Self::declarations).
impl PartialEq for Template {
    fn eq(&self, other: &Self) -> bool {
        self.source_hash == other.source_hash
    }
}

impl Eq for Template {}

/// Serialize a [`Template`] as a source-hash identifier.
///
/// Templates embedded in macro-generated parameter structs need `Serialize`
/// to satisfy derive bounds, even when the struct is never actually
/// serialized.  The hash lets debug/logging code produce something readable.
#[cfg(feature = "serde")]
impl serde::Serialize for Template {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("template:{:016x}", self.source_hash))
    }
}

/// Deserialize always fails — [`Template`] must be constructed from source.
///
/// This impl exists solely to satisfy derive bounds on macro-generated
/// parameter structs.  Actual deserialization of a compiled template is not
/// meaningful.
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Template {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <serde::de::IgnoredAny as serde::Deserialize>::deserialize(deserializer)?;
        Err(serde::de::Error::custom(
            "Template cannot be deserialized; construct from source with \
             Template::from_source() or Template::from_file()",
        ))
    }
}

pub(super) fn run_static_analysis(
    segments: &[Segment],
    fm: &Frontmatter,
    inline_templates: &HashMap<String, compiled::CompiledInlineTemplate>,
    force_allow_unused: bool,
) -> Result<(), TemplateError> {
    let referenced = compiled::collect_referenced_params(segments);
    let case_labels = compiled::collect_unquoted_case_labels(segments);
    check_undeclared_variables(&referenced, fm, inline_templates)?;
    check_unused_params(
        &fm.declarations,
        &referenced,
        &case_labels,
        force_allow_unused || fm.allow_unused,
    )?;
    check_name_collisions(fm, inline_templates, segments)?;
    let enum_keys = collect_enum_type_keys(fm);
    check_bare_enum_access(segments, &enum_keys)?;
    check_static_enum_in_conditions(segments, &fm.type_aliases)?;
    check_internal_key_access(segments)?;
    let label_errors =
        compiled::validate_match_labels(segments, &fm.declarations, &fm.type_aliases);
    if !label_errors.is_empty() {
        return Err(TemplateError::Syntax(label_errors.join("; ").into()));
    }
    // Flow-sensitive type check (field access, displayability, built-in
    // argument types) — the same checker the `template!` proc macro runs.
    let type_errors = fm.validate_field_types(segments);
    if !type_errors.is_empty() {
        return Err(TemplateError::Syntax(type_errors.join("; ").into()));
    }
    Ok(())
}

/// Load a named template from a directory.
///
/// Looks for `<name>.tmpl.md` in `dir`.
///
/// # Errors
///
/// Returns [`TemplateError::Io`] if the file is not found or cannot be read.
#[cfg(feature = "std")]
pub fn load_template(dir: &Path, name: &str) -> Result<Template, TemplateError> {
    let path = dir.join(format!("{name}.tmpl.md"));
    Template::from_file(&path)
}
