//! Type alias parsing for frontmatter `types:` blocks.
//!
//! Parses declarations like `types: [Priority = enum<High, Medium, Low>]`
//! into a map of alias name → [`VarType`].
//!
//! Uses `=` as the separator between alias name and type expression,
//! consistent with `params:` and `consts:` blocks.

use std::collections::HashMap;

use super::params::{find_char_at_depth_zero, parse_type_annotation, split_at_depth_zero};
use crate::{error::TemplateError, types::VarType};

/// Parse the value part after `types:` into a map of alias name → [`VarType`].
///
/// Format: `[Priority = enum<High, Medium, Low>, Items = list<text = str>]`
/// or block list format similar to `params:`. Uses `=` as separator.
pub(crate) fn parse_types_value(rest: &str) -> Result<HashMap<String, VarType>, TemplateError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(HashMap::new());
    }

    let inner = rest
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(rest);

    let entries = split_type_entries(inner);
    let mut aliases = HashMap::new();
    let empty_imports = HashMap::new();

    for entry in &entries {
        let trimmed = entry.trim().strip_prefix('-').unwrap_or(entry).trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some(eq_pos) = find_char_at_depth_zero(trimmed, '=') else {
            return Err(TemplateError::syntax(format!(
                "type entry '{trimmed}' is missing a type definition (expected 'Name = TypeExpr')"
            )));
        };

        let type_name = trimmed[..eq_pos].trim().to_string();
        let type_expr = trimmed[eq_pos + 1..].trim();

        // Validate: type name must not shadow builtins.
        if crate::types::BUILTIN_TYPE_NAMES.contains(&type_name.to_lowercase().as_str()) {
            return Err(TemplateError::syntax(format!(
                "{}: '{type_name}'",
                crate::consts::ERR_BUILTIN_SHADOW
            )));
        }

        // Validate: no duplicate type alias names.
        if aliases.contains_key(&type_name) {
            return Err(TemplateError::syntax(format!(
                "{}: '{type_name}'",
                crate::consts::ERR_DUPLICATE_TYPE_ALIAS
            )));
        }

        // Parse the type expression using already-defined aliases for chained refs.
        let var_type = parse_type_annotation(type_expr, &aliases, &empty_imports)
            .map_err(|e| TemplateError::syntax(format!("type '{type_name}': {e}")))?;

        aliases.insert(type_name, var_type);
    }

    Ok(aliases)
}

/// Split type entries, handling depth-aware comma splitting.
///
/// Type entries use `=` as separator between name and type,
/// so we split on commas at depth 0.
fn split_type_entries(input: &str) -> Vec<String> {
    // Handle block format with `- ` markers.
    if input.contains(" - ") || input.starts_with("- ") {
        let mut result = Vec::new();
        for part in input.split(" - ") {
            let part = part.trim().strip_prefix('-').unwrap_or(part).trim();
            if !part.is_empty() {
                result.push(part.to_string());
            }
        }
        return result;
    }

    // Inline format: split on commas at depth 0.
    split_at_depth_zero(input)
        .into_iter()
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VarType;

    #[test]
    fn list_type_alias() {
        let aliases = parse_types_value("[BugList = list<title = str, score = int>]").unwrap();
        assert!(aliases.contains_key("BugList"));
        assert!(matches!(aliases["BugList"], VarType::List(_)));
    }

    #[test]
    fn dict_type_alias() {
        let aliases = parse_types_value("[Config = dict<timeout = int, retries = int>]").unwrap();
        assert!(aliases.contains_key("Config"));
        assert!(matches!(aliases["Config"], VarType::Dict(_)));
    }

    #[test]
    fn chained_alias_enum_inside_list() {
        // Declare Severity enum first, then use it in BugReport list.
        let input = "- Severity = enum<High, Medium, Low>\n  - BugReport = list<title = str, severity = Severity>";
        let aliases = parse_types_value(input).unwrap();
        assert!(aliases.contains_key("Severity"));
        assert!(aliases.contains_key("BugReport"));
        if let VarType::List(fields) = &aliases["BugReport"] {
            let sev_field = fields.iter().find(|f| f.name == "severity").unwrap();
            assert!(matches!(sev_field.var_type, VarType::Enum(_)));
        } else {
            panic!("BugReport should be List");
        }
    }

    #[test]
    fn chained_alias_list_inside_dict() {
        let input = "[Items = list<name = str>, Config = dict<items = Items, version = int>]";
        let aliases = parse_types_value(input).unwrap();
        assert!(aliases.contains_key("Config"));
        if let VarType::Dict(fields) = &aliases["Config"] {
            let items_field = fields.iter().find(|f| f.name == "items").unwrap();
            assert!(matches!(items_field.var_type, VarType::List(_)));
        } else {
            panic!("Config should be Dict");
        }
    }

    #[test]
    fn str_type_alias_rejected_as_builtin_shadow() {
        let err = parse_types_value("[str = enum<A, B>]").unwrap_err();
        assert!(
            err.to_string().contains("shadow") || err.to_string().contains("builtin"),
            "shadowing builtin should error: {err}"
        );
    }

    #[test]
    fn duplicate_type_alias_rejected() {
        let err = parse_types_value("[Foo = str, Foo = int]").unwrap_err();
        assert!(
            err.to_string().contains("duplicate") || err.to_string().contains("Foo"),
            "duplicate should error: {err}"
        );
    }

    #[test]
    fn empty_types_block() {
        let aliases = parse_types_value("").unwrap();
        assert!(aliases.is_empty());
    }
}
