//! Import declaration parsing and resolution for frontmatter `imports:` blocks.
//!
//! Handles parsing `imports: [[stem](path.tmpl.md)]` entries and resolving
//! them by reading referenced template files to extract type information.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::{Import, ImportedNamespace, parse_frontmatter};
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
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(rest);

    let mut imports = Vec::new();

    // Handle block format with `- ` markers.
    let parts: Vec<&str> = if inner.contains(" - ") || inner.starts_with("- ") {
        inner.split(" - ").collect()
    } else {
        // Comma-separated at depth 0.
        split_at_depth_zero(inner)
    };

    for part in parts {
        let part = part.trim().strip_prefix('-').unwrap_or(part).trim();
        if part.is_empty() {
            continue;
        }

        // Parse markdown link format: `[stem](path.tmpl.md)`.
        let Some(bracket_open) = part.find('[') else {
            return Err(TemplateError::syntax(format!(
                "import entry '{part}' is not in [stem](path) format"
            )));
        };
        let Some(bracket_close) = part[bracket_open..].find(']') else {
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
        let Some(paren_close) = after_bracket[paren_open..].find(')') else {
            return Err(TemplateError::syntax(format!(
                "import entry '{part}' has unclosed parenthesis"
            )));
        };
        let paren_close = paren_open + paren_close;
        let path_str = after_bracket[paren_open + 1..paren_close].trim();

        // Validate: stem must match the file's template stem.
        let expected_stem = extract_template_stem(Path::new(path_str));
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
            path: PathBuf::from(path_str),
        });
    }

    Ok(imports)
}

/// Extract the template stem (filename without extensions) from a path.
///
/// For example, `review.tmpl.md` → `review`, `path/to/check.tmpl.md` → `check`.
#[must_use]
pub fn extract_template_stem(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    // Strip known extensions: `.tmpl.md`, `.tmpl`, `.md`.
    let stem = name
        .strip_suffix(".tmpl.md")
        .or_else(|| name.strip_suffix(".tmpl"))
        .or_else(|| name.strip_suffix(".md"))
        .unwrap_or(name);
    stem.to_string()
}

/// Resolve imports by reading referenced template files and extracting their type information.
///
/// Detects circular imports via the `visited` set.
///
/// # Errors
///
/// Returns [`TemplateError`] if an imported file cannot be read, parsed, or forms a cycle.
pub fn resolve_imports<S: std::hash::BuildHasher>(
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
