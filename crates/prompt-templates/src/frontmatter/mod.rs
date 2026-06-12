//! YAML-style frontmatter parsing for `.tmpl.md` files.
//!
//! Extracts template metadata (name, description, typed variable declarations)
//! from the `---`-delimited block at the start of a template source string.
//!
//! ## Frontmatter v2 format
//!
//! Uses `=` for name-type pairs, `<>` for type parameters, `:=` for defaults:
//!
//! ```text
//! ---
//! name: my_template
//! params:
//!   - name = str
//!   - count = int := 42
//!   - bugs = list<title = str, severity = int>
//! ---
//! ```

mod imports;
mod params;
mod type_aliases;
mod validation;

use std::{collections::HashMap, path::PathBuf};

pub use imports::*;
pub(crate) use params::*;
pub(crate) use type_aliases::*;
pub(crate) use validation::*;

use crate::{
    consts::{
        FM_ALLOW_UNUSED_PREFIX, FM_CONSTS_PREFIX, FM_DELIMITER, FM_DELIMITER_NEWLINE,
        FM_DESC_PREFIX, FM_IMPORTS_PREFIX, FM_NAME_PREFIX, FM_PARAMS_PREFIX, FM_TYPES_PREFIX,
    },
    error::TemplateError,
    frontmatter::params::parse_declarations,
    types::{VarDecl, VarType},
};

/// A template import declaration: `[stem](path.tmpl.md)`.
#[derive(Debug, Clone)]
pub struct Import {
    /// Short alias used as namespace prefix, e.g. `other`.
    pub stem: String,
    /// Relative path to the imported template file.
    pub path: PathBuf,
}

/// Resolved namespace from an imported template.
#[derive(Debug, Clone, Default)]
pub struct ImportedNamespace {
    /// Type aliases exported by the imported template.
    pub type_aliases: HashMap<String, VarType>,
    /// Parameter types (for cross-template type references).
    pub param_types: HashMap<String, VarType>,
    /// Constants exported by the imported template.
    pub consts: HashMap<String, crate::value::Value>,
}

/// Parsed YAML frontmatter from a `.tmpl.md` file.
#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    /// Template name (matches SKILL.md `name:` convention).
    pub name: String,
    /// Description of the template's purpose.
    pub description: String,
    /// List of expected variable declarations (name + type + optional default).
    pub declarations: Vec<VarDecl>,
    /// Convenience: parameter names only (derived from `declarations`).
    pub params: Vec<String>,
    /// Whether the params: block was present in frontmatter.
    pub has_params: bool,
    /// Allow declared parameters that are never referenced in the body.
    ///
    /// Set via `allow_unused: true` in frontmatter. Useful for
    /// dynamically-loaded templates where params may be conditionally used.
    pub allow_unused: bool,
    /// Type aliases defined via `types:` in frontmatter.
    ///
    /// Maps alias names (e.g. `Priority`) to their resolved [`VarType`].
    pub type_aliases: HashMap<String, VarType>,
    /// Import declarations defined via `imports:` in frontmatter.
    pub imports: Vec<Import>,
    /// Constants defined via `consts:` in frontmatter.
    pub consts: Vec<VarDecl>,
    /// Resolved constants from imports, keyed by `stem.NAME`.
    pub imported_consts: HashMap<String, crate::value::Value>,
}

/// Strip YAML frontmatter delimited by `---` and return only the body text.
///
/// # Errors
///
/// Returns [`TemplateError::Syntax`] if the frontmatter block is missing or invalid.
pub fn strip_frontmatter(source: &str) -> Result<&str, TemplateError> {
    parse_frontmatter(source).map(|(_, body)| body)
}

/// Parse YAML frontmatter delimited by `---` lines.
///
/// Returns the parsed [`Frontmatter`] and a string slice pointing to the
/// template body after the closing `---`.
///
/// # Errors
///
/// Returns [`TemplateError::Syntax`] if the frontmatter block is
/// missing, unclosed, or contains invalid declarations.
pub fn parse_frontmatter(source: &str) -> Result<(Frontmatter, &str), TemplateError> {
    let trimmed = source.trim_start();
    if !trimmed.starts_with(FM_DELIMITER) {
        return Err(TemplateError::syntax(
            crate::consts::ERR_MISSING_FM.to_string(),
        ));
    }

    // Find the closing `---`.
    let after_first = trimmed[FM_DELIMITER.len()..].trim_start_matches(['\r', '\n']);
    let Some(end) = after_first.find(FM_DELIMITER_NEWLINE) else {
        return Err(TemplateError::syntax(
            crate::consts::ERR_UNCLOSED_FM.to_string(),
        ));
    };

    let yaml_block = &after_first[..end];
    // Skip past "\n---" (4 chars), then skip the trailing newline if present.
    let after_close = end + FM_DELIMITER_NEWLINE.len();
    let body_start = if after_first[after_close..].starts_with('\n') {
        after_close + 1
    } else if after_first[after_close..].starts_with("\r\n") {
        after_close + 2
    } else {
        after_close
    };
    let body = &after_first[body_start..];

    let mut fm = Frontmatter::default();

    // Collect all raw lines, then join continuation lines (lines starting with
    // whitespace) back onto their parent so that multiline `params:` blocks
    // are handled correctly.
    let logical_lines = join_continuation_lines(yaml_block);

    // --- Pass 1: Collect types:, imports:, and simple keys ---
    let mut params_raw: Option<String> = None;
    let mut consts_raw: Option<String> = None;

    for line in &logical_lines {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(FM_NAME_PREFIX) {
            fm.name = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(FM_DESC_PREFIX) {
            fm.description = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(FM_TYPES_PREFIX) {
            fm.type_aliases = parse_types_value(rest)?;
        } else if let Some(rest) = line.strip_prefix(FM_IMPORTS_PREFIX) {
            fm.imports = parse_imports_value(rest)?;
        } else if let Some(rest) = line.strip_prefix(FM_PARAMS_PREFIX) {
            params_raw = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix(FM_CONSTS_PREFIX) {
            consts_raw = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix(FM_ALLOW_UNUSED_PREFIX) {
            fm.allow_unused = rest.trim() == crate::consts::LIT_TRUE;
        }
    }

    // --- Pass 2: Parse params/consts with type aliases available ---
    let resolved_imports = HashMap::new();
    if let Some(raw) = params_raw {
        let decls = parse_declarations(&raw, &fm.type_aliases, &resolved_imports, false)?;
        fm.params = decls.iter().map(|d| d.name.clone()).collect();
        fm.declarations = decls;
        fm.has_params = true;
    }
    if let Some(raw) = consts_raw {
        fm.consts = parse_declarations(&raw, &fm.type_aliases, &resolved_imports, true)?;
    }

    validate_collision_rules(&fm)?;
    add_implicit_param_types(&mut fm);

    Ok((fm, body))
}

/// Parse YAML frontmatter with cross-template import resolution.
///
/// Like [`parse_frontmatter`], but additionally resolves `imports:` entries
/// by reading referenced template files from disk relative to `base_dir`.
/// This allows params to reference imported types (e.g. `types.Severity`).
///
/// # Errors
///
/// Returns [`TemplateError::Syntax`] if the frontmatter block is invalid,
/// an imported file cannot be read, or imported types cannot be resolved.
pub fn parse_frontmatter_with_base_dir<'a>(
    source: &'a str,
    base_dir: &std::path::Path,
) -> Result<(Frontmatter, &'a str), TemplateError> {
    let trimmed = source.trim_start();
    if !trimmed.starts_with(FM_DELIMITER) {
        return Err(TemplateError::syntax(
            crate::consts::ERR_MISSING_FM.to_string(),
        ));
    }

    // Find the closing `---`.
    let after_first = trimmed[FM_DELIMITER.len()..].trim_start_matches(['\r', '\n']);
    let Some(end) = after_first.find(FM_DELIMITER_NEWLINE) else {
        return Err(TemplateError::syntax(
            crate::consts::ERR_UNCLOSED_FM.to_string(),
        ));
    };

    let yaml_block = &after_first[..end];
    let after_close = end + FM_DELIMITER_NEWLINE.len();
    let body_start = if after_first[after_close..].starts_with('\n') {
        after_close + 1
    } else if after_first[after_close..].starts_with("\r\n") {
        after_close + 2
    } else {
        after_close
    };
    let body = &after_first[body_start..];

    let mut fm = Frontmatter::default();
    let logical_lines = join_continuation_lines(yaml_block);

    // --- Pass 1: Collect types:, imports:, and simple keys ---
    let mut params_raw: Option<String> = None;
    let mut consts_raw: Option<String> = None;

    for line in &logical_lines {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(FM_NAME_PREFIX) {
            fm.name = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(FM_DESC_PREFIX) {
            fm.description = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(FM_TYPES_PREFIX) {
            fm.type_aliases = parse_types_value(rest)?;
        } else if let Some(rest) = line.strip_prefix(FM_IMPORTS_PREFIX) {
            fm.imports = parse_imports_value(rest)?;
        } else if let Some(rest) = line.strip_prefix(FM_PARAMS_PREFIX) {
            params_raw = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix(FM_CONSTS_PREFIX) {
            consts_raw = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix(FM_ALLOW_UNUSED_PREFIX) {
            fm.allow_unused = rest.trim() == crate::consts::LIT_TRUE;
        }
    }

    let resolved_imports = if fm.imports.is_empty() {
        HashMap::new()
    } else {
        let mut visited = std::collections::HashSet::new();
        resolve_imports(&fm.imports, base_dir, &mut visited)?
    };

    for (stem, ns) in &resolved_imports {
        for (name, val) in &ns.consts {
            fm.imported_consts
                .insert(format!("{stem}.{name}"), val.clone());
        }
    }

    if let Some(raw) = params_raw {
        let decls = parse_declarations(&raw, &fm.type_aliases, &resolved_imports, false)?;
        fm.params = decls.iter().map(|d| d.name.clone()).collect();
        fm.declarations = decls;
        fm.has_params = true;
    }
    if let Some(raw) = consts_raw {
        fm.consts = parse_declarations(&raw, &fm.type_aliases, &resolved_imports, true)?;
    }

    validate_collision_rules(&fm)?;
    add_implicit_param_types(&mut fm);

    Ok((fm, body))
}

/// Parse YAML frontmatter with access to a parent template's type aliases.
///
/// Used for inline template definitions (`{% tmpl %}` blocks) that can
/// reference type aliases from the enclosing template. The inline's own
/// `types:` block shadows the parent's (resolution order: own → parent).
///
/// This is the same as [`parse_frontmatter`] except params can reference
/// parent type aliases for type resolution.
pub fn parse_frontmatter_with_parent_scope<'a>(
    source: &'a str,
    parent_type_aliases: &HashMap<String, VarType>,
) -> Result<(Frontmatter, &'a str), TemplateError> {
    let trimmed = source.trim_start();
    if !trimmed.starts_with(FM_DELIMITER) {
        // Inline templates may omit frontmatter entirely — treat the whole
        // source as body with no params. The parent's type aliases are still
        // available for any future extensions.
        return Ok((Frontmatter::default(), source));
    }

    let after_first = trimmed[FM_DELIMITER.len()..].trim_start_matches(['\r', '\n']);
    let Some(end) = after_first.find(FM_DELIMITER_NEWLINE) else {
        return Err(TemplateError::syntax(
            crate::consts::ERR_UNCLOSED_FM.to_string(),
        ));
    };

    let yaml_block = &after_first[..end];
    let after_close = end + FM_DELIMITER_NEWLINE.len();
    let body_start = if after_first[after_close..].starts_with('\n') {
        after_close + 1
    } else if after_first[after_close..].starts_with("\r\n") {
        after_close + 2
    } else {
        after_close
    };
    let body = &after_first[body_start..];

    let mut fm = Frontmatter::default();
    let logical_lines = join_continuation_lines(yaml_block);

    // --- Pass 1: Collect own types:, imports:, and simple keys ---
    let mut params_raw: Option<String> = None;
    let mut consts_raw: Option<String> = None;

    for line in &logical_lines {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(FM_NAME_PREFIX) {
            fm.name = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(FM_DESC_PREFIX) {
            fm.description = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(FM_TYPES_PREFIX) {
            fm.type_aliases = parse_types_value(rest)?;
        } else if let Some(rest) = line.strip_prefix(FM_IMPORTS_PREFIX) {
            fm.imports = parse_imports_value(rest)?;
        } else if let Some(rest) = line.strip_prefix(FM_PARAMS_PREFIX) {
            params_raw = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix(FM_CONSTS_PREFIX) {
            consts_raw = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix(FM_ALLOW_UNUSED_PREFIX) {
            fm.allow_unused = rest.trim() == crate::consts::LIT_TRUE;
        }
    }

    // --- Pass 2: Parse params with merged type aliases (own → parent) ---
    if let Some(raw) = params_raw {
        // Build merged alias map: parent first, then own (own shadows parent).
        let mut merged_aliases = parent_type_aliases.clone();
        for (k, v) in &fm.type_aliases {
            merged_aliases.insert(k.clone(), v.clone());
        }
        let resolved_imports = HashMap::new();
        let decls = parse_declarations(&raw, &merged_aliases, &resolved_imports, false)?;
        fm.params = decls.iter().map(|d| d.name.clone()).collect();
        fm.declarations = decls;
        fm.has_params = true;
    }
    if let Some(raw) = consts_raw {
        let mut merged_aliases = parent_type_aliases.clone();
        for (k, v) in &fm.type_aliases {
            merged_aliases.insert(k.clone(), v.clone());
        }
        let resolved_imports = HashMap::new();
        fm.consts = parse_declarations(&raw, &merged_aliases, &resolved_imports, true)?;
    }

    validate_collision_rules(&fm)?;
    add_implicit_param_types(&mut fm);

    Ok((fm, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    /// Wrapper for `parse_type_annotation` without aliases — for baseline tests.
    fn parse_type_annotation(s: &str) -> Result<VarType, String> {
        let empty_aliases = HashMap::new();
        let empty_imports = HashMap::new();
        super::parse_type_annotation(s, &empty_aliases, &empty_imports)
    }

    /// Wrapper for `parse_declarations` without aliases — for baseline tests.
    fn parse_params_value(rest: &str) -> Result<Vec<VarDecl>, TemplateError> {
        let empty_aliases = HashMap::new();
        let empty_imports = HashMap::new();
        super::parse_declarations(rest, &empty_aliases, &empty_imports, false)
    }

    #[test]
    fn parse_empty_source() {
        let err = parse_frontmatter("").unwrap_err();
        assert!(
            err.to_string()
                .contains("missing mandatory YAML frontmatter block")
        );
    }

    #[test]
    fn parse_no_frontmatter() {
        let source = "Hello {{ name }}!";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing mandatory YAML frontmatter block")
        );
    }

    #[test]
    fn parse_basic_frontmatter() {
        let source = "---\nname: greeting\ndescription: A greeting template\nparams: [name = str, count = int]\n---\nHello {{ name }}!";
        let (fm, body) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.name, "greeting");
        assert_eq!(fm.description, "A greeting template");
        assert_eq!(fm.params, vec!["name", "count"]);
        assert_eq!(fm.declarations.len(), 2);
        assert_eq!(fm.declarations[0].name, "name");
        assert_eq!(fm.declarations[0].var_type, VarType::Str);
        assert_eq!(fm.declarations[1].name, "count");
        assert_eq!(fm.declarations[1].var_type, VarType::Int);
        assert_eq!(body, "Hello {{ name }}!");
    }

    #[test]
    fn reject_untyped_params() {
        let source = "---\nparams: [a, b, c]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(err.to_string().contains("missing a type annotation"));
    }

    #[test]
    fn parse_multiline_block_format() {
        let source = "---\nname: test\nparams:\n  - a = str\n  - b = int\n---\n{{ a }} {{ b }}";
        let (fm, body) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.params, vec!["a", "b"]);
        assert_eq!(fm.declarations[0].var_type, VarType::Str);
        assert_eq!(fm.declarations[1].var_type, VarType::Int);
        assert_eq!(body, "{{ a }} {{ b }}");
    }

    #[test]
    fn parse_list_with_fields() {
        let source = "---\nparams: [items = list<title = str, score = float>]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations.len(), 1);
        assert_eq!(fm.declarations[0].name, "items");
        match &fm.declarations[0].var_type {
            VarType::List(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "title");
                assert_eq!(fields[0].var_type, VarType::Str);
                assert_eq!(fields[1].name, "score");
                assert_eq!(fields[1].var_type, VarType::Float);
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn parse_dict_type() {
        let source = "---\nparams: [config = dict<key = str, enabled = bool>]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations.len(), 1);
        match &fm.declarations[0].var_type {
            VarType::Dict(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "key");
                assert_eq!(fields[0].var_type, VarType::Str);
                assert_eq!(fields[1].name, "enabled");
                assert_eq!(fields[1].var_type, VarType::Bool);
            }
            other => panic!("Expected Dict, got {other:?}"),
        }
    }

    #[test]
    fn reject_bare_list_type() {
        let source = "---\nparams: [items = list]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(err.to_string().contains("unknown type"));
    }

    #[test]
    fn parse_float_type() {
        let source = "---\nparams: [score = float]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].var_type, VarType::Float);
    }

    #[test]
    fn parse_bool_type() {
        let source = "---\nparams: [active = bool]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].var_type, VarType::Bool);
    }

    #[test]
    fn reject_unknown_type() {
        let source = "---\nparams: [x = unknown_type]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(err.to_string().contains("unknown type 'unknown_type'"));
    }

    #[test]
    fn reject_mixed_typed_and_untyped() {
        let source = "---\nparams: [name = str, label, count = int]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(err.to_string().contains("missing a type annotation"));
    }

    #[test]
    fn parse_empty_params_list() {
        let source = "---\nparams: []\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert!(fm.declarations.is_empty());
        assert!(fm.params.is_empty());
    }

    #[test]
    fn types_only_template_no_params_block() {
        let source =
            "---\nname: types\ntypes:\n  - Priority = enum<High, Medium, Low>\n---\n{# no body #}";
        let (fm, body) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.name, "types");
        assert!(fm.declarations.is_empty());
        assert!(fm.params.is_empty());
        assert!(!fm.has_params);
        assert!(fm.type_aliases.contains_key("Priority"));
        assert_eq!(body, "{# no body #}");
    }

    #[test]
    fn frontmatter_not_at_start() {
        let source = "some text\n---\nname: test\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing mandatory YAML frontmatter block")
        );
    }

    #[test]
    fn frontmatter_without_closing_delimiter() {
        let source = "---\nname: test\nno closing delimiter";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(err.to_string().contains("unclosed YAML frontmatter block"));
    }

    #[test]
    fn join_continuation_lines_basic() {
        let block = "key1: val1\nkey2:\n  continued\n  more";
        let lines = join_continuation_lines(block);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "key1: val1");
        assert!(lines[1].contains("continued"));
        assert!(lines[1].contains("more"));
    }

    #[test]
    fn parse_type_annotation_all_simple_types() {
        assert_eq!(parse_type_annotation("str").unwrap(), VarType::Str);
        assert_eq!(parse_type_annotation("bool").unwrap(), VarType::Bool);
        assert_eq!(parse_type_annotation("int").unwrap(), VarType::Int);
        assert_eq!(parse_type_annotation("float").unwrap(), VarType::Float);
        parse_type_annotation("garbage").expect_err("unknown type 'garbage' should be rejected");
        parse_type_annotation("list").expect_err("bare 'list' without <fields> should be rejected");
        parse_type_annotation("dict").expect_err("bare 'dict' without <fields> should be rejected");
    }

    #[test]
    fn parse_type_annotation_with_whitespace() {
        assert_eq!(parse_type_annotation("  str  ").unwrap(), VarType::Str);
        assert_eq!(parse_type_annotation("\tint\t").unwrap(), VarType::Int);
    }

    #[test]
    fn parse_params_complex() {
        let rest = "[name = str, items = list<label = str, count = int>, active = bool]";
        let decls = parse_params_value(rest).unwrap();
        assert_eq!(decls.len(), 3);
        assert_eq!(decls[0].name, "name");
        assert_eq!(decls[0].var_type, VarType::Str);
        assert_eq!(decls[2].name, "active");
        assert_eq!(decls[2].var_type, VarType::Bool);
        match &decls[1].var_type {
            VarType::List(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "label");
                assert_eq!(fields[1].name, "count");
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn parse_enum_with_associated_data() {
        let rest = "[outcome = enum<Confirmed(evidence = list<text = str>), Inconclusive>]";
        let decls = parse_params_value(rest).unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "outcome");
        match &decls[0].var_type {
            VarType::Enum(variants) => {
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0].name, "Confirmed");
                assert_eq!(variants[0].fields.len(), 1);
                assert_eq!(variants[0].fields[0].name, "evidence");
                assert_eq!(variants[1].name, "Inconclusive");
                assert!(variants[1].fields.is_empty());
            }
            other => panic!("Expected Enum, got {other:?}"),
        }
    }

    // -- Default value tests --

    #[test]
    fn parse_string_default() {
        let source = "---\nparams: [name = str := \"hello world\"]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].name, "name");
        assert_eq!(fm.declarations[0].var_type, VarType::Str);
        assert_eq!(
            fm.declarations[0].default_value,
            Some(Value::Str("hello world".to_string()))
        );
    }

    #[test]
    fn parse_int_default() {
        let source = "---\nparams: [count = int := 42]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].var_type, VarType::Int);
        assert_eq!(fm.declarations[0].default_value, Some(Value::Int(42)));
    }

    #[test]
    fn parse_bool_default() {
        let source = "---\nparams: [active = bool := true]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].var_type, VarType::Bool);
        assert_eq!(fm.declarations[0].default_value, Some(Value::Bool(true)));
    }

    #[test]
    fn parse_float_default() {
        let source = "---\nparams: [score = float := 3.15]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].var_type, VarType::Float);
        assert_eq!(fm.declarations[0].default_value, Some(Value::Float(3.15)));
    }

    #[test]
    fn parse_mixed_defaults_and_required() {
        let source = "---\nparams: [name = str, count = int := 10]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].default_value, None);
        assert_eq!(fm.declarations[1].default_value, Some(Value::Int(10)));
    }

    #[test]
    fn default_does_not_confuse_with_inner_colons() {
        // The `:=` inside `<>` should not be treated as a default separator.
        // This is handled by find_assign_default_at_depth_zero.
        let source = "---\nparams: [bugs = list<title = str>]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].default_value, None);
        match &fm.declarations[0].var_type {
            VarType::List(fields) => {
                assert_eq!(fields[0].name, "title");
                assert_eq!(fields[0].var_type, VarType::Str);
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn parse_default_value_types() {
        assert_eq!(
            parse_default_value("\"hello\""),
            Some(Value::Str("hello".to_string()))
        );
        assert_eq!(
            parse_default_value("'world'"),
            Some(Value::Str("world".to_string()))
        );
        assert_eq!(parse_default_value("42"), Some(Value::Int(42)));
        assert_eq!(parse_default_value("-1"), Some(Value::Int(-1)));
        assert_eq!(parse_default_value("3.15"), Some(Value::Float(3.15)));
        assert_eq!(parse_default_value("true"), Some(Value::Bool(true)));
        assert_eq!(parse_default_value("false"), Some(Value::Bool(false)));
        assert_eq!(parse_default_value(""), None);
    }

    #[test]
    fn parse_block_format_with_defaults() {
        let source = "---\nparams:\n  - name = str\n  - count = int := 5\n  - label = str := \"default\"\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations.len(), 3);
        assert_eq!(fm.declarations[0].default_value, None);
        assert_eq!(fm.declarations[1].default_value, Some(Value::Int(5)));
        assert_eq!(
            fm.declarations[2].default_value,
            Some(Value::Str("default".to_string()))
        );
    }

    #[test]
    fn parse_nested_types() {
        let source = "---\nparams: [data = list<item = dict<name = str, tags = list<label = str>>>]\n---\nbody";
        let (fm, _) = parse_frontmatter(source).unwrap();
        match &fm.declarations[0].var_type {
            VarType::List(fields) => {
                assert_eq!(fields[0].name, "item");
                match &fields[0].var_type {
                    VarType::Dict(dict_fields) => {
                        assert_eq!(dict_fields[0].name, "name");
                        assert_eq!(dict_fields[0].var_type, VarType::Str);
                        match &dict_fields[1].var_type {
                            VarType::List(inner) => {
                                assert_eq!(inner[0].name, "label");
                                assert_eq!(inner[0].var_type, VarType::Str);
                            }
                            other => panic!("Expected inner List, got {other:?}"),
                        }
                    }
                    other => panic!("Expected Dict, got {other:?}"),
                }
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn default_value_accessor() {
        let decl = VarDecl {
            name: "test".to_string(),
            var_type: VarType::Str,
            default_value: Some(Value::Str("hello".to_string())),
        };
        assert_eq!(decl.default_value(), Some(&Value::Str("hello".to_string())));

        let no_default = VarDecl {
            name: "test".to_string(),
            var_type: VarType::Int,
            default_value: None,
        };
        assert_eq!(no_default.default_value(), None);
    }

    // -- Strict default type validation --

    #[test]
    fn reject_int_default_for_str_type() {
        let source = "---\nparams: [name = str := 42]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string().contains("value has type"),
            "expected type mismatch error, got: {err}"
        );
    }

    #[test]
    fn reject_str_default_for_int_type() {
        let source = "---\nparams: [count = int := \"hello\"]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string().contains("value has type"),
            "expected type mismatch error, got: {err}"
        );
    }

    #[test]
    fn reject_bool_default_for_float_type() {
        let source = "---\nparams: [score = float := true]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string().contains("value has type"),
            "expected type mismatch error, got: {err}"
        );
    }

    #[test]
    fn reject_float_default_for_bool_type() {
        let source = "---\nparams: [active = bool := 3.15]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string().contains("value has type"),
            "expected type mismatch error, got: {err}"
        );
    }

    #[test]
    fn accept_matching_int_default() {
        let source = "---\nparams: [count = int := 0]\n---\n{{ count }}";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].default_value, Some(Value::Int(0)));
    }

    #[test]
    fn accept_matching_str_default() {
        let source = "---\nparams: [name = str := \"hi\"]\n---\n{{ name }}";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(
            fm.declarations[0].default_value,
            Some(Value::Str("hi".to_string()))
        );
    }

    #[test]
    fn accept_matching_bool_default() {
        let source = "---\nparams: [active = bool := false]\n---\n{{ active }}";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].default_value, Some(Value::Bool(false)));
    }

    #[test]
    fn accept_matching_float_default() {
        let source = "---\nparams: [score = float := -1.5]\n---\n{{ score }}";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations[0].default_value, Some(Value::Float(-1.5)));
    }

    #[test]
    fn reject_negative_int_for_str() {
        let source = "---\nparams: [label = str := -99]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(err.to_string().contains("value has type"));
    }

    // -- Type library (allow_unused) tests --

    #[test]
    fn allow_unused_suppresses_unused_type_alias() {
        let source = "\
---
types:
  - Severity = enum<Low, Medium, High>
params:
  - x = str
allow_unused: true
---
type library";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert!(fm.allow_unused);
        assert!(fm.type_aliases.contains_key("Severity"));
    }

    #[test]
    fn reject_unused_type_alias_without_allow_unused() {
        let source = "\
---
types:
  - Severity = enum<Low, Medium, High>
params:
  - x = str
---
{{ x }}";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string().contains("unused type alias"),
            "expected unused type alias error, got: {err}"
        );
    }

    #[test]
    fn type_library_with_exported_types_and_params() {
        let source = "\
---
name: types
types:
  - Labelled = enum<Known(label = str), Unknown>
  - Severity = enum<Informational, Low, Medium, High, Critical>
params:
  - bugs = list<title = str, vuln_type = Labelled, component = Labelled>
  - post_types = list<tag = str>
allow_unused: true
---
{# type library #}";
        let (fm, _) = parse_frontmatter(source).unwrap();
        assert_eq!(fm.declarations.len(), 2);
        // Labelled is used by bugs param, so it remains in type_aliases.
        assert!(fm.type_aliases.contains_key("Labelled"));
        // Severity is NOT used by any param, but allow_unused suppresses the error.
        // It remains in the explicit type_aliases map.
        assert!(
            fm.type_aliases.contains_key("Severity"),
            "Severity should remain in type_aliases with allow_unused: {:?}",
            fm.type_aliases.keys().collect::<Vec<_>>()
        );
    }
}
