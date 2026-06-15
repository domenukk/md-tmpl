//! Parameter declaration parsing for frontmatter `params:` blocks.
//!
//! Handles both inline (`[name = str, count = int]`) and block
//! (`- name = str`) formats, including default values and nested types.

use std::collections::HashMap;

use super::ImportedNamespace;
use crate::{
    error::TemplateError,
    types::{VarDecl, VarType},
    value::Value,
};

/// Join YAML continuation lines: any line starting with whitespace is appended
/// to the preceding logical line.
pub(crate) fn join_continuation_lines(block: &str) -> Vec<String> {
    let mut logical: Vec<String> = Vec::new();
    for raw in block.lines() {
        if raw.starts_with(' ') || raw.starts_with('\t') {
            // Continuation of previous logical line.
            if let Some(prev) = logical.last_mut() {
                prev.push(' ');
                prev.push_str(raw.trim());
            } else {
                logical.push(raw.to_string());
            }
        } else {
            logical.push(raw.to_string());
        }
    }
    logical
}

/// Parse the value part after `params:` or `consts:`.
///
/// Supports both inline and block list formats:
/// - Inline: `[name = str, count = int]`
/// - Block (joined): `- name = str, - count = int` (after continuation joining)
pub(crate) fn parse_declarations(
    rest: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
    is_constant: bool,
) -> Result<Vec<VarDecl>, TemplateError> {
    let rest = rest.trim();
    if rest.is_empty() {
        // `params:` with no value and no continuation lines → empty params.
        return Ok(vec![]);
    }

    // Strip only the outermost `[` and `]` (inline YAML flow sequence).
    let inner = rest
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(rest);

    // Handle block list format: entries are `- name = type` joined by spaces
    // (after continuation line joining, the `- ` markers are preserved).
    let entries = if inner.contains("- ") {
        // Split on ` - ` to separate entries, then strip leading `- ` from
        // the first entry if present.
        let mut result = Vec::new();
        for part in inner.split(" - ") {
            let part = part.trim().strip_prefix('-').unwrap_or(part).trim();
            if !part.is_empty() {
                result.push(part.to_string());
            }
        }
        result
    } else {
        // Inline format: split on commas at bracket-depth 0.
        split_at_depth_zero(inner)
            .into_iter()
            .map(ToString::to_string)
            .collect()
    };

    let mut decls = Vec::new();
    let mut seen_names = std::collections::HashSet::new();
    for entry in &entries {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Find `=` at depth 0 to split name from type+default.
        let Some(eq_pos) = find_char_at_depth_zero(trimmed, '=') else {
            let label = if is_constant { "constant" } else { "param" };
            return Err(TemplateError::syntax(format!(
                "{label} '{trimmed}' is missing a type annotation (expected 'name = type')"
            )));
        };

        let name = trimmed[..eq_pos].trim().to_string();
        let type_and_default = trimmed[eq_pos + 1..].trim();

        // Check duplicate names.
        if !seen_names.insert(name.clone()) {
            let err = if is_constant {
                crate::consts::ERR_DUPLICATE_CONST
            } else {
                crate::consts::ERR_DUPLICATE_PARAM
            };
            return Err(TemplateError::syntax(format!("{err}: '{name}'")));
        }

        // Check reserved keywords.
        if crate::consts::RESERVED_NAMES.contains(&name.as_str()) {
            return Err(TemplateError::syntax(format!(
                "{}: '{name}'",
                crate::consts::ERR_RESERVED_KEYWORD
            )));
        }

        // Find `:=` at depth 0 to split type from default value.
        let (type_str, default_value) =
            if let Some(assign_pos) = find_assign_default_at_depth_zero(type_and_default) {
                let type_part = type_and_default[..assign_pos].trim();
                let default_part = type_and_default[assign_pos + 2..].trim();
                let default = parse_default_value(default_part);
                (type_part, default)
            } else {
                (type_and_default, None)
            };

        let var_type = parse_type_annotation(type_str, type_aliases, resolved_imports)
            .map_err(|e| TemplateError::syntax(format!("declaration '{name}': {e}")))?;

        // For constants, the default value is mandatory.
        if is_constant && default_value.is_none() {
            return Err(TemplateError::syntax(format!(
                "constant '{name}' is missing a value (expected 'name = type := value')"
            )));
        }

        // Validate that the default value matches the declared type.
        if let Some(ref default) = default_value
            && !var_type.matches(default)
        {
            let label = if is_constant { "constant" } else { "param" };
            return Err(TemplateError::syntax(format!(
                "{label} '{name}': value has type '{}' but declared type is '{var_type}'",
                default.type_name()
            )));
        }

        decls.push(VarDecl {
            name,
            var_type,
            default_value,
        });
    }

    Ok(decls)
}

// Compatibility wrapper for `params:` removed as it is now unused.

/// Split a string on commas at bracket-depth 0.
pub(crate) fn split_at_depth_zero(input: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;
    for (i, ch) in input.char_indices() {
        match ch {
            '<' | '[' | '(' | '{' => depth += 1,
            '>' | ']' | ')' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    entries.push(&input[start..]);
    entries
}

/// Find the first occurrence of `target` at bracket-depth 0.
pub(crate) fn find_char_at_depth_zero(input: &str, target: char) -> Option<usize> {
    let mut depth: u32 = 0;
    for (i, ch) in input.char_indices() {
        match ch {
            '<' | '[' | '(' | '{' => depth += 1,
            '>' | ']' | ')' | '}' => depth = depth.saturating_sub(1),
            c if c == target && depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find the position of `:=` at bracket-depth zero.
fn find_assign_default_at_depth_zero(input: &str) -> Option<usize> {
    let mut depth: u32 = 0;
    let bytes = input.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' | b'[' | b'(' | b'{' => depth += 1,
            b'>' | b']' | b')' | b'}' => depth = depth.saturating_sub(1),
            b':' if depth == 0 && bytes.get(i + 1) == Some(&b'=') => return Some(i),
            _ => {}
        }
    }
    None
}

/// Parse a type annotation string into a [`VarType`].
///
/// Supported forms:
/// - `str` → [`VarType::Str`]
/// - `bool` → [`VarType::Bool`]
/// - `int` → [`VarType::Int`]
/// - `float` → [`VarType::Float`]
/// - `list<name = str, count = int>` → [`VarType::List`] with field declarations
/// - `dict<key = str>` → [`VarType::Dict`] with field declarations
/// - `enum<A, B(field = type)>` → [`VarType::Enum`] with variant declarations
pub(crate) fn parse_type_annotation(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::{
        TYPE_BOOL, TYPE_DICT_PREFIX, TYPE_ENUM_PREFIX, TYPE_FLOAT, TYPE_INT, TYPE_LIST_PREFIX,
        TYPE_STR, TYPE_TMPL_PREFIX,
    };

    let s = s.trim();

    // Check type aliases first (own or inherited).
    if let Some(ty) = type_aliases.get(s) {
        return Ok(ty.clone());
    }

    // Check dotted import paths: `stem.TypeName`.
    if let Some(dot_pos) = s.find('.') {
        let stem = &s[..dot_pos];
        let type_name = &s[dot_pos + 1..];
        if let Some(ns) = resolved_imports.get(stem) {
            if let Some(ty) = ns.type_aliases.get(type_name) {
                return Ok(ty.clone());
            }
            if let Some(ty) = ns.param_types.get(type_name) {
                return Ok(ty.clone());
            }
            return Err(format!("import '{stem}' has no type '{type_name}'"));
        }
    }

    if s == TYPE_STR {
        Ok(VarType::Str)
    } else if s == TYPE_BOOL {
        Ok(VarType::Bool)
    } else if s == TYPE_INT {
        Ok(VarType::Int)
    } else if s == TYPE_FLOAT {
        Ok(VarType::Float)
    } else if s.starts_with(TYPE_LIST_PREFIX) {
        parse_compound_type_list(s, type_aliases, resolved_imports)
    } else if s.starts_with(TYPE_DICT_PREFIX) {
        parse_compound_type_dict(s, type_aliases, resolved_imports)
    } else if s.starts_with(TYPE_ENUM_PREFIX) {
        parse_enum_type(s, type_aliases, resolved_imports)
    } else if s.starts_with(TYPE_TMPL_PREFIX) {
        parse_tmpl_type(s, type_aliases, resolved_imports)
    } else {
        Err(format!("unknown type '{s}'"))
    }
}

/// Parse an enum type like `enum<Confirmed(evidence = list<text = str>), Inconclusive>`.
fn parse_enum_type(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::{consts::TYPE_ENUM, types::VariantDecl};

    let rest = s.strip_prefix(TYPE_ENUM).unwrap_or("").trim();
    let Some(inner) = rest.strip_prefix('<').and_then(|r| r.strip_suffix('>')) else {
        return Err(format!("malformed enum type: '{s}'"));
    };
    let entries = split_at_depth_zero(inner);
    let mut variants = Vec::new();
    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if let (Some(open_idx), Some(close_idx)) = (
            entry.find(crate::consts::PAREN_OPEN),
            entry.rfind(crate::consts::PAREN_CLOSE),
        ) {
            let name = entry[..open_idx].trim().to_string();
            let fields_str = &entry[open_idx + 1..close_idx];
            let fields = parse_field_declarations(fields_str, type_aliases, resolved_imports)?;
            variants.push(VariantDecl { name, fields });
            continue;
        }
        variants.push(VariantDecl {
            name: entry.to_string(),
            fields: vec![],
        });
    }
    Ok(VarType::Enum(variants))
}

/// Parse a compound type like `list<name = str, count = int>`.
fn parse_compound_type_list(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::TYPE_LIST;

    let rest = s.strip_prefix(TYPE_LIST).unwrap_or("").trim();
    let Some(inner) = rest.strip_prefix('<').and_then(|r| r.strip_suffix('>')) else {
        return Err(format!("malformed list type: '{s}'"));
    };
    let fields = parse_field_declarations(inner, type_aliases, resolved_imports)?;
    Ok(VarType::List(fields))
}

/// Parse a compound type like `dict<key = str, value = int>`.
fn parse_compound_type_dict(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::TYPE_DICT;

    let rest = s.strip_prefix(TYPE_DICT).unwrap_or("").trim();
    let Some(inner) = rest.strip_prefix('<').and_then(|r| r.strip_suffix('>')) else {
        return Err(format!("malformed dict type: '{s}'"));
    };
    let fields = parse_field_declarations(inner, type_aliases, resolved_imports)?;
    Ok(VarType::Dict(fields))
}

/// Parse a tmpl type like `tmpl<name = str, count = int>`.
fn parse_tmpl_type(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::TYPE_TMPL;

    let rest = s.strip_prefix(TYPE_TMPL).unwrap_or("").trim();
    let Some(inner) = rest.strip_prefix('<').and_then(|r| r.strip_suffix('>')) else {
        return Err(format!("malformed tmpl type: '{s}'"));
    };
    let fields = parse_field_declarations(inner, type_aliases, resolved_imports)?;
    Ok(VarType::Tmpl(fields))
}

/// Parse field declarations like `name = str, count = int` into [`VarDecl`]s.
fn parse_field_declarations(
    inner: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<Vec<VarDecl>, String> {
    let entries = split_at_depth_zero(inner);
    let mut decls = Vec::new();
    for f in &entries {
        let f = f.trim();
        if f.is_empty() {
            continue;
        }
        let (name, type_str) = if let Some(eq_pos) = find_char_at_depth_zero(f, '=') {
            (f[..eq_pos].trim().to_string(), f[eq_pos + 1..].trim())
        } else {
            (String::new(), f)
        };
        let var_type = parse_type_annotation(type_str, type_aliases, resolved_imports)?;
        decls.push(VarDecl {
            name,
            var_type,
            default_value: None,
        });
    }
    Ok(decls)
}

/// Parse a default value string into a [`Value`].
///
/// Supports:
/// - Inline lists: `[1, 2, 3]`
/// - Inline dicts: `{a: 1, b: 2}` (keys must be unquoted or quoted strings)
/// - Quoted strings: `"hello"` or `'hello'`
/// - Integers, floats, booleans
pub(crate) fn parse_default_value(s: &str) -> Option<Value> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Handle inline list: [a, b, c]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let entries = split_at_depth_zero(inner);
        let mut list = Vec::new();
        for e in entries {
            if let Some(v) = parse_default_value(e) {
                list.push(v);
            }
        }
        return Some(Value::List(list));
    }

    // Handle inline dict: {a: 1, b: 2}
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        let entries = split_at_depth_zero(inner);
        let mut map = HashMap::new();
        for e in entries {
            let e = e.trim();
            if e.is_empty() {
                continue;
            }
            if let Some(colon_pos) = find_char_at_depth_zero(e, ':') {
                let key = e[..colon_pos].trim();
                // Strip quotes from key if present.
                let key = crate::consts::strip_string_literal(key).unwrap_or(key);
                let val_str = &e[colon_pos + 1..];
                if let Some(v) = parse_default_value(val_str) {
                    map.insert(key.to_string(), v);
                }
            }
        }
        return Some(Value::Dict(map));
    }

    // Quoted string
    if let Some(inner) = crate::consts::strip_string_literal(s) {
        return Some(Value::Str(inner.to_string()));
    }

    // Boolean
    if s == crate::consts::LIT_TRUE {
        return Some(Value::Bool(true));
    }
    if s == crate::consts::LIT_FALSE {
        return Some(Value::Bool(false));
    }

    // Integer
    if let Ok(n) = s.parse::<i64>() {
        return Some(Value::Int(n));
    }

    // Float
    if let Ok(n) = s.parse::<f64>() {
        return Some(Value::Float(n));
    }

    // Intentional fallback: values that don't match any specific type (quoted
    // string, bool, int, float, list, dict) are treated as unquoted string
    // literals. This is by design — frontmatter default values like
    // `name = str := hello` should work without requiring quotes around the
    // value. The type-checking in `parse_declarations` will catch mismatches
    // between the declared type and this fallback value.
    Some(Value::Str(s.to_string()))
}
