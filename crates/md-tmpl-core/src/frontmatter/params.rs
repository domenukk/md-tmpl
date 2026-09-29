//! Parameter declaration parsing for frontmatter `params:` blocks.
//!
//! Handles both inline (`[name = str, count = int]`) and block
//! (`- name = str`) formats, including default values and nested types.

use alloc::{
    boxed::Box,
    string::{String, ToString},
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
        // For block list items, strip a YAML-consistent inline `#` comment from
        // the item scalar before joining (see `strip_list_item_comment`). Other
        // lines keep their original form so existing layout behavior is intact.
        let cleaned: Option<String> = match trimmed.strip_prefix(crate::consts::LIST_ITEM_PREFIX) {
            Some(scalar) => {
                let kept = strip_list_item_comment(scalar.trim_start());
                if kept.is_empty() {
                    // `- # comment` → empty list item; skip like a comment line.
                    continue;
                }
                Some(alloc::format!("{}{kept}", crate::consts::LIST_ITEM_PREFIX))
            }
            None => None,
        };
        if raw.starts_with(' ') || raw.starts_with('\t') {
            // Continuation of previous logical line.
            if let Some(prev) = logical.last_mut() {
                prev.push(' ');
                prev.push_str(cleaned.as_deref().unwrap_or(trimmed));
            } else {
                logical.push(cleaned.unwrap_or_else(|| raw.to_string()));
            }
        } else {
            logical.push(cleaned.unwrap_or_else(|| raw.to_string()));
        }
    }
    logical
}

/// Strip a YAML-consistent inline `#` comment from a block list-item scalar.
///
/// `scalar` is the text following the `- ` block-sequence marker. Matches real
/// YAML plain-scalar comment semantics: a `#` that begins the scalar or is
/// preceded by whitespace starts a comment running to end of line.
///
/// A scalar wholly wrapped in a YAML quote (`"..."` / `'...'`) protects any `#`
/// inside the quotes — only a `#` appearing after the closing quote is treated
/// as a comment. This is intentionally NOT md-tmpl-string-aware: the `"` inside
/// an unquoted (plain) scalar such as `x = str := "a # b"` are ordinary
/// characters, so ` #` still starts a comment, mirroring real YAML.
///
/// The returned slice has trailing whitespace trimmed when a comment was
/// removed.
pub(crate) fn strip_list_item_comment(scalar: &str) -> &str {
    match scalar.chars().next() {
        Some(crate::consts::QUOTE_DOUBLE) => {
            match closing_double_quote_end(scalar) {
                Some(end) => match find_yaml_comment(&scalar[end..], false) {
                    Some(pos) => scalar[..end + pos].trim_end(),
                    None => scalar,
                },
                // Unterminated quote — leave untouched; downstream reports it.
                None => scalar,
            }
        }
        Some(crate::consts::QUOTE_SINGLE) => match closing_single_quote_end(scalar) {
            Some(end) => match find_yaml_comment(&scalar[end..], false) {
                Some(pos) => scalar[..end + pos].trim_end(),
                None => scalar,
            },
            None => scalar,
        },
        _ => match find_yaml_comment(scalar, true) {
            Some(pos) => scalar[..pos].trim_end(),
            None => scalar,
        },
    }
}

/// Find the byte index of a `#` that begins a YAML comment.
///
/// A `#` starts a comment when preceded by ASCII whitespace, or — when
/// `start_is_comment` is `true` — when it is the first character of the string.
fn find_yaml_comment(s: &str, start_is_comment: bool) -> Option<usize> {
    let mut prev: Option<char> = None;
    for (i, c) in s.char_indices() {
        if c == crate::consts::FM_COMMENT_PREFIX {
            let is_comment = match prev {
                None => start_is_comment,
                Some(p) => p == ' ' || p == '\t',
            };
            if is_comment {
                return Some(i);
            }
        }
        prev = Some(c);
    }
    None
}

/// Return the byte index just past the closing `"` of a YAML double-quoted
/// scalar that starts at index 0, honoring `\`-escapes. Returns `None` if the
/// quote is never closed.
fn closing_double_quote_end(s: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in s.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if c == crate::consts::BACKSLASH {
            escaped = true;
        } else if c == crate::consts::QUOTE_DOUBLE {
            return Some(i + c.len_utf8());
        }
    }
    None
}

/// Return the byte index just past the closing `'` of a YAML single-quoted
/// scalar that starts at index 0. In YAML single-quoted scalars, `''` is an
/// escaped literal quote. Returns `None` if the quote is never closed.
fn closing_single_quote_end(s: &str) -> Option<usize> {
    let mut it = s.char_indices().skip(1).peekable();
    while let Some((i, c)) = it.next() {
        if c == crate::consts::QUOTE_SINGLE {
            if it.peek().map(|&(_, c2)| c2) == Some(crate::consts::QUOTE_SINGLE) {
                it.next(); // consume the second quote of an escaped `''`
                continue;
            }
            return Some(i + c.len_utf8());
        }
    }
    None
}

/// Map of param name → `(import_stem, imported_type_name)` for params whose
/// top-level type is a dotted import reference resolving to an **enum**.
///
/// Enables codegen backends (currently the Rust proc-macro) to reference the
/// imported, already-generated type directly instead of emitting a duplicate
/// per-template copy.
pub(crate) type ImportedTypeRefs = HashMap<String, ImportedTypeRef>;

/// A single imported-enum reference: `(import_stem, type_name)`.
///
/// E.g. a param typed `role = artist.WorkRole` yields `("artist", "WorkRole")`.
pub(crate) type ImportedTypeRef = (String, String);

type ParsedDeclaration = (
    VarDecl,
    Option<ImportedTypeRef>,
    HashMap<String, crate::types::SanitizeSpec>,
);

/// Parse the value part after `params:` or `consts:`.
pub(crate) fn parse_declarations(
    rest: &str,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
    is_constant: bool,
    available_consts: &HashMap<String, Value>,
) -> Result<(Vec<VarDecl>, ImportedTypeRefs), TemplateError> {
    let empty_alias_sanitize = HashMap::new();
    let (decls, import_refs, _) = parse_declarations_with_sanitize(
        rest,
        type_aliases,
        &empty_alias_sanitize,
        resolved_imports,
        is_constant,
        available_consts,
    )?;
    Ok((decls, import_refs))
}

pub(crate) type ParsedDeclarationsWithSanitize = (
    Vec<VarDecl>,
    ImportedTypeRefs,
    HashMap<String, crate::types::SanitizeSpec>,
);

/// Parse `params:` or `consts:` along with declarative `| sanitize(...)` policies.
pub(crate) fn parse_declarations_with_sanitize(
    rest: &str,
    type_aliases: &HashMap<String, VarType>,
    type_alias_sanitize: &super::type_aliases::TypeAliasSanitizeMap,
    resolved_imports: &HashMap<String, ImportedNamespace>,
    is_constant: bool,
    available_consts: &HashMap<String, Value>,
) -> Result<ParsedDeclarationsWithSanitize, TemplateError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((vec![], HashMap::new(), HashMap::new()));
    }

    let inner = rest
        .strip_prefix(crate::consts::BRACKET_OPEN)
        .and_then(|s| s.strip_suffix(crate::consts::BRACKET_CLOSE))
        .unwrap_or(rest);

    let entries = if inner.contains("- ") {
        let mut result = Vec::new();
        for part in inner.split(" - ") {
            let part = part.trim().strip_prefix('-').unwrap_or(part).trim();
            if !part.is_empty() {
                result.push(part.to_string());
            }
        }
        result
    } else {
        split_at_depth_zero(inner)
            .into_iter()
            .map(ToString::to_string)
            .collect()
    };

    let mut decls = Vec::new();
    let mut import_refs = ImportedTypeRefs::new();
    let mut param_sanitize = HashMap::new();
    let mut seen_names = crate::compat::HashSet::new();
    let mut current_consts = available_consts.clone();
    for entry in &entries {
        let e = entry.trim();
        let unescaped =
            crate::consts::strip_string_literal(e).map(crate::consts::unescape_string_literal);
        let trimmed = unescaped.as_deref().map_or(e, str::trim);
        if let Some((decl, import_ref, rel_specs)) = parse_single_declaration(
            trimmed,
            type_aliases,
            type_alias_sanitize,
            resolved_imports,
            is_constant,
            &mut current_consts,
            &mut seen_names,
        )? {
            if !rel_specs.is_empty() {
                super::sanitize_decl::register_decl_sanitize_specs(
                    &decl,
                    rel_specs,
                    &mut param_sanitize,
                )?;
            }
            if let Some(r) = import_ref {
                import_refs.insert(decl.name.clone(), r);
            }
            decls.push(decl);
        }
    }

    Ok((decls, import_refs, param_sanitize))
}

/// If `type_str` is a bare dotted import reference (`stem.TypeName`) resolving
/// to an **enum** in `resolved_imports`, return `(stem, type_name)`.
///
/// Only whole-annotation plain enum references qualify — not nested positions,
/// options, lists, or structs. Mirrors the dotted-path resolution in
/// [`parse_type_annotation`].
fn imported_enum_type_ref(
    type_str: &str,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Option<ImportedTypeRef> {
    let s = crate::consts::strip_string_literal(type_str.trim())
        .unwrap_or(type_str.trim())
        .trim();
    let dot = s.find(crate::consts::PATH_SEP)?;
    let stem = &s[..dot];
    let type_name = &s[dot + crate::consts::PATH_SEP.len_utf8()..];
    let ns = resolved_imports.get(stem)?;
    let var_type = ns
        .type_aliases
        .get(type_name)
        .or_else(|| ns.param_types.get(type_name))?;
    matches!(var_type, VarType::Enum(_)).then(|| (stem.to_string(), type_name.to_string()))
}

fn resolve_declaration_default(
    name: &str,
    cleaned_default_part: Option<&str>,
    var_type: &VarType,
    is_constant: bool,
    current_consts: &mut HashMap<String, Value>,
    type_aliases: &HashMap<String, VarType>,
    resolved_imports: &HashMap<String, ImportedNamespace>,
) -> Result<Option<Value>, TemplateError> {
    let default_value = if let Some(dp) = cleaned_default_part {
        let default = parse_default_value_full(
            dp,
            var_type,
            current_consts,
            type_aliases,
            resolved_imports,
        )
        .or_else(|| resolve_const_default(dp, current_consts))
        .or_else(|| resolve_kinds_default(dp, type_aliases, resolved_imports))
        .ok_or_else(|| {
            if let Some(msg) = qualified_variant_default_error(dp, var_type) {
                return TemplateError::syntax(format!("declaration '{name}': {msg}"));
            }
            TemplateError::syntax(format!(
                "invalid default value '{dp}' for declaration '{name}' (strings must be quoted)"
            ))
        })?;
        current_consts.insert(name.to_string(), default.clone());
        Some(default)
    } else {
        None
    };

    if is_constant && default_value.is_none() {
        return Err(TemplateError::syntax(format!(
            "constant '{name}' is missing a value (expected 'name = type := value')"
        )));
    }

    if let Some(ref default) = default_value
        && !var_type.matches(default)
    {
        let label = if is_constant { "constant" } else { "param" };
        return Err(TemplateError::syntax(format!(
            "{label} '{name}': value has type '{}' but declared type is '{var_type}'",
            default.type_name()
        )));
    }

    Ok(default_value)
}

fn parse_single_declaration(
    trimmed: &str,
    type_aliases: &HashMap<String, VarType>,
    type_alias_sanitize: &super::type_aliases::TypeAliasSanitizeMap,
    resolved_imports: &HashMap<String, ImportedNamespace>,
    is_constant: bool,
    current_consts: &mut HashMap<String, Value>,
    seen_names: &mut crate::compat::HashSet<String>,
) -> Result<Option<ParsedDeclaration>, TemplateError> {
    if trimmed.is_empty() {
        return Ok(None);
    }

    let Some(eq_pos) = find_char_at_depth_zero(trimmed, crate::consts::EQUALS) else {
        let label = if is_constant { "constant" } else { "param" };
        return Err(TemplateError::syntax(format!(
            "{label} '{trimmed}' is missing a type annotation (expected 'name = type')"
        )));
    };

    let name = trimmed[..eq_pos].trim().to_string();
    let type_and_default = trimmed[eq_pos + 1..].trim();

    if eq_pos > 0 && trimmed.as_bytes()[eq_pos - 1] == crate::consts::COLON_BYTE {
        let label = if is_constant { "constant" } else { "param" };
        let bare_name = trimmed[..eq_pos - 1].trim();
        return Err(TemplateError::syntax(format!(
            "{label} '{bare_name}' must have an explicit type (expected 'name = type := value')"
        )));
    }

    if !seen_names.insert(name.clone()) {
        let err = if is_constant {
            crate::consts::ERR_DUPLICATE_CONST
        } else {
            crate::consts::ERR_DUPLICATE_PARAM
        };
        return Err(TemplateError::syntax(format!("{err}: '{name}'")));
    }

    if crate::consts::RESERVED_NAMES.contains(&name.as_str()) {
        return Err(TemplateError::syntax(format!(
            "{}: '{name}'",
            crate::consts::ERR_RESERVED_KEYWORD
        )));
    }

    let (raw_type_str, raw_default_part) =
        if let Some(assign_pos) = find_assign_default_at_depth_zero(type_and_default) {
            (
                type_and_default[..assign_pos].trim(),
                Some(type_and_default[assign_pos + 2..].trim()),
            )
        } else {
            (type_and_default, None)
        };

    let (cleaned_type_str, cleaned_default_part, rel_specs) =
        super::sanitize_decl::extract_declaration_sanitize(
            &name,
            raw_type_str,
            raw_default_part,
            is_constant,
            type_alias_sanitize,
        )?;
    let type_str = cleaned_type_str.as_str();

    let var_type = parse_type_annotation(type_str, type_aliases, resolved_imports)
        .map_err(|e| TemplateError::syntax(format!("declaration '{name}': {e}")))?;

    let default_value = resolve_declaration_default(
        &name,
        cleaned_default_part.as_deref(),
        &var_type,
        is_constant,
        current_consts,
        type_aliases,
        resolved_imports,
    )?;

    let import_ref = if is_constant {
        None
    } else {
        imported_enum_type_ref(type_str, resolved_imports)
    };

    Ok(Some((
        VarDecl {
            name,
            var_type,
            default_value,
        },
        import_ref,
        rel_specs,
    )))
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
    // When inside a quote, tracks whether the previous char was an unescaped
    // backslash (which escapes the current char, e.g. `\"` does not close).
    let mut escaped = false;
    for (i, ch) in input.char_indices() {
        if let Some(q) = in_quote {
            if escaped {
                escaped = false;
            } else if ch == crate::consts::BACKSLASH {
                escaped = true;
            } else if ch == q {
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

    if let Some(rest) = s.strip_prefix(crate::consts::TYPE_UNTRUSTED) {
        if rest.starts_with(' ') || rest.starts_with('\t') {
            return parse_type_annotation(rest.trim_start(), type_aliases, resolved_imports);
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
        // Reject reserved names (incl. codegen collision guards like __self).
        if !name.is_empty() && crate::consts::RESERVED_NAMES.contains(&name.as_str()) {
            return Err(format!("{}: '{name}'", crate::consts::ERR_RESERVED_KEYWORD));
        }
        decls.push(VarDecl {
            name,
            var_type,
            default_value: None,
        });
    }
    Ok(decls)
}

#[path = "param_defaults.rs"]
mod param_defaults;
pub(crate) use param_defaults::*;

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;
