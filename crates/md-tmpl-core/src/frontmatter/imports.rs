//! Import declaration parsing and resolution for frontmatter `imports:` blocks.
//!
//! Handles parsing `imports: [[stem](path.tmpl.md)]` entries and resolving
//! them by reading referenced template files to extract type information.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
#[cfg(feature = "std")]
use std::path::{Path, PathBuf};

use super::Import;
#[cfg(feature = "std")]
use super::{ImportedNamespace, parse_frontmatter};
use crate::{compat::HashMap, error::TemplateError, value::Value};

/// Parse the value part after `imports:`.
///
/// Format: `[stem](path.tmpl.md)` entries separated by commas or block list.
pub(crate) fn parse_imports_value(rest: &str) -> Result<Vec<Import>, TemplateError> {
    use super::params::split_at_depth_zero;

    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(vec![]);
    }

    let inner = rest
        .strip_prefix(crate::consts::BRACKET_OPEN)
        .and_then(|s| s.strip_suffix(crate::consts::BRACKET_CLOSE))
        .unwrap_or(rest);

    let mut imports = Vec::new();

    // Handle block format with `- ` markers.
    let parts: Vec<&str> = if inner.contains(crate::consts::LIST_BLOCK_SEP)
        || inner.starts_with(crate::consts::LIST_ITEM_PREFIX)
    {
        inner.split(crate::consts::LIST_BLOCK_SEP).collect()
    } else {
        // Comma-separated at depth 0.
        split_at_depth_zero(inner)
    };

    for part in parts {
        let p = part
            .trim()
            .strip_prefix(crate::consts::TRIM_MARKER)
            .unwrap_or(part)
            .trim();
        let part = crate::consts::strip_string_literal(p).unwrap_or(p).trim();
        if part.is_empty() {
            continue;
        }

        // Parse markdown link format: `[stem](path.tmpl.md)`.
        let Some(bracket_open) = part.find(crate::consts::BRACKET_OPEN) else {
            return Err(TemplateError::syntax(format!(
                "import entry '{part}' is not in [stem](path) format"
            )));
        };
        let Some(bracket_close) = part[bracket_open..].find(crate::consts::BRACKET_CLOSE) else {
            return Err(TemplateError::syntax(format!(
                "import entry '{part}' has unclosed bracket"
            )));
        };
        let bracket_close = bracket_open + bracket_close;
        let stem = part[bracket_open + 1..bracket_close].trim().to_string();

        let after_bracket = &part[bracket_close + 1..];
        let Some(paren_open) = after_bracket.find(crate::consts::PAREN_OPEN) else {
            return Err(TemplateError::syntax(format!(
                "import entry '{part}' missing (path) after [stem]"
            )));
        };
        let Some(paren_close) = after_bracket[paren_open..].find(crate::consts::PAREN_CLOSE) else {
            return Err(TemplateError::syntax(format!(
                "import entry '{part}' has unclosed parenthesis"
            )));
        };
        let paren_close = paren_open + paren_close;
        let path_str = after_bracket[paren_open + 1..paren_close].trim();
        if !crate::consts::is_valid_include_path(path_str) {
            return Err(TemplateError::syntax(format!(
                "import path '{path_str}' must start with './', '../', or '/'"
            )));
        }

        // Validate: stem must match the file's template stem (unless path contains interpolation expressions, which will be validated after interpolation).
        if !path_str.contains(crate::consts::EXPR_START) {
            let expected_stem = extract_template_stem_str(path_str);
            if stem != expected_stem {
                return Err(TemplateError::syntax(format!(
                    "import stem '{stem}' does not match filename stem '{expected_stem}' (from '{path_str}')"
                )));
            }
        }

        // Validate: stem must not be a reserved keyword.
        if crate::consts::RESERVED_NAMES.contains(&stem.as_str()) {
            return Err(TemplateError::syntax(format!(
                "{}: import stem '{stem}'",
                crate::consts::ERR_RESERVED_KEYWORD
            )));
        }

        imports.push(Import {
            stem,
            #[cfg(feature = "std")]
            path: PathBuf::from(path_str),
            #[cfg(not(feature = "std"))]
            path: path_str.into(),
        });
    }

    Ok(imports)
}

/// Extract the template stem (filename without extensions) from a path string.
///
/// For example, `review.tmpl.md` → `review`, `path/to/check.tmpl.md` → `check`.
fn extract_template_stem_str(path_str: &str) -> alloc::string::String {
    // Extract filename (after last `/` or `\`).
    let name = path_str
        .rsplit([crate::consts::SLASH, crate::consts::BACKSLASH])
        .next()
        .unwrap_or(path_str);
    // Strip known extensions: `.tmpl.md`, `.tmpl`, `.md`.
    let stem = name
        .strip_suffix(crate::consts::EXT_TMPL_MD)
        .or_else(|| name.strip_suffix(crate::consts::EXT_TMPL))
        .or_else(|| name.strip_suffix(crate::consts::EXT_MD))
        .unwrap_or(name);
    stem.into()
}

/// Extract the template stem (filename without extensions) from a path.
///
/// For example, `review.tmpl.md` → `review`, `path/to/check.tmpl.md` → `check`.
#[cfg(feature = "std")]
#[must_use]
pub fn extract_template_stem(path: &Path) -> alloc::string::String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    // Strip known extensions: `.tmpl.md`, `.tmpl`, `.md`.
    let stem = name
        .strip_suffix(crate::consts::EXT_TMPL_MD)
        .or_else(|| name.strip_suffix(crate::consts::EXT_TMPL))
        .or_else(|| name.strip_suffix(crate::consts::EXT_MD))
        .unwrap_or(name);
    stem.into()
}

/// `no_std` version delegates to the string-based implementation.
#[cfg(not(feature = "std"))]
#[must_use]
pub fn extract_template_stem(path_str: &str) -> alloc::string::String {
    extract_template_stem_str(path_str)
}

/// Interpolate expressions like `{{ consts.DIR }}` in a path string using available constants.
pub(crate) fn interpolate_path_str(
    path: &str,
    available_consts: &HashMap<String, Value>,
) -> Result<alloc::string::String, TemplateError> {
    let mut result = alloc::string::String::new();
    let mut remaining = path;

    while let Some(start_idx) = remaining.find(crate::consts::EXPR_START) {
        result.push_str(&remaining[..start_idx]);
        let after_start = &remaining[start_idx + crate::consts::EXPR_START.len()..];

        let Some(end_idx) = after_start.find(crate::consts::EXPR_END) else {
            return Err(TemplateError::syntax(format!(
                "unclosed '{}' in import path '{path}'",
                crate::consts::EXPR_START
            )));
        };

        let expr = after_start[..end_idx].trim();
        if expr.is_empty() {
            return Err(TemplateError::syntax(format!(
                "empty expression '{}{}' in import path '{path}'",
                crate::consts::EXPR_START,
                crate::consts::EXPR_END
            )));
        }

        let mut val_opt = if let Some(lit) = crate::consts::strip_string_literal(expr) {
            Some(Value::Str(lit.into()))
        } else {
            available_consts.get(expr).cloned()
        };

        if val_opt.is_none() {
            // Strip common prefixes: consts., opts., options., params.
            let stripped = if let Some(s) = expr.strip_prefix(crate::consts::PREFIX_CONSTS_DOT) {
                s.trim()
            } else if let Some(s) = expr.strip_prefix(crate::consts::PREFIX_OPTS_DOT) {
                s.trim()
            } else if let Some(s) = expr.strip_prefix(crate::consts::PREFIX_OPTIONS_DOT) {
                s.trim()
            } else if let Some(s) = expr.strip_prefix(crate::consts::PREFIX_PARAMS_DOT) {
                s.trim()
            } else {
                expr
            };

            val_opt = available_consts.get(stripped).cloned();

            if val_opt.is_none() {
                let mut parts = stripped.split(crate::consts::PATH_SEP);
                let root_key = parts.next().unwrap_or("").trim();

                if let Some(mut val) = available_consts.get(root_key).cloned() {
                    let mut ok = true;
                    for part in parts {
                        let part = part.trim();
                        if let Some(next_val) = val.get_field(part).cloned() {
                            val = next_val;
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        val_opt = Some(val);
                    }
                }
            }
        }

        let Some(val) = val_opt else {
            return Err(TemplateError::syntax(format!(
                "unresolvable expression '{}{expr}{}' in import path '{path}'",
                crate::consts::EXPR_START,
                crate::consts::EXPR_END
            )));
        };

        result.push_str(&val.to_string());
        remaining = &after_start[end_idx + crate::consts::EXPR_END.len()..];
    }

    result.push_str(remaining);
    Ok(result)
}

/// In-place interpolate expressions in import paths and validate prefixes and stems.
pub(crate) fn interpolate_imports(
    imports: &mut [Import],
    available_consts: &HashMap<String, Value>,
) -> Result<(), TemplateError> {
    for import in imports.iter_mut() {
        #[cfg(feature = "std")]
        let path_str = import.path.to_string_lossy().to_string();
        #[cfg(not(feature = "std"))]
        let path_str = import.path.clone();

        if path_str.contains(crate::consts::EXPR_START) {
            let interpolated = interpolate_path_str(&path_str, available_consts)?;
            if !crate::consts::is_valid_resolved_path(&interpolated)
                || interpolated.contains(crate::consts::EXPR_START)
            {
                return Err(TemplateError::syntax(format!(
                    "import path '{interpolated}' must start with './', '../', or '/'"
                )));
            }
            let expected_stem = extract_template_stem_str(&interpolated);
            if import.stem != expected_stem {
                return Err(TemplateError::syntax(format!(
                    "import stem '{}' does not match filename stem '{expected_stem}' (from '{interpolated}')",
                    import.stem
                )));
            }
            #[cfg(feature = "std")]
            {
                import.path = PathBuf::from(interpolated);
            }
            #[cfg(not(feature = "std"))]
            {
                import.path = interpolated;
            }
        }
    }
    Ok(())
}

/// Resolve imports by reading referenced template files and extracting their type information.
///
/// Detects circular imports via the `visited` set.
///
/// # Errors
///
/// Returns [`TemplateError`] if an imported file cannot be read, parsed, or forms a cycle.
#[cfg(feature = "std")]
pub fn resolve_imports<S: core::hash::BuildHasher>(
    imports: &[Import],
    base_dir: &Path,
    visited: &mut std::collections::HashSet<PathBuf, S>,
) -> Result<HashMap<String, ImportedNamespace>, TemplateError> {
    let empty_consts = HashMap::new();
    let mut imports_clone = imports.to_vec();
    resolve_imports_with_consts(&mut imports_clone, base_dir, visited, &empty_consts)
}

/// Resolve imports with available constants for path interpolation.
///
/// Imports are processed **sequentially**: literal-path imports are resolved
/// first, and their exported constants are accumulated into the available set.
/// This allows later imports to use expressions like `{{ env.PROMPTS_DIR }}`
/// in their paths, where `env` was resolved by an earlier import.
///
/// # Errors
///
/// Returns [`TemplateError`] if an import path cannot be interpolated, read, parsed, or forms a cycle.
#[cfg(feature = "std")]
pub fn resolve_imports_with_consts<S: core::hash::BuildHasher>(
    imports: &mut [Import],
    base_dir: &Path,
    visited: &mut std::collections::HashSet<PathBuf, S>,
    available_consts: &HashMap<String, Value>,
) -> Result<HashMap<String, ImportedNamespace>, TemplateError> {
    let mut result = HashMap::new();
    // Start with the caller-provided consts (from the template's own `consts:` block).
    let mut accumulated_consts = available_consts.clone();

    for import in imports.iter_mut() {
        // Interpolate this import's path using all consts accumulated so far.
        {
            #[cfg(feature = "std")]
            let path_str = import.path.to_string_lossy().to_string();
            #[cfg(not(feature = "std"))]
            let path_str = import.path.clone();

            if path_str.contains(crate::consts::EXPR_START) {
                let interpolated = interpolate_path_str(&path_str, &accumulated_consts)?;
                if !crate::consts::is_valid_resolved_path(&interpolated)
                    || interpolated.contains(crate::consts::EXPR_START)
                {
                    return Err(TemplateError::syntax(format!(
                        "import path '{interpolated}' must start with './', '../', or '/'"
                    )));
                }
                let expected_stem = extract_template_stem_str(&interpolated);
                if import.stem != expected_stem {
                    return Err(TemplateError::syntax(format!(
                        "import stem '{}' does not match filename stem '{expected_stem}' (from '{interpolated}')",
                        import.stem
                    )));
                }
                #[cfg(feature = "std")]
                {
                    import.path = PathBuf::from(interpolated);
                }
                #[cfg(not(feature = "std"))]
                {
                    import.path = interpolated;
                }
            }
        }

        let resolved_path = if import.path.is_absolute() {
            import.path.clone()
        } else {
            base_dir.join(&import.path)
        };

        // Circular import detection.
        if !visited.insert(resolved_path.clone()) {
            return Err(TemplateError::syntax(format!(
                "{}: '{}'",
                crate::consts::ERR_CIRCULAR_IMPORT,
                resolved_path.display()
            )));
        }

        let source = std::fs::read_to_string(&resolved_path).map_err(|e| {
            TemplateError::syntax(format!(
                "cannot read imported template '{}': {}",
                resolved_path.display(),
                e
            ))
        })?;

        let (imported_fm, _) = parse_frontmatter(&source)?;

        let ns = ImportedNamespace {
            type_aliases: imported_fm.type_aliases,
            param_types: imported_fm
                .declarations
                .iter()
                .map(|d| (d.name.clone(), d.var_type.clone()))
                .collect(),
            consts: imported_fm
                .consts
                .iter()
                .filter_map(|d| d.default_value.clone().map(|v| (d.name.clone(), v)))
                .collect(),
            const_types: imported_fm
                .consts
                .iter()
                .map(|d| (d.name.clone(), d.var_type.clone()))
                .collect(),
        };

        // Accumulate this import's consts so subsequent imports can reference them.
        for (name, val) in &ns.consts {
            accumulated_consts.insert(format!("{}.{name}", import.stem), val.clone());
        }

        result.insert(import.stem.clone(), ns);
    }

    Ok(result)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use std::path::Path;

    use super::*;

    // -- parse_imports_value: valid inputs ------------------------------------

    #[cfg(feature = "std")]
    #[test]
    fn parse_single_import() {
        let imports = parse_imports_value("[review](./review.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "review");
        assert_eq!(imports[0].path, PathBuf::from("./review.tmpl.md"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_multiple_comma_separated_imports() {
        let imports = parse_imports_value("[a](./a.tmpl.md), [b](./b.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].stem, "a");
        assert_eq!(imports[0].path, PathBuf::from("./a.tmpl.md"));
        assert_eq!(imports[1].stem, "b");
        assert_eq!(imports[1].path, PathBuf::from("./b.tmpl.md"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_empty_input_returns_empty_vec() {
        let imports = parse_imports_value("").unwrap();
        assert!(imports.is_empty());
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_whitespace_only_returns_empty_vec() {
        let imports = parse_imports_value("   ").unwrap();
        assert!(imports.is_empty());
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_block_format_with_dash_markers() {
        let imports = parse_imports_value("- [review](./review.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "review");
        assert_eq!(imports[0].path, PathBuf::from("./review.tmpl.md"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_block_format_multiple_entries() {
        let imports =
            parse_imports_value("- [alpha](./alpha.tmpl.md) - [beta](./beta.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].stem, "alpha");
        assert_eq!(imports[1].stem, "beta");
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_import_with_subdirectory_path() {
        let imports = parse_imports_value("[check](./path/to/check.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "check");
        assert_eq!(imports[0].path, PathBuf::from("./path/to/check.tmpl.md"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_import_with_md_extension_only() {
        let imports = parse_imports_value("[simple](./simple.md)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "simple");
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_import_with_tmpl_extension_only() {
        let imports = parse_imports_value("[name](./name.tmpl)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "name");
    }

    #[cfg(feature = "std")]
    #[test]
    fn parse_import_wrapped_in_brackets() {
        let imports = parse_imports_value("[[review](./review.tmpl.md)]").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "review");
    }

    // -- parse_imports_value: error cases ------------------------------------

    #[cfg(feature = "std")]
    #[test]
    fn reject_bare_relative_filename() {
        let err = parse_imports_value("[review](review.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("must start with './', '../', or '/'"),
            "expected prefix error, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn reject_stem_mismatch() {
        let err = parse_imports_value("[wrong](./review.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("does not match"),
            "expected stem mismatch error, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn reject_missing_brackets() {
        let err = parse_imports_value("review(./review.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not in [stem](path) format"),
            "expected format error, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn reject_unclosed_bracket() {
        let err = parse_imports_value("[review(./review.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unclosed bracket"),
            "expected unclosed bracket error, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn reject_missing_parens_after_bracket() {
        let err = parse_imports_value("[review]").unwrap_err();
        let msg = err.to_string();
        // Outer brackets are stripped, so `[review]` → `review` which lacks `[`.
        assert!(
            msg.contains("not in [stem](path) format"),
            "expected format error, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn reject_missing_parens_after_stem_bracket() {
        // With extra text to avoid outer-bracket stripping.
        let err = parse_imports_value("[review] trailing").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("missing (path) after [stem]"),
            "expected missing parens error, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn reject_reserved_keyword_stem() {
        let err = parse_imports_value("[list](./list.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("reserved keyword"),
            "expected reserved keyword error, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn reject_reserved_keyword_struct() {
        let err = parse_imports_value("[struct](./struct.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("reserved keyword"),
            "expected reserved keyword error, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn reject_reserved_keyword_params() {
        let err = parse_imports_value("[params](./params.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("reserved keyword"),
            "expected reserved keyword error, got: {msg}"
        );
    }

    // -- extract_template_stem (std::path::Path) -----------------------------

    #[cfg(feature = "std")]
    #[test]
    fn extract_stem_tmpl_md() {
        assert_eq!(extract_template_stem(Path::new("review.tmpl.md")), "review");
    }

    #[cfg(feature = "std")]
    #[test]
    fn extract_stem_with_directory_prefix() {
        assert_eq!(
            extract_template_stem(Path::new("path/to/check.tmpl.md")),
            "check"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn extract_stem_md_only() {
        assert_eq!(extract_template_stem(Path::new("simple.md")), "simple");
    }

    #[cfg(feature = "std")]
    #[test]
    fn extract_stem_tmpl_only() {
        assert_eq!(extract_template_stem(Path::new("name.tmpl")), "name");
    }

    #[cfg(feature = "std")]
    #[test]
    fn extract_stem_no_extension() {
        assert_eq!(extract_template_stem(Path::new("noext")), "noext");
    }

    #[cfg(feature = "std")]
    #[test]
    fn extract_stem_deeply_nested_path() {
        assert_eq!(
            extract_template_stem(Path::new("a/b/c/deep.tmpl.md")),
            "deep"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn extract_stem_dot_in_name() {
        // `my.file.tmpl.md` → strips `.tmpl.md` → `my.file`
        assert_eq!(
            extract_template_stem(Path::new("my.file.tmpl.md")),
            "my.file"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_interpolate_path_str() {
        let mut consts = HashMap::new();
        consts.insert("DIR".to_string(), Value::Str("./prompts".to_string()));
        consts.insert("FILE".to_string(), Value::Str("layout".to_string()));

        let res =
            interpolate_path_str("{{ consts.DIR }}/{{ consts.FILE }}.tmpl.md", &consts).unwrap();
        assert_eq!(res, "./prompts/layout.tmpl.md");
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_interpolate_path_str_missing_error() {
        let consts = HashMap::new();
        let err = interpolate_path_str("{{ consts.MISSING }}/file.tmpl.md", &consts).unwrap_err();
        assert!(err.to_string().contains("unresolvable expression"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_interpolate_imports() {
        let mut consts = HashMap::new();
        consts.insert("DIR".to_string(), Value::Str("./shared".to_string()));

        let mut imports = vec![Import {
            stem: "header".to_string(),
            path: PathBuf::from("{{ consts.DIR }}/header.tmpl.md"),
        }];

        interpolate_imports(&mut imports, &consts).unwrap();
        assert_eq!(imports[0].path, PathBuf::from("./shared/header.tmpl.md"));
    }

    /// Cascading imports: import A (env) provides a const, then import B's
    /// path uses `{{ env.PROMPTS_DIR }}` to resolve its location.
    #[cfg(feature = "std")]
    #[test]
    fn cascading_imports_resolve_sequentially() {
        let dir = tempfile::tempdir().expect("tempdir");
        let base = dir.path();

        // Create env.tmpl.md with a PROMPTS_DIR const pointing to a subdirectory.
        let sub = base.join("sub");
        std::fs::create_dir_all(&sub).expect("mkdir");
        std::fs::write(
            base.join("env.tmpl.md"),
            format!(
                "---\nname: env\nconsts:\n  - PROMPTS_DIR = str := \"{}\"\n---\n",
                sub.display()
            ),
        )
        .expect("write env");

        // Create the target template in the subdirectory.
        std::fs::write(
            sub.join("layout.tmpl.md"),
            "---\nname: layout\nconsts:\n  - MAX = int := 42\n---\n",
        )
        .expect("write layout");

        // Simulate the import list: env first (literal path), then layout (expression path).
        let mut imports = vec![
            Import {
                stem: "env".to_string(),
                path: PathBuf::from("./env.tmpl.md"),
            },
            Import {
                stem: "layout".to_string(),
                path: PathBuf::from("{{ env.PROMPTS_DIR }}/layout.tmpl.md"),
            },
        ];

        let initial_consts = HashMap::new();
        let mut visited = std::collections::HashSet::new();
        let result =
            resolve_imports_with_consts(&mut imports, base, &mut visited, &initial_consts).unwrap();

        // Both imports should be resolved.
        assert!(result.contains_key("env"), "env import should be resolved");
        assert!(
            result.contains_key("layout"),
            "layout import should be resolved"
        );
        // layout's consts should be accessible.
        assert_eq!(
            result["layout"].consts.get("MAX"),
            Some(&Value::Int(42)),
            "layout's MAX const should be 42"
        );
    }
}
