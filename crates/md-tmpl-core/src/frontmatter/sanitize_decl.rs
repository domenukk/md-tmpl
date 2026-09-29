//! Declarative `| sanitize` parsing and validation for frontmatter `params:` and `types:`.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use super::params::{find_char_at_depth_zero, split_at_depth_zero, strip_type_brackets};
use crate::{
    compat::HashMap,
    consts::{
        EQUALS, FILTER_SANITIZE, PAREN_CLOSE, PAREN_OPEN, PATH_SEP, TYPE_ENUM, TYPE_LIST,
        TYPE_OPTION, TYPE_STRUCT, TYPE_TMPL,
    },
    error::TemplateError,
    filter::{parse_filter, parse_sanitize_filter_args},
    parser::{split_filters_aware, split_pipe_aware},
    types::{SanitizeSpec, VarDecl, VarType},
};

/// Parse a `| sanitize` or `| sanitize("tag", ...)` filter chain attached to a frontmatter
/// declaration into a [`SanitizeSpec`].
pub(crate) fn parse_decl_sanitize_chain(chain: &str) -> Result<SanitizeSpec, String> {
    let filters = split_filters_aware(chain);
    let mut result: Option<SanitizeSpec> = None;
    for raw_filter in filters {
        let trimmed = raw_filter.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (name, args) = parse_filter(trimmed);
        if name != FILTER_SANITIZE {
            return Err(format!(
                "unsupported parameter filter '{name}' in frontmatter (only '| sanitize' is allowed on declarations)"
            ));
        }
        if result.is_some() {
            return Err("duplicate '| sanitize' on declaration".to_string());
        }
        let parsed = parse_sanitize_filter_args(args).map_err(|e| e.to_string())?;
        result = Some(match parsed {
            None => SanitizeSpec::Inline,
            Some((tag, notice)) => SanitizeSpec::Block { tag, notice },
        });
    }
    result.ok_or_else(|| "empty filter after '|' in declaration".to_string())
}

/// Strip any `| sanitize(...)` annotations from a type expression (including inside compound
/// `struct(...)`, `list(...)`, `option(...)`, and `enum(...)` definitions), returning the
/// cleaned type expression and a map of relative dotted field paths (`""` for the root type,
/// `"field"` or `"field.sub"` for nested fields) to their [`SanitizeSpec`].
pub(crate) fn extract_type_sanitize_specs(
    raw_type: &str,
) -> Result<(String, HashMap<String, SanitizeSpec>), String> {
    let mut specs = HashMap::new();
    let cleaned = extract_type_sanitize_inner(raw_type.trim(), "", &mut specs)?;
    Ok((cleaned, specs))
}

fn insert_spec(
    specs: &mut HashMap<String, SanitizeSpec>,
    prefix: &str,
    spec: SanitizeSpec,
) -> Result<(), String> {
    if let Some(existing) = specs.get(prefix) {
        if *existing == spec {
            return Ok(());
        }
        if matches!(existing, SanitizeSpec::Inline) && matches!(spec, SanitizeSpec::Block { .. }) {
            specs.insert(prefix.to_string(), spec);
            return Ok(());
        }
        if matches!(existing, SanitizeSpec::Block { .. }) && matches!(spec, SanitizeSpec::Inline) {
            return Ok(());
        }
        return Err(format!(
            "conflicting sanitization specifications on '{}'",
            if prefix.is_empty() { "type" } else { prefix }
        ));
    }
    specs.insert(prefix.to_string(), spec);
    Ok(())
}

fn join_prefix(prefix: &str, field: &str) -> String {
    if prefix.is_empty() {
        field.to_string()
    } else if field.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}{PATH_SEP}{field}")
    }
}

fn extract_type_sanitize_inner(
    raw: &str,
    prefix: &str,
    specs: &mut HashMap<String, SanitizeSpec>,
) -> Result<String, String> {
    let (base_part, filter_chain) = split_pipe_aware(raw);
    let mut base = base_part.trim();
    if !filter_chain.trim().is_empty() {
        let spec = parse_decl_sanitize_chain(filter_chain)?;
        insert_spec(specs, prefix, spec)?;
    }

    if let Some(rest) = base.strip_prefix(crate::consts::TYPE_UNTRUSTED) {
        if rest.starts_with(' ') || rest.starts_with('\t') {
            insert_spec(specs, prefix, SanitizeSpec::Inline)?;
            base = rest.trim();
        } else if let Some(inner) = strip_type_brackets(rest.trim()) {
            insert_spec(specs, prefix, SanitizeSpec::Inline)?;
            base = inner.trim();
        }
    }

    if let Some(rest) = base.strip_prefix(TYPE_OPTION) {
        let rest_trimmed = rest.trim();
        if let Some(inner) = strip_type_brackets(rest_trimmed) {
            let cleaned_inner = extract_type_sanitize_inner(inner.trim(), prefix, specs)?;
            return Ok(format!(
                "{TYPE_OPTION}{PAREN_OPEN}{cleaned_inner}{PAREN_CLOSE}"
            ));
        }
    }

    for compound_kw in [TYPE_STRUCT, TYPE_LIST, TYPE_TMPL] {
        if let Some(rest) = base.strip_prefix(compound_kw) {
            let rest_trimmed = rest.trim();
            if let Some(inner) = strip_type_brackets(rest_trimmed) {
                let cleaned_fields = extract_fields_sanitize(inner, prefix, specs)?;
                return Ok(format!(
                    "{compound_kw}{PAREN_OPEN}{cleaned_fields}{PAREN_CLOSE}"
                ));
            }
        }
    }

    if let Some(rest) = base.strip_prefix(TYPE_ENUM) {
        let rest_trimmed = rest.trim();
        if let Some(inner) = strip_type_brackets(rest_trimmed) {
            let cleaned_variants = extract_enum_variants_sanitize(inner, prefix, specs)?;
            return Ok(format!(
                "{TYPE_ENUM}{PAREN_OPEN}{cleaned_variants}{PAREN_CLOSE}"
            ));
        }
    }

    Ok(base.to_string())
}

fn extract_fields_sanitize(
    inner: &str,
    prefix: &str,
    specs: &mut HashMap<String, SanitizeSpec>,
) -> Result<String, String> {
    let entries = split_at_depth_zero(inner);
    let mut cleaned_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        let f = entry.trim();
        if f.is_empty() {
            continue;
        }
        if let Some(eq_pos) = find_char_at_depth_zero(f, EQUALS) {
            let field_name = f[..eq_pos].trim();
            let field_type_raw = f[eq_pos + 1..].trim();
            let field_prefix = join_prefix(prefix, field_name);
            let cleaned_field_type =
                extract_type_sanitize_inner(field_type_raw, &field_prefix, specs)?;
            cleaned_entries.push(format!("{field_name} = {cleaned_field_type}"));
        } else {
            let cleaned_elem = extract_type_sanitize_inner(f, prefix, specs)?;
            cleaned_entries.push(cleaned_elem);
        }
    }
    Ok(cleaned_entries.join(", "))
}

fn extract_enum_variants_sanitize(
    inner: &str,
    prefix: &str,
    specs: &mut HashMap<String, SanitizeSpec>,
) -> Result<String, String> {
    let entries = split_at_depth_zero(inner);
    let mut cleaned_variants = Vec::with_capacity(entries.len());
    for entry in entries {
        let v = entry.trim();
        if v.is_empty() {
            continue;
        }
        if let Some(open_pos) = v.find(PAREN_OPEN)
            && v.ends_with(PAREN_CLOSE)
        {
            let var_name = v[..open_pos].trim();
            let var_inner = &v[open_pos + 1..v.len() - 1];
            let cleaned_fields = extract_fields_sanitize(var_inner, prefix, specs)?;
            cleaned_variants.push(format!(
                "{var_name}{PAREN_OPEN}{cleaned_fields}{PAREN_CLOSE}"
            ));
        } else {
            cleaned_variants.push(v.to_string());
        }
    }
    Ok(cleaned_variants.join(", "))
}

/// Resolve a relative field path (Wait: `""` for root, `"a.b"` for nested field) against `var_type`
/// and verify that the target type supports `| sanitize` (`str`, `option(str)`, or scalar `list(str)`).
pub(crate) fn validate_sanitize_specs_on_type(
    decl_name: &str,
    var_type: &VarType,
    specs: &HashMap<String, SanitizeSpec>,
) -> Result<(), TemplateError> {
    for rel_path in specs.keys() {
        let full_path = join_prefix(decl_name, rel_path);
        let Some(target_type) = resolve_relative_type(var_type, rel_path) else {
            return Err(TemplateError::syntax(format!(
                "declaration '{decl_name}': cannot resolve field '{full_path}' for '| sanitize'"
            )));
        };
        if !target_type.allows_sanitize() {
            return Err(TemplateError::syntax(format!(
                "declaration '{decl_name}': '| sanitize' is only supported on 'str', 'option(str)', or 'list(str)', got '{target_type}' on '{full_path}'"
            )));
        }
    }
    Ok(())
}

fn resolve_relative_type<'a>(var_type: &'a VarType, rel_path: &str) -> Option<&'a VarType> {
    if rel_path.is_empty() {
        return Some(var_type);
    }
    let (head, tail) = match rel_path.split_once(PATH_SEP) {
        Some((h, t)) => (h, t),
        None => (rel_path, ""),
    };
    match var_type {
        VarType::Option(inner) => resolve_relative_type(inner, rel_path),
        VarType::Struct(fields) | VarType::List(fields) | VarType::Tmpl(fields) => {
            if fields.len() == 1 && fields[0].name.is_empty() {
                return resolve_relative_type(&fields[0].var_type, rel_path);
            }
            let field = fields.iter().find(|f| f.name == head)?;
            resolve_relative_type(&field.var_type, tail)
        }
        VarType::Enum(variants) => {
            for v in variants {
                if let Some(field) = v.fields.iter().find(|f| f.name == head) {
                    return resolve_relative_type(&field.var_type, tail);
                }
            }
            None
        }
        VarType::Str | VarType::Bool | VarType::Int | VarType::Float => None,
    }
}

/// Propagate any sanitize specs inherited from referenced `types:` aliases (`type_alias_sanitize`)
/// by walking the raw type expression for type alias references.
pub(crate) fn collect_inherited_alias_sanitize(
    raw_cleaned_type: &str,
    prefix: &str,
    type_alias_sanitize: &HashMap<String, HashMap<String, SanitizeSpec>>,
    out: &mut HashMap<String, SanitizeSpec>,
) {
    if type_alias_sanitize.is_empty() {
        return;
    }
    let t = raw_cleaned_type.trim();
    if let Some(alias_specs) = type_alias_sanitize.get(t) {
        for (rel, spec) in alias_specs {
            let key = join_prefix(prefix, rel);
            out.entry(key).or_insert_with(|| spec.clone());
        }
        return;
    }
    if let Some(rest) = t.strip_prefix(TYPE_OPTION)
        && let Some(inner) = strip_type_brackets(rest.trim())
    {
        collect_inherited_alias_sanitize(inner, prefix, type_alias_sanitize, out);
        return;
    }
    for compound_kw in [TYPE_STRUCT, TYPE_LIST, TYPE_TMPL] {
        if let Some(rest) = t.strip_prefix(compound_kw)
            && let Some(inner) = strip_type_brackets(rest.trim())
        {
            for entry in split_at_depth_zero(inner) {
                let f = entry.trim();
                if f.is_empty() {
                    continue;
                }
                if let Some(eq_pos) = find_char_at_depth_zero(f, EQUALS) {
                    let field_name = f[..eq_pos].trim();
                    let field_type = f[eq_pos + 1..].trim();
                    let field_prefix = join_prefix(prefix, field_name);
                    collect_inherited_alias_sanitize(
                        field_type,
                        &field_prefix,
                        type_alias_sanitize,
                        out,
                    );
                } else {
                    collect_inherited_alias_sanitize(f, prefix, type_alias_sanitize, out);
                }
            }
            return;
        }
    }
}

pub(crate) type ExtractedDeclarationSanitize =
    (String, Option<String>, HashMap<String, SanitizeSpec>);

/// Extract `| sanitize(...)` annotations from a single declaration's `type_str` and optional
/// `default_part` (supporting both `name = str | sanitize := "val"` and `name = str := "val" | sanitize`),
/// merging any inherited `types:` alias sanitize specs.
pub(crate) fn extract_declaration_sanitize(
    name: &str,
    type_str: &str,
    default_part: Option<&str>,
    is_constant: bool,
    type_alias_sanitize: &HashMap<String, HashMap<String, SanitizeSpec>>,
) -> Result<ExtractedDeclarationSanitize, TemplateError> {
    let (cleaned_type, mut rel_specs) = extract_type_sanitize_specs(type_str)
        .map_err(|e| TemplateError::syntax(format!("declaration '{name}': {e}")))?;

    let cleaned_default = if let Some(dp) = default_part {
        let (base_dp, dp_filter) = split_pipe_aware(dp);
        if dp_filter.trim().is_empty() {
            Some(dp.to_string())
        } else {
            let spec = parse_decl_sanitize_chain(dp_filter)
                .map_err(|e| TemplateError::syntax(format!("declaration '{name}': {e}")))?;
            insert_spec(&mut rel_specs, "", spec)
                .map_err(|e| TemplateError::syntax(format!("declaration '{name}': {e}")))?;
            Some(base_dp.trim().to_string())
        }
    } else {
        None
    };

    if is_constant && !rel_specs.is_empty() {
        return Err(TemplateError::syntax(format!(
            "constant '{name}': '| sanitize' is only supported on 'params' and 'types', not 'consts'"
        )));
    }

    if !is_constant {
        collect_inherited_alias_sanitize(&cleaned_type, "", type_alias_sanitize, &mut rel_specs);
    }

    Ok((cleaned_type, cleaned_default, rel_specs))
}

/// Merge a single declaration's relative sanitize specs into the template's `param_sanitize` map.
pub(crate) fn register_decl_sanitize_specs(
    decl: &VarDecl,
    rel_specs: HashMap<String, SanitizeSpec>,
    param_sanitize: &mut HashMap<String, SanitizeSpec>,
) -> Result<(), TemplateError> {
    validate_sanitize_specs_on_type(&decl.name, &decl.var_type, &rel_specs)?;
    for (rel, spec) in rel_specs {
        let full_key = join_prefix(&decl.name, &rel);
        param_sanitize.insert(full_key, spec);
    }
    Ok(())
}

/// Parse a `sanitize_notice:` frontmatter value, stripping optional surrounding quotes.
#[must_use]
pub(crate) fn parse_sanitize_notice_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with(crate::consts::QUOTE_DOUBLE)
            && trimmed.ends_with(crate::consts::QUOTE_DOUBLE))
            || (trimmed.starts_with(crate::consts::QUOTE_SINGLE)
                && trimmed.ends_with(crate::consts::QUOTE_SINGLE)))
    {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == crate::consts::BACKSLASH
                && let Some(next_ch) = chars.next()
            {
                match next_ch {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    other => out.push(other),
                }
            } else {
                out.push(ch);
            }
        }
        out
    } else {
        trimmed.to_string()
    }
}
