//! Default-value parsing and resolution for frontmatter declarations.

use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use super::{super::ImportedNamespace, find_char_at_depth_zero, split_at_depth_zero};
use crate::{
    compat::HashMap,
    types::{VarDecl, VarType},
    value::Value,
};

/// Parse the *inner* content of a `{key = value, ...}` struct default into
/// a [`Value::Struct`].
///
/// Uses `=` as the key-value separator (not `:`) and curly braces for
/// delimiters.
pub(super) fn parse_struct_default(
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
pub(super) fn resolve_const_default(
    name: &str,
    available_consts: &HashMap<String, Value>,
) -> Option<Value> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    available_consts.get(name).cloned()
}

/// Resolve a function expression default like `kinds(EnumType)` into a [`Value::List`].
pub(super) fn resolve_kinds_default(
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
        return Some(Value::Str(crate::consts::unescape_string_literal(inner)));
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

/// Return the enum variants for `var_type`, transparently unwrapping
/// `option(T)` so `option(Stage)` is treated like `Stage`. Returns `None` for
/// non-enum types.
fn enum_variants_of(var_type: &VarType) -> Option<&[crate::types::VariantDecl]> {
    match var_type {
        VarType::Enum(variants) => Some(variants),
        VarType::Option(inner) => enum_variants_of(inner),
        _ => None,
    }
}

/// If `default` is a qualified `Type.Variant` reference for an enum-typed (or
/// `option(enum)`) declaration whose suffix names a real variant, return a
/// helpful error message. Qualified references are only valid in expression
/// position; defaults must use the bare variant name.
///
/// Returns `None` when the type is not an enum or the default is not a
/// qualified reference to one of its variants, so const/other fallbacks keep
/// their generic error.
pub(super) fn qualified_variant_default_error(default: &str, var_type: &VarType) -> Option<String> {
    let variants = enum_variants_of(var_type)?;
    let (_, suffix) = default.rsplit_once(crate::consts::PATH_SEP)?;
    let suffix = suffix.trim();
    if variants.iter().any(|v| v.name == suffix) {
        Some(alloc::format!(
            "invalid enum default '{default}': use the bare variant name '{suffix}' \
             (a qualified 'Type.Variant' is only valid in expressions)"
        ))
    } else {
        None
    }
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
    if let Some(open_pos) = s.find(crate::consts::PAREN_OPEN)
        && s.ends_with(crate::consts::PAREN_CLOSE)
    {
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
