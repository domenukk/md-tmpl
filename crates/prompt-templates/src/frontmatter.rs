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

use crate::{
    consts::{
        FM_ALLOW_UNUSED_PREFIX, FM_DELIMITER, FM_DELIMITER_NEWLINE, FM_DESC_PREFIX, FM_NAME_PREFIX,
        FM_PARAMS_PREFIX, TYPE_BOOL, TYPE_DICT, TYPE_DICT_PREFIX, TYPE_ENUM, TYPE_ENUM_PREFIX,
        TYPE_FLOAT, TYPE_INT, TYPE_LIST, TYPE_LIST_PREFIX, TYPE_STR,
    },
    error::TemplateError,
    types::{VarDecl, VarType, VariantDecl},
    value::Value,
};

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
/// missing, unclosed, or does not declare a `params` block.
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

    for line in &logical_lines {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(FM_NAME_PREFIX) {
            fm.name = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(FM_DESC_PREFIX) {
            fm.description = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(FM_PARAMS_PREFIX) {
            let decls = parse_params_value(rest)?;
            fm.params = decls.iter().map(|d| d.name.clone()).collect();
            fm.declarations = decls;
            fm.has_params = true;
        } else if let Some(rest) = line.strip_prefix(FM_ALLOW_UNUSED_PREFIX) {
            fm.allow_unused = rest.trim() == "true";
        }
    }

    if !fm.has_params {
        return Err(TemplateError::syntax(
            crate::consts::ERR_MISSING_PARAMS.to_string(),
        ));
    }

    Ok((fm, body))
}

/// Join YAML continuation lines: any line starting with whitespace is appended
/// to the preceding logical line.
fn join_continuation_lines(block: &str) -> Vec<String> {
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

/// Parse the value part after `params:`.
///
/// Supports both inline and block list formats:
/// - Inline: `[name = str, count = int]`
/// - Block (joined): `- name = str, - count = int` (after continuation joining)
fn parse_params_value(rest: &str) -> Result<Vec<VarDecl>, TemplateError> {
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
    for entry in &entries {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Find `=` at depth 0 to split name from type+default.
        let Some(eq_pos) = find_char_at_depth_zero(trimmed, '=') else {
            return Err(TemplateError::syntax(format!(
                "param '{trimmed}' is missing a type annotation (expected 'name = type')"
            )));
        };

        let name = trimmed[..eq_pos].trim().to_string();
        let type_and_default = trimmed[eq_pos + 1..].trim();

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

        let var_type = parse_type_annotation(type_str)
            .map_err(|e| TemplateError::syntax(format!("param '{name}': {e}")))?;

        // Validate that the default value matches the declared type.
        if let Some(ref default) = default_value
            && !var_type.matches(default)
        {
            return Err(TemplateError::syntax(format!(
                "param '{name}': default value has type '{}' but declared type is '{var_type}'",
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

/// Split a string on commas at bracket-depth 0.
fn split_at_depth_zero(input: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;
    for (i, ch) in input.char_indices() {
        match ch {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth = depth.saturating_sub(1),
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
fn find_char_at_depth_zero(input: &str, target: char) -> Option<usize> {
    let mut depth: u32 = 0;
    for (i, ch) in input.char_indices() {
        match ch {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth = depth.saturating_sub(1),
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
            b'<' | b'[' | b'(' => depth += 1,
            b'>' | b']' | b')' => depth = depth.saturating_sub(1),
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
fn parse_type_annotation(s: &str) -> Result<VarType, String> {
    let s = s.trim();

    if s == TYPE_STR {
        Ok(VarType::Str)
    } else if s == TYPE_BOOL {
        Ok(VarType::Bool)
    } else if s == TYPE_INT {
        Ok(VarType::Int)
    } else if s == TYPE_FLOAT {
        Ok(VarType::Float)
    } else if s.starts_with(TYPE_LIST_PREFIX) {
        parse_compound_type_list(s)
    } else if s.starts_with(TYPE_DICT_PREFIX) {
        parse_compound_type_dict(s)
    } else if s.starts_with(TYPE_ENUM_PREFIX) {
        parse_enum_type(s)
    } else {
        Err(format!("unknown type '{s}'"))
    }
}

/// Parse an enum type like `enum<Confirmed(evidence = list<text = str>), Inconclusive>`.
fn parse_enum_type(s: &str) -> Result<VarType, String> {
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
        if let (Some(open_idx), Some(close_idx)) = (entry.find('('), entry.rfind(')')) {
            let name = entry[..open_idx].trim().to_string();
            let fields_str = &entry[open_idx + 1..close_idx];
            let fields = parse_field_declarations(fields_str)?;
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
fn parse_compound_type_list(s: &str) -> Result<VarType, String> {
    let rest = s.strip_prefix(TYPE_LIST).unwrap_or("").trim();
    let Some(inner) = rest.strip_prefix('<').and_then(|r| r.strip_suffix('>')) else {
        return Err(format!("malformed list type: '{s}'"));
    };
    let fields = parse_field_declarations(inner)?;
    Ok(VarType::List(fields))
}

/// Parse a compound type like `dict<key = str, value = int>`.
fn parse_compound_type_dict(s: &str) -> Result<VarType, String> {
    let rest = s.strip_prefix(TYPE_DICT).unwrap_or("").trim();
    let Some(inner) = rest.strip_prefix('<').and_then(|r| r.strip_suffix('>')) else {
        return Err(format!("malformed dict type: '{s}'"));
    };
    let fields = parse_field_declarations(inner)?;
    Ok(VarType::Dict(fields))
}

/// Parse field declarations like `name = str, count = int` into [`VarDecl`]s.
fn parse_field_declarations(inner: &str) -> Result<Vec<VarDecl>, String> {
    let entries = split_at_depth_zero(inner);
    let mut decls = Vec::new();
    for f in &entries {
        let f = f.trim();
        if f.is_empty() {
            continue;
        }
        let Some(eq_pos) = find_char_at_depth_zero(f, '=') else {
            return Err(format!(
                "field '{f}' is missing a type annotation (expected 'name = type')"
            ));
        };
        let name = f[..eq_pos].trim().to_string();
        let type_str = f[eq_pos + 1..].trim();
        let var_type = parse_type_annotation(type_str)?;
        decls.push(VarDecl {
            name,
            var_type,
            default_value: None,
        });
    }
    Ok(decls)
}

/// Parse a scalar default value string into a [`Value`].
///
/// Supports:
/// - Quoted strings: `"hello"` or `'hello'`
/// - Integers: `42`, `-1`
/// - Floats: `3.15`, `-0.5`
/// - Booleans: `true`, `false`
fn parse_default_value(s: &str) -> Option<Value> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Quoted string
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        return Some(Value::Str(inner.to_string()));
    }

    // Boolean
    if s == "true" {
        return Some(Value::Bool(true));
    }
    if s == "false" {
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

    // Unquoted string fallback
    Some(Value::Str(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            err.to_string().contains("default value has type"),
            "expected type mismatch error, got: {err}"
        );
    }

    #[test]
    fn reject_str_default_for_int_type() {
        let source = "---\nparams: [count = int := \"hello\"]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string().contains("default value has type"),
            "expected type mismatch error, got: {err}"
        );
    }

    #[test]
    fn reject_bool_default_for_float_type() {
        let source = "---\nparams: [score = float := true]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string().contains("default value has type"),
            "expected type mismatch error, got: {err}"
        );
    }

    #[test]
    fn reject_float_default_for_bool_type() {
        let source = "---\nparams: [active = bool := 3.15]\n---\nbody";
        let err = parse_frontmatter(source).unwrap_err();
        assert!(
            err.to_string().contains("default value has type"),
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
        assert!(err.to_string().contains("default value has type"));
    }
}
