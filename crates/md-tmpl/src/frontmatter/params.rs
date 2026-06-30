//! Parameter declaration parsing for frontmatter `params:` blocks.
//!
//! Handles both inline (`[name = str, count = int]`) and block
//! (`- name = str`) formats, including default values and nested types.

use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use super::ImportedNamespace;
use crate::{
    compat::HashMap,
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
pub(crate) fn parse_declarations(
    rest: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
    is_constant: bool,
    available_consts: &HashMap<String, Value>,
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
    let mut seen_names = crate::compat::HashSet::new();
    let mut current_consts = available_consts.clone();
    for entry in &entries {
        let e = entry.trim();
        let trimmed = crate::consts::strip_string_literal(e).unwrap_or(e).trim();
        if let Some(decl) = parse_single_declaration(
            trimmed,
            type_aliases,
            resolved_imports,
            is_constant,
            &mut current_consts,
            &mut seen_names,
        )? {
            decls.push(decl);
        }
    }

    Ok(decls)
}

/// Parse a single declaration entry (e.g. `name = str := "default"`) into a [`VarDecl`].
fn parse_single_declaration(
    trimmed: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
    is_constant: bool,
    current_consts: &mut HashMap<String, Value>,
    seen_names: &mut crate::compat::HashSet<String>,
) -> Result<Option<VarDecl>, TemplateError> {
    if trimmed.is_empty() {
        return Ok(None);
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
    let (type_str, default_part) =
        if let Some(assign_pos) = find_assign_default_at_depth_zero(type_and_default) {
            (
                type_and_default[..assign_pos].trim(),
                Some(type_and_default[assign_pos + 2..].trim()),
            )
        } else {
            (type_and_default, None)
        };

    let var_type = parse_type_annotation(type_str, type_aliases, resolved_imports)
        .map_err(|e| TemplateError::syntax(format!("declaration '{name}': {e}")))?;

    let default_value = if let Some(dp) = default_part {
        let default = parse_default_value_with_type(dp, &var_type, current_consts)
            .or_else(|| resolve_const_default(dp, current_consts))
            .ok_or_else(|| {
                TemplateError::syntax(format!(
                    "invalid default value '{dp}' for declaration '{name}' (strings must be quoted)"
                ))
            })?;
        current_consts.insert(name.clone(), default.clone());
        Some(default)
    } else {
        None
    };

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

    Ok(Some(VarDecl {
        name,
        var_type,
        default_value,
    }))
}

// Compatibility wrapper for `params:` removed as it is now unused.

/// Strip enclosing compound type delimiter pair `(...)`.
pub(crate) fn strip_type_brackets(s: &str) -> Option<&str> {
    if let (Some(inner), true) = (
        s.strip_prefix(crate::consts::PAREN_OPEN),
        s.ends_with(crate::consts::PAREN_CLOSE),
    ) {
        Some(&inner[..inner.len() - 1])
    } else {
        None
    }
}

/// Split a string on commas at bracket-depth 0.
pub(crate) fn split_at_depth_zero(input: &str) -> Vec<&str> {
    use crate::consts::{
        ANGLE_CLOSE, ANGLE_OPEN, BRACE_CLOSE, BRACE_OPEN, BRACKET_CLOSE, BRACKET_OPEN, COMMA,
        PAREN_CLOSE, PAREN_OPEN,
    };
    let mut entries = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;
    for (i, ch) in input.char_indices() {
        match ch {
            ANGLE_OPEN | BRACKET_OPEN | PAREN_OPEN | BRACE_OPEN => depth += 1,
            ANGLE_CLOSE | BRACKET_CLOSE | PAREN_CLOSE | BRACE_CLOSE => {
                depth = depth.saturating_sub(1);
            }
            COMMA if depth == 0 => {
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
    use crate::consts::{
        ANGLE_CLOSE, ANGLE_OPEN, BRACE_CLOSE, BRACE_OPEN, BRACKET_CLOSE, BRACKET_OPEN, PAREN_CLOSE,
        PAREN_OPEN,
    };
    let mut depth: u32 = 0;
    for (i, ch) in input.char_indices() {
        match ch {
            ANGLE_OPEN | BRACKET_OPEN | PAREN_OPEN | BRACE_OPEN => depth += 1,
            ANGLE_CLOSE | BRACKET_CLOSE | PAREN_CLOSE | BRACE_CLOSE => {
                depth = depth.saturating_sub(1);
            }
            c if c == target && depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find the position of `:=` at bracket-depth zero.
fn find_assign_default_at_depth_zero(input: &str) -> Option<usize> {
    use crate::consts::{
        ANGLE_CLOSE_BYTE, ANGLE_OPEN_BYTE, BRACE_CLOSE_BYTE, BRACE_OPEN_BYTE, BRACKET_CLOSE_BYTE,
        BRACKET_OPEN_BYTE, COLON_BYTE, EQUALS_BYTE, PAREN_CLOSE_BYTE, PAREN_OPEN_BYTE,
    };
    let mut depth: u32 = 0;
    let bytes = input.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            ANGLE_OPEN_BYTE | BRACKET_OPEN_BYTE | PAREN_OPEN_BYTE | BRACE_OPEN_BYTE => depth += 1,
            ANGLE_CLOSE_BYTE | BRACKET_CLOSE_BYTE | PAREN_CLOSE_BYTE | BRACE_CLOSE_BYTE => {
                depth = depth.saturating_sub(1);
            }
            COLON_BYTE if depth == 0 && bytes.get(i + 1) == Some(&EQUALS_BYTE) => return Some(i),
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
/// - `list(name = str, count = int)` → [`VarType::List`] with field declarations
/// - `struct(key = str)` → [`VarType::Struct`] with field declarations
/// - `enum(A, B(field = type))` → [`VarType::Enum`] with variant declarations
///
/// # Errors
///
/// Returns an error string if the type annotation is malformed or
/// references an unknown type name.
fn starts_with_compound_type(s: &str, keyword: &str) -> bool {
    if let Some(rest) = s.strip_prefix(keyword) {
        let rest = rest.trim_start();
        rest.starts_with(crate::consts::PAREN_OPEN)
    } else {
        false
    }
}

/// Parses a type annotation string into a `VarType`.
///
/// # Errors
/// Returns an error string if the type annotation syntax is invalid or references an unknown type alias.
pub fn parse_type_annotation(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::{
        ANGLE_OPEN, BRACKET_OPEN, ERR_COMPOUND_BRACKETS_PROHIBITED, TYPE_BOOL, TYPE_ENUM,
        TYPE_FLOAT, TYPE_INT, TYPE_LIST, TYPE_OPTION, TYPE_STR, TYPE_STRUCT, TYPE_TMPL,
    };

    let s = crate::consts::strip_string_literal(s.trim())
        .unwrap_or(s.trim())
        .trim();

    for kw in &[TYPE_LIST, TYPE_STRUCT, TYPE_ENUM, TYPE_TMPL, TYPE_OPTION] {
        if let Some(rest) = s.strip_prefix(kw) {
            let rest_trimmed = rest.trim_start();
            if rest_trimmed.starts_with(ANGLE_OPEN) || rest_trimmed.starts_with(BRACKET_OPEN) {
                return Err(format!(
                    "compound type '{kw}': {ERR_COMPOUND_BRACKETS_PROHIBITED}"
                ));
            }
        }
    }

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
    } else if starts_with_compound_type(s, TYPE_LIST) {
        parse_compound_type_list(s, type_aliases, resolved_imports)
    } else if starts_with_compound_type(s, TYPE_STRUCT) {
        parse_compound_type_struct(s, type_aliases, resolved_imports)
    } else if starts_with_compound_type(s, TYPE_ENUM) {
        parse_enum_type(s, type_aliases, resolved_imports)
    } else if starts_with_compound_type(s, TYPE_TMPL) {
        parse_tmpl_type(s, type_aliases, resolved_imports)
    } else if starts_with_compound_type(s, TYPE_OPTION) {
        parse_option_type(s, type_aliases, resolved_imports)
    } else {
        Err(format!("unknown type '{s}'"))
    }
}

/// Parse an enum type like `enum(Confirmed(evidence = list(text = str)), Inconclusive)`.
fn parse_enum_type(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::{consts::TYPE_ENUM, types::VariantDecl};

    let rest = s.strip_prefix(TYPE_ENUM).unwrap_or("").trim();
    let Some(inner) = strip_type_brackets(rest) else {
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
            if fields.iter().any(|f| f.name.is_empty()) {
                return Err(
                    "enum struct variant must use named fields (e.g. Variant(name = str))"
                        .to_string(),
                );
            }
            variants.push(VariantDecl { name, fields });
            continue;
        }
        variants.push(VariantDecl {
            name: entry.to_string(),
            fields: vec![],
        });
    }
    if variants.is_empty() {
        return Err("enum must have at least one variant".to_string());
    }
    // Reject variant names that shadow builtin type keywords.
    for v in &variants {
        if crate::consts::RESERVED_NAMES.contains(&v.name.as_str()) {
            return Err(format!(
                "enum variant name '{}' shadows a builtin type keyword",
                v.name
            ));
        }
    }
    Ok(VarType::Enum(variants))
}

/// Parse a compound type like `list(name = str, count = int)`.
fn parse_compound_type_list(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::TYPE_LIST;

    let rest = s.strip_prefix(TYPE_LIST).unwrap_or("").trim();
    let Some(inner) = strip_type_brackets(rest) else {
        return Err(format!("malformed list type: '{s}'"));
    };
    let fields = parse_field_declarations(inner, type_aliases, resolved_imports)?;
    if fields.is_empty() {
        return Err("untyped list() is not allowed; must specify element type or fields (e.g., list(str) or list(name = str))".to_string());
    }
    if fields.len() > 1 && fields.iter().any(|f| f.name.is_empty()) {
        return Err(
            "list with multiple fields must use named fields (e.g. list(name = str, count = int))"
                .to_string(),
        );
    }
    // Reject literal raw struct declarations inside list definitions (e.g. list(struct(name = str, count = int))).
    // Users should write named fields directly (e.g. list(name = str, count = int)) or reference a strong Type alias.
    let inner_trimmed = inner.trim();
    if inner_trimmed.starts_with("struct<")
        || inner_trimmed.starts_with("struct(")
        || inner_trimmed.starts_with("struct[")
        || inner_trimmed.starts_with("struct ")
    {
        return Err(
            "list(struct(..)) is redundant; use named fields directly: list(name = str, count = int)"
                .to_string(),
        );
    }
    // If the inner type resolved to a strong struct alias (e.g. list(MyStruct)),
    // unwrap the struct fields directly into the list fields.
    if fields.len() == 1 && fields[0].name.is_empty() {
        if let VarType::Struct(ref struct_fields) = fields[0].var_type {
            return Ok(VarType::List(struct_fields.clone()));
        }
    }
    Ok(VarType::List(fields))
}

/// Parse a compound type like `struct(key = str, value = int)`.
fn parse_compound_type_struct(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::TYPE_STRUCT;

    let rest = s.strip_prefix(TYPE_STRUCT).unwrap_or("").trim();
    let Some(inner) = strip_type_brackets(rest) else {
        return Err(format!("malformed struct type: '{s}'"));
    };
    let fields = parse_field_declarations(inner, type_aliases, resolved_imports)?;
    if fields.is_empty() {
        return Err(
            "untyped struct() is not allowed; must specify fields (e.g., struct(name = str))"
                .to_string(),
        );
    }
    if fields.iter().any(|f| f.name.is_empty()) {
        return Err(
            "struct must use named fields (e.g. struct(name = str, count = int))".to_string(),
        );
    }
    Ok(VarType::Struct(fields))
}

/// Parse a tmpl type like `tmpl(name = str, count = int)`.
fn parse_tmpl_type(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::TYPE_TMPL;

    let rest = s.strip_prefix(TYPE_TMPL).unwrap_or("").trim();
    let Some(inner) = strip_type_brackets(rest) else {
        return Err(format!("malformed tmpl type: '{s}'"));
    };
    let fields = parse_field_declarations(inner, type_aliases, resolved_imports)?;
    if fields.iter().any(|f| f.name.is_empty()) {
        return Err("tmpl must use named fields (e.g. tmpl(name = str, count = int))".to_string());
    }
    Ok(VarType::Tmpl(fields))
}

/// Parse `option(T)` into [`VarType::Option`].
fn parse_option_type(
    s: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<VarType, String> {
    use crate::consts::TYPE_OPTION;

    let rest = s.strip_prefix(TYPE_OPTION).unwrap_or("").trim();
    let Some(inner) = strip_type_brackets(rest) else {
        return Err(format!("malformed option type: '{s}'"));
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return Err("option() requires an inner type (e.g. option(str))".to_string());
    }
    let inner_type = parse_type_annotation(inner, type_aliases, resolved_imports)?;
    Ok(VarType::Option(Box::new(inner_type)))
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

/// Parse the *inner* content of a `{key = value, ...}` struct default into
/// a [`Value::Struct`].
///
/// Uses `=` as the key-value separator (not `:`) and curly braces for
/// delimiters.
fn parse_struct_default(
    inner: &str,
    fields: &[VarDecl],
    available_consts: &HashMap<String, Value>,
) -> Value {
    let entries = split_at_depth_zero(inner);
    let mut map = HashMap::new();
    for e in entries {
        let e = e.trim();
        if e.is_empty() {
            continue;
        }
        if let Some(eq_pos) = find_char_at_depth_zero(e, '=') {
            let key = e[..eq_pos].trim();
            let val_str = e[eq_pos + 1..].trim();
            let field_type = fields
                .iter()
                .find(|d| d.name == key)
                .map_or(&VarType::Str, |d| &d.var_type);
            if let Some(v) = parse_default_value_with_type(val_str, field_type, available_consts) {
                map.insert(key.to_string(), v);
            }
        }
    }
    Value::Struct(Arc::new(map))
}

/// Resolve a const name used as a default value.
///
/// Looks up `name` in the available constants map, supporting both local
/// const names (e.g. `MAX`) and imported const names (e.g. `lib.LIMIT`).
/// Returns a clone of the const value if found.
fn resolve_const_default(name: &str, available_consts: &HashMap<String, Value>) -> Option<Value> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    available_consts.get(name).cloned()
}

/// Parse a default value string into a [`Value`].
///
/// Supports:
/// - Inline lists: `[1, 2, 3]` or `['a', 'b']`
/// - Inline structs: `{key = value, key2 = value2}`
/// - List of structs: `[{k = v1}, {k = v2}]`
/// - Quoted strings: `"hello"` or `'hello'`
/// - Integers, floats, booleans
///
/// Lists use `[]` and structs use `{}` with `=` as the key-value separator.
pub(crate) fn parse_default_value_with_type(
    s: &str,
    var_type: &VarType,
    available_consts: &HashMap<String, Value>,
) -> Option<Value> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Handle list defaults: [a, b, c]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        if inner.trim().is_empty() {
            return Some(Value::List(Arc::new(Vec::new())));
        }
        let entries = split_at_depth_zero(inner);
        let mut list = Vec::new();
        let elem_type = match var_type {
            VarType::List(fields) => {
                if fields.len() == 1 && fields[0].name.is_empty() {
                    &fields[0].var_type
                } else {
                    var_type
                }
            }
            _ => var_type,
        };
        for e in entries {
            if let Some(v) = parse_default_value_with_type(e, elem_type, available_consts) {
                list.push(v);
            }
        }
        return Some(Value::List(Arc::new(list)));
    }

    // Handle struct defaults: {key = value, ...}
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return match var_type {
                VarType::Struct(_) => Some(Value::Struct(Arc::new(HashMap::new()))),
                _ => None,
            };
        }

        let fields = match var_type {
            VarType::Struct(f) | VarType::List(f) => f.as_slice(),
            _ => &[],
        };
        return Some(parse_struct_default(inner, fields, available_consts));
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

    // Handle option(T) defaults: None maps to Value::None, otherwise delegate
    // to the inner type.
    if let VarType::Option(inner) = var_type {
        if s == crate::consts::OPTION_NONE {
            return Some(Value::None);
        }
        return parse_default_value_with_type(s, inner, available_consts);
    }

    // If the expected type is an Enum, handle variant identifiers.
    if let VarType::Enum(variants) = var_type {
        return parse_enum_default_value(s, variants, available_consts);
    }

    if let Some(val) = resolve_const_default(s, available_consts) {
        return Some(val);
    }

    // Intentional removal of fallback: unquoted strings are no longer allowed
    // as default values. All string defaults must be explicitly quoted.
    None
}

/// Parse a default value for an enum variant — either a unit variant name
/// (e.g. `Active`) or a struct variant with fields (e.g. `Error(msg = "oops")`).
fn parse_enum_default_value(
    s: &str,
    variants: &[crate::types::VariantDecl],
    available_consts: &HashMap<String, Value>,
) -> Option<Value> {
    // Check for struct variant default: VariantName(field = value, ...)
    // Uses () to match the type declaration syntax and avoid ambiguity
    // with <> which is used for struct/list defaults.
    if let Some(open_pos) = s.find(crate::consts::PAREN_OPEN) {
        if s.ends_with(crate::consts::PAREN_CLOSE) {
            let variant_name = s[..open_pos].trim();
            let inner = &s[open_pos + 1..s.len() - 1];
            // Find the variant declaration.
            let variant = variants.iter().find(|v| v.name == variant_name);
            match variant {
                Some(v) if v.fields.is_empty() => {
                    return None; // Unit variant can't have fields
                }
                Some(v) => {
                    // Parse field values and build a tagged dict.
                    let entries = split_at_depth_zero(inner);
                    let mut map = HashMap::new();
                    map.insert(
                        crate::consts::ENUM_TAG_KEY.to_string(),
                        Value::Str(variant_name.to_string()),
                    );
                    for e in entries {
                        let e = e.trim();
                        if e.is_empty() {
                            continue;
                        }
                        if let Some(eq_pos) = find_char_at_depth_zero(e, '=') {
                            let key = e[..eq_pos].trim();
                            let val_str = e[eq_pos + 1..].trim();
                            let field_type = v
                                .fields
                                .iter()
                                .find(|f| f.name == key)
                                .map_or(&VarType::Str, |f| &f.var_type);
                            if let Some(val) =
                                parse_default_value_with_type(val_str, field_type, available_consts)
                            {
                                map.insert(key.to_string(), val);
                            }
                        }
                    }
                    return Some(Value::Struct(Arc::new(map)));
                }
                None => return None, // Unknown variant
            }
        }
    }

    // Bare identifier — must be a known unit variant.
    let variant = variants.iter().find(|v| v.name == s);
    match variant {
        Some(v) if !v.fields.is_empty() => {
            // Struct variant without fields — reject.
            None
        }
        Some(_) => Some(Value::Str(s.to_string())),
        None => None, // Unknown variant name
    }
}

#[cfg(test)]
pub(crate) fn parse_default_value(s: &str) -> Option<Value> {
    parse_default_value_with_type(s, &VarType::Str, &HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compat::HashMap,
        types::{VarDecl, VarType},
        value::Value,
    };

    /// Helper: parse a type annotation with empty aliases/imports.
    fn parse_type(s: &str) -> Result<VarType, String> {
        let aliases = HashMap::new();
        let imports = HashMap::new();
        parse_type_annotation(s, &aliases, &imports)
    }

    /// Helper: parse declarations (params, not constants) with empty aliases/imports.
    fn parse_decls(rest: &str) -> Result<Vec<VarDecl>, crate::error::TemplateError> {
        let aliases = HashMap::new();
        let imports = HashMap::new();
        let consts = HashMap::new();
        parse_declarations(rest, &aliases, &imports, false, &consts)
    }

    /// Helper: parse constant declarations with empty aliases/imports.
    fn parse_consts(rest: &str) -> Result<Vec<VarDecl>, crate::error::TemplateError> {
        let aliases = HashMap::new();
        let imports = HashMap::new();
        let consts = HashMap::new();
        parse_declarations(rest, &aliases, &imports, true, &consts)
    }

    // =========================================================================
    // join_continuation_lines
    // =========================================================================

    #[test]
    fn join_normal_lines() {
        let block = "line1\nline2\nline3";
        let result = join_continuation_lines(block);
        assert_eq!(result, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn join_indented_continuation() {
        let block = "key:\n  continued\n  more";
        let result = join_continuation_lines(block);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "key: continued more");
    }

    #[test]
    fn join_tab_continuation() {
        let block = "key:\n\tcontinued\n\tmore";
        let result = join_continuation_lines(block);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "key: continued more");
    }

    #[test]
    fn join_first_line_indented() {
        // If the very first line is indented, there's no previous line to join to,
        // so it becomes its own logical line.
        let block = "  indented_first\nsecond";
        let result = join_continuation_lines(block);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "  indented_first");
        assert_eq!(result[1], "second");
    }

    #[test]
    fn join_multiple_groups() {
        let block = "key1: val1\n  continued1\nkey2: val2\n  continued2";
        let result = join_continuation_lines(block);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "key1: val1 continued1");
        assert_eq!(result[1], "key2: val2 continued2");
    }

    #[test]
    fn join_empty_block() {
        let result = join_continuation_lines("");
        assert!(result.is_empty());
    }

    #[test]
    fn join_no_continuations() {
        let block = "a\nb\nc";
        let result = join_continuation_lines(block);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    // =========================================================================
    // split_at_depth_zero
    // =========================================================================

    #[test]
    fn split_simple_comma() {
        let result = split_at_depth_zero("a, b, c");
        assert_eq!(result, vec!["a", " b", " c"]);
    }

    #[test]
    fn split_nested_angle_brackets_preserved() {
        let result = split_at_depth_zero("name = str, items = list<label = str, count = int>");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "name = str");
        assert_eq!(result[1], " items = list<label = str, count = int>");
    }

    #[test]
    fn split_nested_parens() {
        let result = split_at_depth_zero("A(x = str, y = int), B");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "A(x = str, y = int)");
        assert_eq!(result[1], " B");
    }

    #[test]
    fn split_empty_input() {
        let result = split_at_depth_zero("");
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn split_single_entry() {
        let result = split_at_depth_zero("only_one");
        assert_eq!(result, vec!["only_one"]);
    }

    #[test]
    fn split_nested_braces() {
        let result = split_at_depth_zero("{a: 1, b: 2}, c");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "{a: 1, b: 2}");
        assert_eq!(result[1], " c");
    }

    #[test]
    fn split_deeply_nested() {
        let result = split_at_depth_zero("list<list<a = str, b = list<c = int>>>, x = bool");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "list<list<a = str, b = list<c = int>>>");
        assert_eq!(result[1], " x = bool");
    }

    // =========================================================================
    // find_char_at_depth_zero
    // =========================================================================

    #[test]
    fn find_equals_at_depth_zero() {
        let result = find_char_at_depth_zero("name = str", '=');
        assert_eq!(result, Some(5));
    }

    #[test]
    fn find_skips_inside_angle_brackets() {
        let result = find_char_at_depth_zero("list<a = str>", '=');
        assert_eq!(result, None, "= inside <> should not be found at depth 0");
    }

    #[test]
    fn find_returns_none_when_not_found() {
        let result = find_char_at_depth_zero("no_target_here", '=');
        assert_eq!(result, None);
    }

    #[test]
    fn find_first_occurrence_at_depth_zero() {
        let result = find_char_at_depth_zero("a = b = c", '=');
        assert_eq!(result, Some(2));
    }

    #[test]
    fn find_inside_parens_skipped() {
        let result = find_char_at_depth_zero("fn(x = 1)", '=');
        assert_eq!(result, None);
    }

    #[test]
    fn find_after_brackets() {
        let result = find_char_at_depth_zero("list<a = str> = val", '=');
        assert_eq!(result, Some(14));
    }

    #[test]
    fn find_on_empty_input() {
        assert_eq!(find_char_at_depth_zero("", '='), None);
    }

    // =========================================================================
    // find_assign_default_at_depth_zero (internal, tested via parse_declarations)
    // =========================================================================

    #[test]
    fn find_assign_default_basic() {
        let result = find_assign_default_at_depth_zero("str := hello");
        assert_eq!(result, Some(4));
    }

    #[test]
    fn find_assign_default_skips_inside_brackets() {
        let result = find_assign_default_at_depth_zero("list<str := x>");
        assert_eq!(result, None);
    }

    #[test]
    fn find_assign_default_not_found() {
        let result = find_assign_default_at_depth_zero("str");
        assert_eq!(result, None);
    }

    #[test]
    fn find_assign_default_colon_without_equals() {
        // A bare `:` without `=` should not match.
        let result = find_assign_default_at_depth_zero("a: b");
        assert_eq!(result, None);
    }

    // =========================================================================
    // parse_default_value
    // =========================================================================

    #[test]
    fn parse_default_quoted_string() {
        assert_eq!(
            parse_default_value("\"hello\""),
            Some(Value::Str("hello".to_string()))
        );
    }

    #[test]
    fn parse_default_single_quoted_string() {
        assert_eq!(
            parse_default_value("'world'"),
            Some(Value::Str("world".to_string()))
        );
    }

    #[test]
    fn parse_default_integer() {
        assert_eq!(parse_default_value("42"), Some(Value::Int(42)));
    }

    #[test]
    fn parse_default_negative_integer() {
        assert_eq!(parse_default_value("-7"), Some(Value::Int(-7)));
    }

    #[test]
    fn parse_default_float() {
        assert_eq!(parse_default_value("3.125"), Some(Value::Float(3.125)));
    }

    #[test]
    fn parse_default_bool_true() {
        assert_eq!(parse_default_value("true"), Some(Value::Bool(true)));
    }

    #[test]
    fn parse_default_bool_false() {
        assert_eq!(parse_default_value("false"), Some(Value::Bool(false)));
    }

    #[test]
    fn parse_default_list() {
        let result = parse_default_value("[1, 2, 3]").unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::Int(1));
                assert_eq!(items[1], Value::Int(2));
                assert_eq!(items[2], Value::Int(3));
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn parse_default_dict() {
        let result = parse_default_value_with_type(
            "{a = 1, b = 2}",
            &VarType::Struct(vec![
                VarDecl {
                    name: "a".into(),
                    var_type: VarType::Int,
                    default_value: None,
                },
                VarDecl {
                    name: "b".into(),
                    var_type: VarType::Int,
                    default_value: None,
                },
            ]),
            &HashMap::new(),
        )
        .unwrap();
        match result {
            Value::Struct(map) => {
                assert_eq!(map.get("a"), Some(&Value::Int(1)));
                assert_eq!(map.get("b"), Some(&Value::Int(2)));
            }
            other => panic!("Expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn parse_default_empty_returns_none() {
        assert_eq!(parse_default_value(""), None);
    }

    #[test]
    fn parse_default_whitespace_only_returns_none() {
        assert_eq!(parse_default_value("   "), None);
    }

    #[test]
    fn parse_default_unquoted_string() {
        // Unquoted non-numeric/non-bool strings are no longer allowed.
        assert_eq!(parse_default_value("hello"), None);
    }

    #[test]
    fn parse_default_empty_list() {
        let result = parse_default_value("[]").unwrap();
        match result {
            Value::List(items) => assert!(items.is_empty()),
            other => panic!("Expected empty List, got {other:?}"),
        }
    }

    #[test]
    fn parse_default_empty_dict() {
        let result =
            parse_default_value_with_type("{}", &VarType::Struct(vec![]), &HashMap::new()).unwrap();
        match result {
            Value::Struct(map) => assert!(map.is_empty()),
            other => panic!("Expected empty Struct, got {other:?}"),
        }
    }

    #[test]
    fn parse_default_zero() {
        assert_eq!(parse_default_value("0"), Some(Value::Int(0)));
    }

    #[test]
    fn parse_default_float_zero() {
        assert_eq!(parse_default_value("0.0"), Some(Value::Float(0.0)));
    }

    #[test]
    fn parse_default_nested_list() {
        let result = parse_default_value("[1, [2, 3]]").unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Int(1));
                match &items[1] {
                    Value::List(inner) => {
                        assert_eq!(inner.len(), 2);
                        assert_eq!(inner[0], Value::Int(2));
                        assert_eq!(inner[1], Value::Int(3));
                    }
                    other => panic!("Expected inner List, got {other:?}"),
                }
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn parse_default_dict_with_quoted_keys() {
        let result = parse_default_value_with_type(
            "{key = 42}",
            &VarType::Struct(vec![VarDecl {
                name: "key".into(),
                var_type: VarType::Int,
                default_value: None,
            }]),
            &HashMap::new(),
        )
        .unwrap();
        match result {
            Value::Struct(map) => {
                assert_eq!(map.get("key"), Some(&Value::Int(42)));
            }
            other => panic!("Expected Struct, got {other:?}"),
        }
    }

    // =========================================================================
    // parse_type_annotation
    // =========================================================================

    #[test]
    fn type_str() {
        assert_eq!(parse_type("str").unwrap(), VarType::Str);
    }

    #[test]
    fn type_bool() {
        assert_eq!(parse_type("bool").unwrap(), VarType::Bool);
    }

    #[test]
    fn type_int() {
        assert_eq!(parse_type("int").unwrap(), VarType::Int);
    }

    #[test]
    fn type_float() {
        assert_eq!(parse_type("float").unwrap(), VarType::Float);
    }

    #[test]
    fn type_str_with_whitespace() {
        assert_eq!(parse_type("  str  ").unwrap(), VarType::Str);
    }

    #[test]
    fn type_list() {
        let result = parse_type("list(name = str)").unwrap();
        match result {
            VarType::List(fields) => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].name, "name");
                assert_eq!(fields[0].var_type, VarType::Str);
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn type_list_multiple_fields() {
        let result = parse_type("list(name = str, count = int)").unwrap();
        match result {
            VarType::List(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "name");
                assert_eq!(fields[0].var_type, VarType::Str);
                assert_eq!(fields[1].name, "count");
                assert_eq!(fields[1].var_type, VarType::Int);
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn type_struct() {
        let result = parse_type("struct(key = str, value = int)").unwrap();
        match result {
            VarType::Struct(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "key");
                assert_eq!(fields[0].var_type, VarType::Str);
                assert_eq!(fields[1].name, "value");
                assert_eq!(fields[1].var_type, VarType::Int);
            }
            other => panic!("Expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn type_enum_simple() {
        let result = parse_type("enum(A, B, C)").unwrap();
        match result {
            VarType::Enum(variants) => {
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].name, "A");
                assert!(variants[0].fields.is_empty());
                assert_eq!(variants[1].name, "B");
                assert_eq!(variants[2].name, "C");
            }
            other => panic!("Expected Enum, got {other:?}"),
        }
    }

    #[test]
    fn type_enum_with_fields() {
        let result = parse_type("enum(A, B(field = str))").unwrap();
        match result {
            VarType::Enum(variants) => {
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0].name, "A");
                assert!(variants[0].fields.is_empty());
                assert_eq!(variants[1].name, "B");
                assert_eq!(variants[1].fields.len(), 1);
                assert_eq!(variants[1].fields[0].name, "field");
                assert_eq!(variants[1].fields[0].var_type, VarType::Str);
            }
            other => panic!("Expected Enum, got {other:?}"),
        }
    }

    #[test]
    fn type_tmpl() {
        let result = parse_type("tmpl(name = str, count = int)").unwrap();
        match result {
            VarType::Tmpl(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "name");
                assert_eq!(fields[0].var_type, VarType::Str);
                assert_eq!(fields[1].name, "count");
                assert_eq!(fields[1].var_type, VarType::Int);
            }
            other => panic!("Expected Tmpl, got {other:?}"),
        }
    }

    #[test]
    fn type_unknown_errors() {
        let err = parse_type("garbage").unwrap_err();
        assert!(err.contains("unknown type"), "got: {err}");
    }

    #[test]
    fn type_bare_list_errors() {
        let err = parse_type("list").unwrap_err();
        assert!(err.contains("unknown type"), "got: {err}");
    }

    #[test]
    fn type_bare_struct_errors() {
        let err = parse_type("struct").unwrap_err();
        assert!(err.contains("unknown type"), "got: {err}");
    }

    #[test]
    fn type_nested_list_in_struct() {
        let result = parse_type("struct(items = list(name = str))").unwrap();
        match result {
            VarType::Struct(fields) => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].name, "items");
                match &fields[0].var_type {
                    VarType::List(inner) => {
                        assert_eq!(inner.len(), 1);
                        assert_eq!(inner[0].name, "name");
                        assert_eq!(inner[0].var_type, VarType::Str);
                    }
                    other => panic!("Expected inner List, got {other:?}"),
                }
            }
            other => panic!("Expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn type_alias_lookup() {
        let mut aliases = HashMap::new();
        aliases.insert("Priority".to_string(), VarType::Enum(vec![]));
        let imports = HashMap::new();
        let result = parse_type_annotation("Priority", &aliases, &imports).unwrap();
        assert_eq!(result, VarType::Enum(vec![]));
    }

    #[test]
    fn type_dotted_import_lookup() {
        let aliases = HashMap::new();
        let mut imports = HashMap::new();
        let mut ns = ImportedNamespace::default();
        ns.type_aliases.insert("Severity".to_string(), VarType::Str);
        imports.insert("types".to_string(), ns);
        let result = parse_type_annotation("types.Severity", &aliases, &imports).unwrap();
        assert_eq!(result, VarType::Str);
    }

    #[test]
    fn type_dotted_import_not_found() {
        let aliases = HashMap::new();
        let mut imports = HashMap::new();
        let ns = ImportedNamespace::default();
        imports.insert("types".to_string(), ns);
        let err = parse_type_annotation("types.Missing", &aliases, &imports).unwrap_err();
        assert!(err.contains("has no type"), "got: {err}");
    }

    // =========================================================================
    // parse_declarations (params mode)
    // =========================================================================

    #[test]
    fn decls_inline_basic() {
        let decls = parse_decls("[name = str, count = int]").unwrap();
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].name, "name");
        assert_eq!(decls[0].var_type, VarType::Str);
        assert_eq!(decls[1].name, "count");
        assert_eq!(decls[1].var_type, VarType::Int);
    }

    #[test]
    fn decls_empty_string() {
        let decls = parse_decls("").unwrap();
        assert!(decls.is_empty());
    }

    #[test]
    fn decls_empty_brackets() {
        let decls = parse_decls("[]").unwrap();
        assert!(decls.is_empty());
    }

    #[test]
    fn decls_with_default_values() {
        let decls = parse_decls("[name = str := \"hello\", count = int := 42]").unwrap();
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].name, "name");
        assert_eq!(decls[0].var_type, VarType::Str);
        assert_eq!(
            decls[0].default_value,
            Some(Value::Str("hello".to_string()))
        );
        assert_eq!(decls[1].name, "count");
        assert_eq!(decls[1].var_type, VarType::Int);
        assert_eq!(decls[1].default_value, Some(Value::Int(42)));
    }

    #[test]
    fn decls_mixed_default_and_required() {
        let decls = parse_decls("[name = str, count = int := 10]").unwrap();
        assert_eq!(decls[0].default_value, None);
        assert_eq!(decls[1].default_value, Some(Value::Int(10)));
    }

    #[test]
    fn decls_duplicate_name_error() {
        let err = parse_decls("[name = str, name = int]").unwrap_err();
        assert!(
            err.to_string().contains("duplicate parameter name"),
            "got: {err}"
        );
    }

    #[test]
    fn decls_reserved_keyword_error() {
        let err = parse_decls("[list = str]").unwrap_err();
        assert!(err.to_string().contains("reserved keyword"), "got: {err}");
    }

    #[test]
    fn decls_reserved_keyword_params() {
        let err = parse_decls("[params = str]").unwrap_err();
        assert!(err.to_string().contains("reserved keyword"), "got: {err}");
    }

    #[test]
    fn enum_variant_reserved_keyword_rejected() {
        let err = parse_decls("[x = enum(struct, ok)]").unwrap_err();
        assert!(
            err.to_string().contains("shadows a builtin type keyword"),
            "got: {err}"
        );
    }

    #[test]
    fn enum_variant_reserved_keyword_list_rejected() {
        let err = parse_decls("[x = enum(list, enum)]").unwrap_err();
        assert!(
            err.to_string().contains("shadows a builtin type keyword"),
            "got: {err}"
        );
    }

    #[test]
    fn decls_missing_type_annotation() {
        let err = parse_decls("[untyped_param]").unwrap_err();
        assert!(
            err.to_string().contains("missing a type annotation"),
            "got: {err}"
        );
    }

    #[test]
    fn decls_with_complex_types() {
        let decls =
            parse_decls("[items = list(name = str, score = float), active = bool]").unwrap();
        assert_eq!(decls.len(), 2);
        match &decls[0].var_type {
            VarType::List(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "name");
                assert_eq!(fields[1].name, "score");
                assert_eq!(fields[1].var_type, VarType::Float);
            }
            other => panic!("Expected List, got {other:?}"),
        }
        assert_eq!(decls[1].name, "active");
        assert_eq!(decls[1].var_type, VarType::Bool);
    }

    #[test]
    fn decls_block_format() {
        // After continuation joining, block entries look like:
        // "- name = str - count = int"
        let decls = parse_decls("- name = str - count = int").unwrap();
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].name, "name");
        assert_eq!(decls[0].var_type, VarType::Str);
        assert_eq!(decls[1].name, "count");
        assert_eq!(decls[1].var_type, VarType::Int);
    }

    #[test]
    fn decls_default_type_mismatch() {
        let err = parse_decls("[name = str := 42]").unwrap_err();
        assert!(
            err.to_string().contains("value has type"),
            "expected type mismatch error, got: {err}"
        );
    }

    // =========================================================================
    // parse_declarations (constants mode)
    // =========================================================================

    #[test]
    fn consts_requires_value() {
        let err = parse_consts("[MAX = int]").unwrap_err();
        assert!(err.to_string().contains("missing a value"), "got: {err}");
    }

    #[test]
    fn consts_with_value() {
        let decls = parse_consts("[MAX = int := 100]").unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "MAX");
        assert_eq!(decls[0].var_type, VarType::Int);
        assert_eq!(decls[0].default_value, Some(Value::Int(100)));
    }

    #[test]
    fn consts_duplicate_name_error() {
        let err = parse_consts("[A = int := 1, A = int := 2]").unwrap_err();
        assert!(
            err.to_string().contains("duplicate constant name"),
            "got: {err}"
        );
    }

    #[test]
    fn consts_reserved_keyword_error() {
        let err = parse_consts("[struct = str := \"hello\"]").unwrap_err();
        assert!(err.to_string().contains("reserved keyword"), "got: {err}");
    }

    #[test]
    fn consts_bool_default() {
        let decls = parse_consts("[ENABLED = bool := true]").unwrap();
        assert_eq!(decls[0].default_value, Some(Value::Bool(true)));
    }

    #[test]
    fn consts_str_default() {
        let decls = parse_consts("[GREETING = str := \"hi\"]").unwrap();
        assert_eq!(decls[0].default_value, Some(Value::Str("hi".to_string())));
    }

    #[test]
    fn untyped_list_fails() {
        let err = parse_decls("[items = list()]").unwrap_err();
        assert!(
            err.to_string().contains("untyped list() is not allowed"),
            "got: {err}"
        );
    }

    #[test]
    fn untyped_struct_fails() {
        let err = parse_decls("[data = struct()]").unwrap_err();
        assert!(
            err.to_string().contains("untyped struct() is not allowed"),
            "got: {err}"
        );
    }

    #[test]
    fn unnamed_multiple_fields_list_fails() {
        let err = parse_decls("[items = list(str, int)]").unwrap_err();
        assert!(
            err.to_string()
                .contains("list with multiple fields must use named fields"),
            "got: {err}"
        );
    }

    #[test]
    fn unquoted_string_default_fails() {
        let err = parse_decls("[name = str := hello]").unwrap_err();
        assert!(
            err.to_string().contains("strings must be quoted"),
            "got: {err}"
        );
    }

    #[test]
    fn consts_type_mismatch() {
        let err = parse_consts("[X = int := \"not_a_number\"]").unwrap_err();
        assert!(
            err.to_string().contains("value has type"),
            "expected type mismatch, got: {err}"
        );
    }

    // =========================================================================
    // Enum defaults and consts
    // =========================================================================

    #[test]
    fn enum_unit_variant_default() {
        let decls = parse_decls("[status = enum(Active, Paused) := Active]").unwrap();
        assert_eq!(
            decls[0].default_value,
            Some(Value::Str("Active".to_string()))
        );
    }

    #[test]
    fn enum_unit_variant_default_on_mixed_enum() {
        // Unit variant default on an enum that also has struct variants.
        let decls =
            parse_decls("[outcome = enum(Confirmed(evidence = str), Rejected) := Rejected]")
                .unwrap();
        assert_eq!(
            decls[0].default_value,
            Some(Value::Str("Rejected".to_string()))
        );
    }

    #[test]
    fn enum_struct_variant_default() {
        // Struct variant default with inline field values.
        let decls = parse_decls(
            "[outcome = enum(Confirmed(evidence = str), Rejected) := Confirmed(evidence = \"found it\")]",
        )
        .unwrap();
        let default = decls[0].default_value.as_ref().unwrap();
        match default {
            Value::Struct(map) => {
                assert_eq!(
                    map.get("__kind__"),
                    Some(&Value::Str("Confirmed".to_string())),
                    "should have __kind__ tag"
                );
                assert_eq!(
                    map.get("evidence"),
                    Some(&Value::Str("found it".to_string())),
                    "should have evidence field"
                );
            }
            other => panic!("Expected Struct for struct variant, got {other:?}"),
        }
    }

    #[test]
    fn enum_struct_variant_default_multiple_fields() {
        let decls = parse_decls(
            "[r = enum(Success(msg = str, code = int), Failure) := Success(msg = \"ok\", code = 200)]",
        )
        .unwrap();
        let default = decls[0].default_value.as_ref().unwrap();
        match default {
            Value::Struct(map) => {
                assert_eq!(map.get("__kind__"), Some(&Value::Str("Success".into())));
                assert_eq!(map.get("msg"), Some(&Value::Str("ok".into())));
                assert_eq!(map.get("code"), Some(&Value::Int(200)));
            }
            other => panic!("Expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn enum_struct_variant_const() {
        let decls =
            parse_consts("[RESULT = enum(Success(msg = str), Failure) := Success(msg = \"done\")]")
                .unwrap();
        let default = decls[0].default_value.as_ref().unwrap();
        match default {
            Value::Struct(map) => {
                assert_eq!(map.get("__kind__"), Some(&Value::Str("Success".into())));
                assert_eq!(map.get("msg"), Some(&Value::Str("done".into())));
            }
            other => panic!("Expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn enum_bare_struct_variant_rejected() {
        // Struct variant without fields → must be rejected.
        let err = parse_decls("[outcome = enum(Confirmed(evidence = str), Rejected) := Confirmed]")
            .unwrap_err();
        assert!(
            err.to_string().contains("strings must be quoted")
                || err.to_string().contains("invalid default"),
            "bare struct variant should be rejected, got: {err}"
        );
    }

    #[test]
    fn enum_unknown_variant_rejected() {
        let err = parse_decls("[status = enum(Active, Paused) := Nonexistent]").unwrap_err();
        assert!(
            err.to_string().contains("invalid default")
                || err.to_string().contains("strings must be quoted"),
            "unknown variant should be rejected, got: {err}"
        );
    }

    #[test]
    fn enum_unit_variant_with_fields_rejected() {
        // Trying to give fields to a unit variant.
        let err = parse_decls("[status = enum(Active, Paused) := Active(x = 1)]").unwrap_err();
        assert!(
            err.to_string().contains("invalid default"),
            "unit variant with fields should be rejected, got: {err}"
        );
    }

    // =========================================================================
    // Type nesting rules
    // =========================================================================

    // --- Positive: valid nesting ---

    #[test]
    fn list_of_scalars_valid() {
        let decls = parse_decls("[items = list(str)]").unwrap();
        assert!(matches!(&decls[0].var_type, VarType::List(fields) if fields.len() == 1));
    }

    #[test]
    fn list_with_named_fields_valid() {
        let decls = parse_decls("[items = list(name = str, score = int)]").unwrap();
        assert!(matches!(&decls[0].var_type, VarType::List(fields) if fields.len() == 2));
    }

    #[test]
    fn list_of_list_valid() {
        // Nested lists (e.g. matrix, grid, coordinates).
        let decls = parse_decls("[grid = list(list(str))]").unwrap();
        if let VarType::List(fields) = &decls[0].var_type {
            assert_eq!(fields.len(), 1);
            assert!(matches!(&fields[0].var_type, VarType::List(_)));
        } else {
            panic!("expected list type");
        }
    }

    #[test]
    fn list_of_enum_valid() {
        // List where each element is an enum value.
        let decls = parse_decls("[statuses = list(enum(Active, Paused))]").unwrap();
        if let VarType::List(fields) = &decls[0].var_type {
            assert_eq!(fields.len(), 1);
            assert!(matches!(&fields[0].var_type, VarType::Enum(_)));
        } else {
            panic!("expected list type");
        }
    }

    #[test]
    fn struct_with_list_field_valid() {
        let decls = parse_decls("[cfg = struct(tags = list(str), name = str)]").unwrap();
        if let VarType::Struct(fields) = &decls[0].var_type {
            assert_eq!(fields.len(), 2);
            assert!(matches!(&fields[0].var_type, VarType::List(_)));
        } else {
            panic!("expected struct type");
        }
    }

    #[test]
    fn struct_with_enum_field_valid() {
        let decls = parse_decls("[cfg = struct(status = enum(On, Off), name = str)]").unwrap();
        if let VarType::Struct(fields) = &decls[0].var_type {
            assert_eq!(fields.len(), 2);
            assert!(matches!(&fields[0].var_type, VarType::Enum(_)));
        } else {
            panic!("expected struct type");
        }
    }

    #[test]
    fn struct_with_nested_struct_field_valid() {
        let decls =
            parse_decls("[cfg = struct(inner = struct(x = int, y = int), name = str)]").unwrap();
        if let VarType::Struct(fields) = &decls[0].var_type {
            assert_eq!(fields.len(), 2);
            assert!(matches!(&fields[0].var_type, VarType::Struct(_)));
        } else {
            panic!("expected struct type");
        }
    }

    #[test]
    fn list_of_list_of_int_valid() {
        // Matrix of ints — deeply nested.
        let decls = parse_decls("[matrix = list(list(int))]").unwrap();
        if let VarType::List(outer) = &decls[0].var_type {
            if let VarType::List(inner) = &outer[0].var_type {
                assert_eq!(inner.len(), 1);
                assert!(matches!(&inner[0].var_type, VarType::Int));
            } else {
                panic!("expected inner list");
            }
        } else {
            panic!("expected outer list");
        }
    }

    // --- Negative: forbidden nesting ---

    #[test]
    fn list_of_raw_struct_rejected_as_redundant() {
        let err = parse_decls("[items = list(struct(name = str, score = int))]").unwrap_err();
        assert!(err.to_string().contains("redundant"), "got: {err}");
    }

    #[test]
    fn list_of_strong_struct_alias_unwraps_cleanly() {
        let mut aliases = HashMap::new();
        aliases.insert(
            "MyItem".to_string(),
            VarType::Struct(vec![
                VarDecl {
                    name: "name".to_string(),
                    var_type: VarType::Str,
                    default_value: None,
                },
                VarDecl {
                    name: "score".to_string(),
                    var_type: VarType::Int,
                    default_value: None,
                },
            ]),
        );
        let var_type = parse_type_annotation("list(MyItem)", &aliases, &HashMap::new()).unwrap();
        if let VarType::List(ref fields) = var_type {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "name");
            assert_eq!(fields[1].name, "score");
        } else {
            panic!("expected VarType::List");
        }
    }

    #[test]
    fn list_of_named_struct_field_allowed() {
        let decls = parse_decls("[items = list(item = struct(name = str, score = int))]").unwrap();
        if let VarType::List(ref fields) = decls[0].var_type {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name, "item");
            if let VarType::Struct(ref inner) = fields[0].var_type {
                assert_eq!(inner.len(), 2);
                assert_eq!(inner[0].name, "name");
                assert_eq!(inner[1].name, "score");
            } else {
                panic!("expected inner VarType::Struct");
            }
        } else {
            panic!("expected VarType::List");
        }
    }

    // =========================================================================
    // option(T) type parsing
    // =========================================================================

    #[test]
    fn type_option_str() {
        let result = parse_type("option(str)").unwrap();
        match result {
            VarType::Option(inner) => {
                assert_eq!(*inner, VarType::Str);
            }
            other => panic!("Expected Option, got {other:?}"),
        }
    }

    #[test]
    fn type_option_int() {
        let result = parse_type("option(int)").unwrap();
        assert!(result.is_option());
        assert_eq!(*result.option_inner_type().unwrap(), VarType::Int);
    }

    #[test]
    fn type_option_with_spaces() {
        let result = parse_type("option( str )").unwrap();
        assert!(result.is_option());
        assert_eq!(*result.option_inner_type().unwrap(), VarType::Str);
    }

    #[test]
    fn type_option_nested_list() {
        let result = parse_type("option(list(name = str))").unwrap();
        assert!(result.is_option());
        assert!(matches!(
            result.option_inner_type().unwrap(),
            VarType::List(_)
        ));
    }

    #[test]
    fn type_option_nested_struct() {
        let result = parse_type("option(struct(x = int, y = int))").unwrap();
        assert!(result.is_option());
        assert!(matches!(
            result.option_inner_type().unwrap(),
            VarType::Struct(_)
        ));
    }

    #[test]
    fn type_option_nested_option() {
        let result = parse_type("option(option(str))").unwrap();
        assert!(result.is_option());
        let inner = result.option_inner_type().unwrap();
        assert!(inner.is_option());
        assert_eq!(*inner.option_inner_type().unwrap(), VarType::Str);
    }

    #[test]
    fn type_option_display() {
        let result = parse_type("option(str)").unwrap();
        assert_eq!(format!("{result}"), "option(str)");
    }

    #[test]
    fn type_option_display_nested() {
        let result = parse_type("option(option(int))").unwrap();
        assert_eq!(format!("{result}"), "option(option(int))");
    }

    #[test]
    fn type_option_empty_rejected() {
        assert!(parse_type("option()").is_err());
    }

    #[test]
    fn type_option_malformed_rejected() {
        assert!(parse_type("option").is_err());
        assert!(parse_type("option(").is_err());
    }

    #[test]
    fn option_default_none() {
        let decls = parse_decls("[x = option(str) := None]").unwrap();
        assert_eq!(decls[0].default_value, Some(Value::None));
    }

    #[test]
    fn option_default_some() {
        // Transparent option: := "hello" stores the raw string, not a Some(val=...) struct.
        let decls = parse_decls("[x = option(str) := \"hello\"]").unwrap();
        assert_eq!(decls[0].default_value, Some(Value::Str("hello".into())));
    }

    #[test]
    fn option_reserved_name() {
        let result = parse_decls("[option = str]");
        assert!(result.is_err());
    }

    #[test]
    fn list_of_option() {
        let result = parse_type("list(option(str))").unwrap();
        match result {
            VarType::List(fields) => {
                assert_eq!(fields.len(), 1);
                assert!(fields[0].var_type.is_option());
            }
            other => panic!("Expected List, got {other:?}"),
        }
    }

    #[test]
    fn prohibited_angle_and_square_brackets() {
        assert!(parse_type("list<str>").is_err());
        assert!(parse_type("list[str]").is_err());
        assert!(parse_type("struct<x = int>").is_err());
        assert!(parse_type("struct[x = int]").is_err());
        assert!(parse_type("enum<A, B>").is_err());
        assert!(parse_type("enum[A, B]").is_err());
        assert!(parse_type("tmpl<x = int>").is_err());
        assert!(parse_type("tmpl[x = int]").is_err());
        assert!(parse_type("option<str>").is_err());
        assert!(parse_type("option[str]").is_err());
    }
}
