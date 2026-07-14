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

/// Join YAML continuation lines into one logical line per top-level entry.
///
/// Any line starting with whitespace is appended to the preceding logical line.
///
/// Blank lines and full-line `#` comments are layout/documentation only: they
/// are skipped entirely and, crucially, do **not** terminate an in-progress
/// block list. This lets block entries be separated by blank lines or
/// interleaved with comments for readability (e.g. a documented `consts:`
/// block) while still joining every entry onto its section's logical line
/// instead of orphaning entries after the first blank onto a stray line that
/// no section prefix matches.
pub(crate) fn join_continuation_lines(block: &str) -> Vec<String> {
    let mut logical: Vec<String> = Vec::new();
    for raw in block.lines() {
        let trimmed = raw.trim();
        // Skip blanks and full-line comments without breaking continuation.
        if trimmed.is_empty() || trimmed.starts_with(crate::consts::FM_COMMENT_PREFIX) {
            continue;
        }
        if raw.starts_with(' ') || raw.starts_with('\t') {
            // Continuation of previous logical line.
            if let Some(prev) = logical.last_mut() {
                prev.push(' ');
                prev.push_str(trimmed);
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
        .strip_prefix(crate::consts::BRACKET_OPEN)
        .and_then(|s| s.strip_suffix(crate::consts::BRACKET_CLOSE))
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
    let Some(eq_pos) = find_char_at_depth_zero(trimmed, crate::consts::EQUALS) else {
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
        let default = parse_default_value_full(
            dp,
            &var_type,
            current_consts,
            type_aliases,
            resolved_imports,
        )
        .or_else(|| resolve_const_default(dp, current_consts))
        .or_else(|| resolve_kinds_default(dp, type_aliases, resolved_imports))
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

/// Split a string on commas at bracket-depth 0, ignoring commas inside quoted
/// string literals.
///
/// Delimiters (brackets, braces, parens, angle brackets, and the separating
/// comma) that appear inside a `"..."` or `'...'` string literal are treated as
/// literal characters. This lets struct/list default values contain quoted
/// strings with embedded commas or brackets (e.g. `{msg = "a, b", n = 1}`)
/// without the field separator being misdetected.
pub(crate) fn split_at_depth_zero(input: &str) -> Vec<&str> {
    use crate::consts::{
        ANGLE_CLOSE, ANGLE_OPEN, BRACE_CLOSE, BRACE_OPEN, BRACKET_CLOSE, BRACKET_OPEN, COMMA,
        PAREN_CLOSE, PAREN_OPEN, QUOTE_DOUBLE, QUOTE_SINGLE,
    };
    let mut entries = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;
    // When inside a string literal, holds the opening quote char; delimiters are
    // ignored until the matching closing quote is seen.
    let mut in_quote: Option<char> = None;
    for (i, ch) in input.char_indices() {
        if let Some(q) = in_quote {
            if ch == q {
                in_quote = None;
            }
            continue;
        }
        match ch {
            QUOTE_DOUBLE | QUOTE_SINGLE => in_quote = Some(ch),
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
    if let Some(dot_pos) = s.find(crate::consts::PATH_SEP) {
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
    if inner_trimmed.starts_with(crate::consts::TYPE_STRUCT_ANGLE_PREFIX)
        || inner_trimmed.starts_with(crate::consts::TYPE_STRUCT_PREFIX)
        || inner_trimmed.starts_with(crate::consts::TYPE_STRUCT_BRACKET_PREFIX)
        || inner_trimmed.starts_with(crate::consts::TYPE_STRUCT_SPACE_PREFIX)
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
        let (name, type_str) =
            if let Some(eq_pos) = find_char_at_depth_zero(f, crate::consts::EQUALS) {
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
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Value {
    let entries = split_at_depth_zero(inner);
    let mut map = HashMap::new();
    for e in entries {
        let e = e.trim();
        if e.is_empty() {
            continue;
        }
        if let Some(eq_pos) = find_char_at_depth_zero(e, crate::consts::EQUALS) {
            let key = e[..eq_pos].trim();
            let val_str = e[eq_pos + 1..].trim();
            let field_type = fields
                .iter()
                .find(|d| d.name == key)
                .map_or(&VarType::Str, |d| &d.var_type);
            if let Some(v) = parse_default_value_full(
                val_str,
                field_type,
                available_consts,
                type_aliases,
                resolved_imports,
            ) {
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

/// Resolve a function expression default like `kinds(EnumType)` into a [`Value::List`].
fn resolve_kinds_default(
    expr: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Option<Value> {
    let s = expr.trim();
    let inner = s
        .strip_prefix(crate::consts::FN_KINDS)?
        .strip_prefix(crate::consts::PAREN_OPEN)?
        .strip_suffix(crate::consts::PAREN_CLOSE)?
        .trim();
    if inner.is_empty() {
        return None;
    }
    let var_type = if let Some(dot_pos) = inner.find(crate::consts::PATH_SEP) {
        let ns_name = &inner[..dot_pos];
        let type_name = &inner[dot_pos + 1..];
        resolved_imports.get(ns_name)?.type_aliases.get(type_name)
    } else {
        type_aliases.get(inner)
    };
    if let Some(VarType::Enum(variants)) = var_type {
        let list: Vec<Value> = variants
            .iter()
            .map(|v| Value::Str(v.name.clone()))
            .collect();
        Some(Value::List(Arc::new(list)))
    } else {
        None
    }
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
#[cfg(test)]
pub(crate) fn parse_default_value_with_type(
    s: &str,
    var_type: &VarType,
    available_consts: &HashMap<String, Value>,
) -> Option<Value> {
    let empty_aliases = HashMap::new();
    let empty_imports = HashMap::new();
    parse_default_value_full(
        s,
        var_type,
        available_consts,
        &empty_aliases,
        &empty_imports,
    )
}

pub(crate) fn parse_default_value_full(
    s: &str,
    var_type: &VarType,
    available_consts: &HashMap<String, Value>,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Option<Value> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Handle list defaults: [a, b, c]
    if s.starts_with(crate::consts::BRACKET_OPEN) && s.ends_with(crate::consts::BRACKET_CLOSE) {
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
            if let Some(v) = parse_default_value_full(
                e,
                elem_type,
                available_consts,
                type_aliases,
                resolved_imports,
            ) {
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
        return Some(parse_struct_default(
            inner,
            fields,
            available_consts,
            type_aliases,
            resolved_imports,
        ));
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
        return parse_default_value_full(
            s,
            inner,
            available_consts,
            type_aliases,
            resolved_imports,
        );
    }

    // If the expected type is an Enum, handle variant identifiers.
    if let VarType::Enum(variants) = var_type {
        return parse_enum_default_value(
            s,
            variants,
            available_consts,
            type_aliases,
            resolved_imports,
        );
    }

    if let Some(val) = resolve_const_default(s, available_consts) {
        return Some(val);
    }
    if let Some(val) = resolve_kinds_default(s, type_aliases, resolved_imports) {
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
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
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
                        if let Some(eq_pos) = find_char_at_depth_zero(e, crate::consts::EQUALS) {
                            let key = e[..eq_pos].trim();
                            let val_str = e[eq_pos + 1..].trim();
                            let field_type = v
                                .fields
                                .iter()
                                .find(|f| f.name == key)
                                .map_or(&VarType::Str, |f| &f.var_type);
                            if let Some(val) = parse_default_value_full(
                                val_str,
                                field_type,
                                available_consts,
                                type_aliases,
                                resolved_imports,
                            ) {
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
#[path = "params_tests.rs"]
mod tests;
