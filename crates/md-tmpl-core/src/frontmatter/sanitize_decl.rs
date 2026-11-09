//! Declarative `| sanitize` and `trusted` / `untrusted` parsing and validation for frontmatter `params:` and `types:`.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use super::params::{find_char_at_depth_zero, split_at_depth_zero, strip_type_brackets};
use crate::{
    compat::HashMap,
    consts::{
        EQUALS, ERR_UNSAFE_TRUST_DECLARATION, FILTER_SANITIZE, FILTER_UNSAFE_TRUST, PAREN_CLOSE,
        PAREN_OPEN, PATH_SEP, QUOTE_DOUBLE, QUOTE_SINGLE, TYPE_BOOL, TYPE_ENUM, TYPE_FLOAT,
        TYPE_INT, TYPE_LIST, TYPE_OPTION, TYPE_STR, TYPE_STRUCT, TYPE_TMPL, TYPE_TRUSTED,
        TYPE_UNSAFE_TRUST, TYPE_UNTRUSTED,
    },
    error::TemplateError,
    filter::{parse_filter, parse_sanitize_filter_args},
    parser::{split_filters_aware, split_pipe_aware},
    types::{SanitizeSpec, VarDecl, VarType},
};

/// Default trust state while walking a type expression (`SafeByDefault` for `params:` and `types:`,
/// `Trusted` for `consts:`, `tmpl(...)` signatures, and inside `trusted ...` subtrees).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrustMode {
    /// `str` fields automatically receive [`SanitizeSpec::Inline`] unless marked `trusted`.
    SafeByDefault,
    /// `str` fields are trusted pass-through unless marked `untrusted` or `| sanitize`.
    Trusted,
}

/// Strip a single leading `trusted` or `untrusted` modifier (`kw T` or `kw(T)`), returning the
/// inner type expression. Does not strip `kw("tag")` string-literal arguments so invalid forms
/// like `untrusted("doc") str` or `trusted("doc") str` are rejected by the type parser.
pub(crate) fn strip_single_trust_keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    if rest.starts_with(' ') || rest.starts_with('\t') {
        let trimmed = rest.trim();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    } else if let Some(inner) = strip_type_brackets(rest.trim()) {
        let trimmed = inner.trim();
        if !trimmed.is_empty()
            && !trimmed.starts_with(QUOTE_DOUBLE)
            && !trimmed.starts_with(QUOTE_SINGLE)
        {
            return Some(trimmed);
        }
    }
    None
}

/// Parse a `| sanitize` or `| sanitize("tag", ...)` filter chain attached to a frontmatter
/// declaration into its [`SanitizeSpec`].
pub(crate) fn parse_decl_sanitize_chain(chain: &str) -> Result<SanitizeSpec, String> {
    let filters = split_filters_aware(chain);
    let mut result: Option<SanitizeSpec> = None;
    for raw_filter in filters {
        let trimmed = raw_filter.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (name, args) = parse_filter(trimmed);
        if name == FILTER_UNSAFE_TRUST {
            return Err(ERR_UNSAFE_TRUST_DECLARATION.to_string());
        }
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

/// Strip any `| sanitize(...)`, `trusted`, and `untrusted` annotations from a type expression
/// (including inside compound `struct(...)`, `list(...)`, `option(...)`, and `enum(...)`
/// definitions), returning the cleaned type expression and a map of relative dotted field paths
/// (`""` for the root type, `"field"` or `"field.sub"` for nested fields) to their [`SanitizeSpec`].
pub(crate) fn extract_type_sanitize_specs_with_aliases(
    raw_type: &str,
    initial_mode: TrustMode,
    type_alias_sanitize: &HashMap<String, HashMap<String, SanitizeSpec>>,
) -> Result<(String, HashMap<String, SanitizeSpec>), String> {
    let mut specs = HashMap::new();
    let cleaned = extract_type_sanitize_inner(
        raw_type.trim(),
        "",
        initial_mode,
        false,
        type_alias_sanitize,
        &mut specs,
    )?;
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

fn strip_trust_modifiers(
    mut base: &str,
    initial_mode: TrustMode,
    inherited_explicit_untrusted: bool,
) -> Result<(&str, TrustMode, bool), String> {
    let mut mode = initial_mode;
    let mut explicit_untrusted = inherited_explicit_untrusted;
    loop {
        if let Some(inner) = strip_single_trust_keyword(base, TYPE_UNTRUSTED) {
            mode = TrustMode::SafeByDefault;
            explicit_untrusted = true;
            base = inner;
        } else if let Some(inner) = strip_single_trust_keyword(base, TYPE_TRUSTED) {
            mode = TrustMode::Trusted;
            explicit_untrusted = false;
            base = inner;
        } else if strip_single_trust_keyword(base, TYPE_UNSAFE_TRUST).is_some() {
            return Err(ERR_UNSAFE_TRUST_DECLARATION.to_string());
        } else {
            break;
        }
    }
    Ok((base, mode, explicit_untrusted))
}

fn extract_type_sanitize_inner(
    raw: &str,
    prefix: &str,
    initial_mode: TrustMode,
    inherited_explicit_untrusted: bool,
    type_alias_sanitize: &HashMap<String, HashMap<String, SanitizeSpec>>,
    specs: &mut HashMap<String, SanitizeSpec>,
) -> Result<String, String> {
    let (base_part, filter_chain) = split_pipe_aware(raw);
    if !filter_chain.trim().is_empty() {
        insert_spec(specs, prefix, parse_decl_sanitize_chain(filter_chain)?)?;
    }
    let (base, mode, explicit_untrusted) =
        strip_trust_modifiers(base_part.trim(), initial_mode, inherited_explicit_untrusted)?;

    if let Some(rest) = base.strip_prefix(TYPE_OPTION) {
        let rest_trimmed = rest.trim();
        if let Some(inner) = strip_type_brackets(rest_trimmed) {
            let cleaned_inner = extract_type_sanitize_inner(
                inner.trim(),
                prefix,
                mode,
                explicit_untrusted,
                type_alias_sanitize,
                specs,
            )?;
            return Ok(format!(
                "{TYPE_OPTION}{PAREN_OPEN}{cleaned_inner}{PAREN_CLOSE}"
            ));
        }
    }

    for compound_kw in [TYPE_STRUCT, TYPE_LIST, TYPE_TMPL] {
        if let Some(rest) = base.strip_prefix(compound_kw) {
            let rest_trimmed = rest.trim();
            if let Some(inner) = strip_type_brackets(rest_trimmed) {
                let child_mode = if compound_kw == TYPE_TMPL {
                    TrustMode::Trusted
                } else {
                    mode
                };
                let cleaned_fields = extract_fields_sanitize(
                    inner,
                    prefix,
                    child_mode,
                    explicit_untrusted && compound_kw == TYPE_LIST,
                    type_alias_sanitize,
                    specs,
                )?;
                return Ok(format!(
                    "{compound_kw}{PAREN_OPEN}{cleaned_fields}{PAREN_CLOSE}"
                ));
            }
        }
    }

    if let Some(rest) = base.strip_prefix(TYPE_ENUM) {
        let rest_trimmed = rest.trim();
        if let Some(inner) = strip_type_brackets(rest_trimmed) {
            let cleaned_variants =
                extract_enum_variants_sanitize(inner, prefix, mode, type_alias_sanitize, specs)?;
            return Ok(format!(
                "{TYPE_ENUM}{PAREN_OPEN}{cleaned_variants}{PAREN_CLOSE}"
            ));
        }
    }

    if base == TYPE_STR {
        if mode == TrustMode::SafeByDefault {
            insert_spec(specs, prefix, SanitizeSpec::Inline)?;
        }
    } else if mode == TrustMode::SafeByDefault {
        let is_builtin_scalar = matches!(base, TYPE_BOOL | TYPE_INT | TYPE_FLOAT);
        if !is_builtin_scalar && let Some(alias_specs) = type_alias_sanitize.get(base) {
            for (rel, spec) in alias_specs {
                let key = join_prefix(prefix, rel);
                specs.entry(key).or_insert_with(|| spec.clone());
            }
        } else if explicit_untrusted {
            insert_spec(specs, prefix, SanitizeSpec::Inline)?;
        }
    }

    Ok(base.to_string())
}

fn extract_fields_sanitize(
    inner: &str,
    prefix: &str,
    mode: TrustMode,
    explicit_untrusted_elem: bool,
    type_alias_sanitize: &HashMap<String, HashMap<String, SanitizeSpec>>,
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
            let cleaned_field_type = extract_type_sanitize_inner(
                field_type_raw,
                &field_prefix,
                mode,
                false,
                type_alias_sanitize,
                specs,
            )?;
            cleaned_entries.push(format!("{field_name} = {cleaned_field_type}"));
        } else {
            let cleaned_elem = extract_type_sanitize_inner(
                f,
                prefix,
                mode,
                explicit_untrusted_elem,
                type_alias_sanitize,
                specs,
            )?;
            cleaned_entries.push(cleaned_elem);
        }
    }
    Ok(cleaned_entries.join(", "))
}

fn extract_enum_variants_sanitize(
    inner: &str,
    prefix: &str,
    mode: TrustMode,
    type_alias_sanitize: &HashMap<String, HashMap<String, SanitizeSpec>>,
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
            let cleaned_fields = extract_fields_sanitize(
                var_inner,
                prefix,
                mode,
                false,
                type_alias_sanitize,
                specs,
            )?;
            cleaned_variants.push(format!(
                "{var_name}{PAREN_OPEN}{cleaned_fields}{PAREN_CLOSE}"
            ));
        } else {
            cleaned_variants.push(v.to_string());
        }
    }
    Ok(cleaned_variants.join(", "))
}

/// Resolve a relative field path (`""` for root, `"a.b"` for nested field) against `var_type`
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

pub(crate) type ExtractedDeclarationSanitize =
    (String, Option<String>, HashMap<String, SanitizeSpec>);

/// Extract `| sanitize(...)`, `trusted`, and `untrusted` annotations from a single declaration's
/// `type_str` and optional `default_part` (supporting both `name = str | sanitize := "val"` and
/// `name = str := "val" | sanitize`), merging any inherited `types:` alias sanitize specs.
pub(crate) fn extract_declaration_sanitize(
    name: &str,
    type_str: &str,
    default_part: Option<&str>,
    is_constant: bool,
    type_alias_sanitize: &HashMap<String, HashMap<String, SanitizeSpec>>,
) -> Result<ExtractedDeclarationSanitize, TemplateError> {
    let initial_mode = if is_constant {
        TrustMode::Trusted
    } else {
        TrustMode::SafeByDefault
    };
    let empty_aliases = HashMap::new();
    let alias_map = if is_constant {
        &empty_aliases
    } else {
        type_alias_sanitize
    };
    let (cleaned_type, mut rel_specs) =
        extract_type_sanitize_specs_with_aliases(type_str, initial_mode, alias_map)
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

/// Parse a `sanitize_notice:` frontmatter value, stripping optional surrounding quotes and
/// decoding the standard string-literal escapes (`\\`, `\"`, `\'`; any other `\X` is kept
/// verbatim), exactly like every other md-tmpl string literal.
#[must_use]
pub(crate) fn parse_sanitize_notice_value(raw: &str) -> String {
    let trimmed = raw.trim();
    match crate::consts::strip_string_literal(trimmed) {
        Some(inner) => crate::consts::unescape_string_literal(inner),
        None => trimmed.to_string(),
    }
}
