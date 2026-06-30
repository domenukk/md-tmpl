//! Import declaration parsing and resolution for frontmatter `imports:` blocks.
//!
//! Handles parsing `imports: [[stem](path.tmpl.md)]` entries and resolving
//! them by reading referenced template files to extract type information.

use alloc::{string::ToString, vec::Vec};
#[cfg(feature = "std")]
use std::path::{Path, PathBuf};

use super::Import;
#[cfg(feature = "std")]
use super::{ImportedNamespace, parse_frontmatter};
#[cfg(feature = "std")]
use crate::compat::HashMap;
use crate::error::TemplateError;

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

        // Validate: stem must match the file's template stem.
        let expected_stem = extract_template_stem_str(path_str);
        if stem != expected_stem {
            return Err(TemplateError::syntax(format!(
                "import stem '{stem}' does not match filename stem '{expected_stem}' (from '{path_str}')"
            )));
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
    let mut result = HashMap::new();

    for import in imports {
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
        };

        result.insert(import.stem.clone(), ns);
    }

    Ok(result)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use std::path::Path;

    use super::*;

    // -- parse_imports_value: valid inputs ------------------------------------

    #[test]
    fn parse_single_import() {
        let imports = parse_imports_value("[review](./review.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "review");
        assert_eq!(imports[0].path, PathBuf::from("./review.tmpl.md"));
    }

    #[test]
    fn parse_multiple_comma_separated_imports() {
        let imports = parse_imports_value("[a](./a.tmpl.md), [b](./b.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].stem, "a");
        assert_eq!(imports[0].path, PathBuf::from("./a.tmpl.md"));
        assert_eq!(imports[1].stem, "b");
        assert_eq!(imports[1].path, PathBuf::from("./b.tmpl.md"));
    }

    #[test]
    fn parse_empty_input_returns_empty_vec() {
        let imports = parse_imports_value("").unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn parse_whitespace_only_returns_empty_vec() {
        let imports = parse_imports_value("   ").unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn parse_block_format_with_dash_markers() {
        let imports = parse_imports_value("- [review](./review.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "review");
        assert_eq!(imports[0].path, PathBuf::from("./review.tmpl.md"));
    }

    #[test]
    fn parse_block_format_multiple_entries() {
        let imports =
            parse_imports_value("- [alpha](./alpha.tmpl.md) - [beta](./beta.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].stem, "alpha");
        assert_eq!(imports[1].stem, "beta");
    }

    #[test]
    fn parse_import_with_subdirectory_path() {
        let imports = parse_imports_value("[check](./path/to/check.tmpl.md)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "check");
        assert_eq!(imports[0].path, PathBuf::from("./path/to/check.tmpl.md"));
    }

    #[test]
    fn parse_import_with_md_extension_only() {
        let imports = parse_imports_value("[simple](./simple.md)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "simple");
    }

    #[test]
    fn parse_import_with_tmpl_extension_only() {
        let imports = parse_imports_value("[name](./name.tmpl)").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "name");
    }

    #[test]
    fn parse_import_wrapped_in_brackets() {
        let imports = parse_imports_value("[[review](./review.tmpl.md)]").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].stem, "review");
    }

    // -- parse_imports_value: error cases ------------------------------------

    #[test]
    fn reject_bare_relative_filename() {
        let err = parse_imports_value("[review](review.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("must start with './', '../', or '/'"),
            "expected prefix error, got: {msg}"
        );
    }

    #[test]
    fn reject_stem_mismatch() {
        let err = parse_imports_value("[wrong](./review.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("does not match"),
            "expected stem mismatch error, got: {msg}"
        );
    }

    #[test]
    fn reject_missing_brackets() {
        let err = parse_imports_value("review(./review.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not in [stem](path) format"),
            "expected format error, got: {msg}"
        );
    }

    #[test]
    fn reject_unclosed_bracket() {
        let err = parse_imports_value("[review(./review.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unclosed bracket"),
            "expected unclosed bracket error, got: {msg}"
        );
    }

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

    #[test]
    fn reject_reserved_keyword_stem() {
        let err = parse_imports_value("[list](./list.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("reserved keyword"),
            "expected reserved keyword error, got: {msg}"
        );
    }

    #[test]
    fn reject_reserved_keyword_struct() {
        let err = parse_imports_value("[struct](./struct.tmpl.md)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("reserved keyword"),
            "expected reserved keyword error, got: {msg}"
        );
    }

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

    #[test]
    fn extract_stem_tmpl_md() {
        assert_eq!(extract_template_stem(Path::new("review.tmpl.md")), "review");
    }

    #[test]
    fn extract_stem_with_directory_prefix() {
        assert_eq!(
            extract_template_stem(Path::new("path/to/check.tmpl.md")),
            "check"
        );
    }

    #[test]
    fn extract_stem_md_only() {
        assert_eq!(extract_template_stem(Path::new("simple.md")), "simple");
    }

    #[test]
    fn extract_stem_tmpl_only() {
        assert_eq!(extract_template_stem(Path::new("name.tmpl")), "name");
    }

    #[test]
    fn extract_stem_no_extension() {
        assert_eq!(extract_template_stem(Path::new("noext")), "noext");
    }

    #[test]
    fn extract_stem_deeply_nested_path() {
        assert_eq!(
            extract_template_stem(Path::new("a/b/c/deep.tmpl.md")),
            "deep"
        );
    }

    #[test]
    fn extract_stem_dot_in_name() {
        // `my.file.tmpl.md` → strips `.tmpl.md` → `my.file`
        assert_eq!(
            extract_template_stem(Path::new("my.file.tmpl.md")),
            "my.file"
        );
    }
}
