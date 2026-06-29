use std::path::PathBuf;

use hashbrown::{HashMap, HashSet};

/// Extract the file stem from a template path, stripping `.tmpl.md` or
/// `.tmpl` suffixes.
pub(crate) fn stem_from_path(path: &str) -> String {
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    // Strip known double extensions first.
    filename
        .strip_suffix(".tmpl.md")
        .or_else(|| filename.strip_suffix(".tmpl"))
        .unwrap_or_else(|| {
            // Fallback: strip last extension.
            filename.rsplit_once('.').map_or(filename, |(stem, _)| stem)
        })
        .to_string()
}

pub(crate) fn hash_source(source: &str) -> u64 {
    prompt_templates::__private::fnv1a_hash(source.as_bytes())
}

/// Result of compiling a template at macro expansion time.
pub(crate) struct CompiledTemplateAst {
    pub(crate) frontmatter: prompt_templates::Frontmatter,
    pub(crate) segments: Vec<prompt_templates::compiled::Segment>,
    pub(crate) inline_templates:
        HashMap<String, prompt_templates::compiled::CompiledInlineTemplate>,
    pub(crate) source_hash: u64,
}

/// Read a template file relative to `CARGO_MANIFEST_DIR`, compile it,
/// and return both the resolved full path and the compiled AST.
pub(crate) fn load_and_compile(
    rel_path: &str,
) -> Result<(std::path::PathBuf, CompiledTemplateAst), String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let full_path = std::path::Path::new(&manifest_dir).join(rel_path);
    let source = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("failed to read template '{}': {e}", full_path.display()))?;
    let base_dir = full_path.parent().unwrap_or(std::path::Path::new("."));
    let ast = compile_template_to_ast(&source, base_dir)?;
    Ok((full_path, ast))
}

pub(crate) fn compile_template_to_ast(
    source: &str,
    base_dir: &std::path::Path,
) -> Result<CompiledTemplateAst, String> {
    let source_hash = hash_source(source);
    let (fm, body) = prompt_templates::parse_frontmatter_with_base_dir(source, base_dir)
        .map_err(|e| e.to_string())?;

    let (mut segments, inline_templates) =
        prompt_templates::compiled::compile(body, &fm.type_aliases).map_err(|e| e.to_string())?;

    // Static analysis: Enforce that all parameters referenced in the body are declared.
    let referenced = prompt_templates::compiled::collect_referenced_params(&segments);
    let mut declared: HashSet<String> = fm.params.iter().cloned().collect();
    for c in &fm.consts {
        declared.insert(c.name.clone());
    }
    for import in &fm.imports {
        declared.insert(import.stem.clone());
    }
    // Inline template names ({% tmpl NAME %}) are valid targets for
    // {% include NAME %} and should not be flagged as undeclared variables.
    for inline_name in inline_templates.keys() {
        declared.insert(inline_name.clone());
    }
    let undeclared: Vec<&String> = referenced
        .iter()
        .filter(|v| !declared.contains(v.as_str()))
        .collect();
    if !undeclared.is_empty() {
        let mut names: Vec<&str> = undeclared.iter().map(|s| s.as_str()).collect();
        names.sort_unstable();
        return Err(format!(
            "undeclared variable(s) referenced in body: {}",
            names.join(", ")
        ));
    }

    // Recursively resolve includes at compile time.
    // Collect declared tmpl() parameter names — these are dynamic includes
    // resolved at runtime, not compile-time file lookups.
    let tmpl_params: HashSet<String> = fm
        .declarations
        .iter()
        .filter(|d| matches!(d.var_type, prompt_templates::VarType::Tmpl(_)))
        .map(|d| d.name.clone())
        .collect();
    let mut visited_paths = HashSet::new();
    resolve_includes_recursive(
        &mut segments,
        base_dir,
        &mut visited_paths,
        &inline_templates,
        &tmpl_params,
        0,
    )?;

    // Flow-sensitive type check: validate variant names and field access.
    // Import stems and const names are opaque to field-level type checking —
    // their structure is resolved at runtime, not at compile time.
    // Block scope ensures `opaque_roots` (which borrows `&str` from `fm`)
    // is dropped before `fm` is moved into the return struct.
    {
        let mut opaque_roots: HashSet<&str> = HashSet::new();
        for import in &fm.imports {
            opaque_roots.insert(&import.stem);
        }
        for c in &fm.consts {
            opaque_roots.insert(&c.name);
        }
        let type_errors = prompt_templates::compiled::validate_field_accesses_with_opaque(
            &segments,
            &fm.declarations,
            &opaque_roots,
        );
        if !type_errors.is_empty() {
            return Err(type_errors.join("\n"));
        }
    }

    Ok(CompiledTemplateAst {
        frontmatter: fm,
        segments,
        inline_templates,
        source_hash,
    })
}

/// Maximum compile-time include depth. Prevents pathological non-circular
/// chains from causing excessive compilation time. Override with the
/// `PROMPT_TEMPLATES_MAX_INCLUDE_DEPTH` environment variable.
const DEFAULT_MAX_COMPILE_INCLUDE_DEPTH: usize = 64;

pub(crate) fn max_compile_include_depth() -> usize {
    std::env::var("PROMPT_TEMPLATES_MAX_INCLUDE_DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_COMPILE_INCLUDE_DEPTH)
}

pub(crate) fn resolve_includes_recursive(
    segments: &mut [prompt_templates::compiled::Segment],
    base_dir: &std::path::Path,
    visited_paths: &mut HashSet<PathBuf>,
    inline_templates: &HashMap<String, prompt_templates::compiled::CompiledInlineTemplate>,
    tmpl_params: &HashSet<String>,
    depth: usize,
) -> Result<(), String> {
    let max_depth = max_compile_include_depth();
    if depth > max_depth {
        return Err(format!(
            "compile-time include depth ({depth}) exceeds maximum ({max_depth}). \
             Set PROMPT_TEMPLATES_MAX_INCLUDE_DEPTH to increase the limit"
        ));
    }

    for seg in segments {
        match seg {
            prompt_templates::compiled::Segment::Include(inc) => {
                // Dynamic tmpl() parameter — resolved at runtime, not compile time.
                if tmpl_params.contains(inc.path.as_ref()) {
                    continue;
                }

                // Check inline templates (scoped to THIS file).
                if let Some(compiled) = inline_templates.get(inc.path.as_ref()) {
                    inc.inline_compiled = Some(compiled.clone());
                    continue;
                }

                let include_path = base_dir.join(inc.path.as_ref());
                let canonical = include_path
                    .canonicalize()
                    .unwrap_or_else(|_| include_path.clone());

                if !visited_paths.insert(canonical.clone()) {
                    // Cycle detected — load declarations for boundary checking
                    // but don't recurse into the body.
                    load_include_declarations(inc, &include_path)?;
                    continue;
                }

                resolve_single_include(inc, base_dir, visited_paths, depth + 1)?;
                visited_paths.remove(&canonical);
            }
            prompt_templates::compiled::Segment::ForLoop { body, .. } => {
                resolve_includes_recursive(
                    body,
                    base_dir,
                    visited_paths,
                    inline_templates,
                    tmpl_params,
                    depth,
                )?;
            }
            prompt_templates::compiled::Segment::If {
                branches,
                else_body,
            } => {
                for (_, branch_body) in branches {
                    resolve_includes_recursive(
                        branch_body,
                        base_dir,
                        visited_paths,
                        inline_templates,
                        tmpl_params,
                        depth,
                    )?;
                }
                resolve_includes_recursive(
                    else_body,
                    base_dir,
                    visited_paths,
                    inline_templates,
                    tmpl_params,
                    depth,
                )?;
            }
            prompt_templates::compiled::Segment::Match { arms, .. } => {
                for (_, arm_body) in arms {
                    resolve_includes_recursive(
                        arm_body,
                        base_dir,
                        visited_paths,
                        inline_templates,
                        tmpl_params,
                        depth,
                    )?;
                }
            }
            prompt_templates::compiled::Segment::Static(_)
            | prompt_templates::compiled::Segment::Expr { .. }
            | prompt_templates::compiled::Segment::Raw(_)
            | prompt_templates::compiled::Segment::Comment(_) => {}
        }
    }
    Ok(())
}

/// Load and compile an included template file into its `inline_compiled` field.
///
/// Used for the cycle case: we need the declarations for boundary type checking
/// but don't recurse into the body's own includes.
pub(crate) fn load_include_declarations(
    inc: &mut prompt_templates::compiled::CompiledInclude,
    include_path: &std::path::Path,
) -> Result<(), String> {
    if inc.inline_compiled.is_some() {
        return Ok(());
    }
    let included_source = std::fs::read_to_string(include_path)
        .map_err(|e| format!("cannot read include {}: {e}", include_path.display()))?;
    let included_base_dir = include_path.parent().unwrap_or(std::path::Path::new("."));
    let (included_fm, included_body) =
        prompt_templates::parse_frontmatter_with_base_dir(&included_source, included_base_dir)
            .map_err(|e| format!("syntax error in include {}: {e}", include_path.display()))?;
    let (included_segments, _) =
        prompt_templates::compiled::compile(included_body, &included_fm.type_aliases).map_err(
            |e| {
                format!(
                    "compilation error in include {}: {e}",
                    include_path.display()
                )
            },
        )?;
    // Build const values map from included file's own consts.
    let mut included_consts = hashbrown::HashMap::new();
    for d in &included_fm.consts {
        if let Some(ref v) = d.default_value {
            included_consts.insert(d.name.clone(), v.clone());
        }
    }
    inc.inline_compiled = Some(prompt_templates::compiled::CompiledInlineTemplate {
        segments: std::sync::Arc::from(included_segments),
        declarations: std::sync::Arc::from(included_fm.declarations),
        consts: std::sync::Arc::new(included_consts),
        imported_consts: std::sync::Arc::new(included_fm.imported_consts),
    });
    Ok(())
}

/// Process a single include directive: load, compile, and recurse into
/// the included template's own includes.
///
/// Contract and type checking is now handled by `validate_field_accesses`
/// after all includes are resolved.
pub(crate) fn resolve_single_include(
    inc: &mut prompt_templates::compiled::CompiledInclude,
    base_dir: &std::path::Path,
    visited_paths: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    let include_path = base_dir.join(inc.path.as_ref());
    let included_source = std::fs::read_to_string(&include_path)
        .map_err(|e| format!("cannot read include {}: {e}", include_path.display()))?;

    let included_base_dir = include_path.parent().unwrap_or(base_dir);
    let (included_fm, included_body) =
        prompt_templates::parse_frontmatter_with_base_dir(&included_source, included_base_dir)
            .map_err(|e| format!("syntax error in include {}: {e}", include_path.display()))?;

    // Compile the included file and extract ITS OWN inline templates.
    // Each file has its own {% tmpl %} namespace — parent templates do NOT
    // leak into includes, and included file templates don't leak to parents.
    let (mut included_segments, included_inline_templates) =
        prompt_templates::compiled::compile(included_body, &included_fm.type_aliases).map_err(
            |e| {
                format!(
                    "compilation error in include {}: {e}",
                    include_path.display()
                )
            },
        )?;

    let child_base_dir = include_path.parent().unwrap_or(base_dir);
    // Use the INCLUDED FILE'S own inline templates, not the parent's.
    // Block scope ensures `child_tmpl_params` is dropped before
    // `included_fm.declarations` is moved into an Arc.
    {
        let child_tmpl_params: HashSet<String> = included_fm
            .declarations
            .iter()
            .filter(|d| matches!(d.var_type, prompt_templates::VarType::Tmpl(_)))
            .map(|d| d.name.clone())
            .collect();
        resolve_includes_recursive(
            &mut included_segments,
            child_base_dir,
            visited_paths,
            &included_inline_templates,
            &child_tmpl_params,
            depth,
        )?;
    }

    // Build const values map from included file's own consts.
    let mut included_consts = hashbrown::HashMap::new();
    for d in &included_fm.consts {
        if let Some(ref v) = d.default_value {
            included_consts.insert(d.name.clone(), v.clone());
        }
    }
    inc.inline_compiled = Some(prompt_templates::compiled::CompiledInlineTemplate {
        segments: std::sync::Arc::from(included_segments),
        declarations: std::sync::Arc::from(included_fm.declarations),
        consts: std::sync::Arc::new(included_consts),
        imported_consts: std::sync::Arc::new(included_fm.imported_consts),
    });
    Ok(())
}
