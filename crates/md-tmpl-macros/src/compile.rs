use std::path::PathBuf;

use hashbrown::{HashMap, HashSet};

/// Extract the file stem from a template path, stripping `.tmpl.md` or
/// `.tmpl` suffixes.
pub(crate) fn stem_from_path(path: &str) -> String {
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    filename
        .strip_suffix(".tmpl.md")
        .or_else(|| filename.strip_suffix(".tmpl"))
        .unwrap_or_else(|| filename.rsplit_once('.').map_or(filename, |(stem, _)| stem))
        .to_string()
}

pub(crate) fn hash_source(source: &str) -> u64 {
    md_tmpl_core::__private::fnv1a_hash(source.as_bytes())
}

/// Result of compiling a template at macro expansion time.
pub(crate) struct CompiledTemplateAst {
    pub(crate) frontmatter: md_tmpl_core::Frontmatter,
    pub(crate) segments: Vec<md_tmpl_core::compiled::Segment>,
    pub(crate) inline_templates: HashMap<String, md_tmpl_core::compiled::CompiledInlineTemplate>,
    pub(crate) source_hash: u64,
    /// Absolute paths of every file read while compiling this template
    /// (imported and `{% include %}`d templates, transitively).
    pub(crate) dependency_paths: Vec<PathBuf>,
}

pub(crate) fn cargo_manifest_dir() -> Result<PathBuf, String> {
    std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map_err(|e| format!("CARGO_MANIFEST_DIR is not set or invalid UTF-8: {e}"))
}

/// Read a template file relative to `CARGO_MANIFEST_DIR`, compile it,
/// and return both the resolved full path and the compiled AST.
pub(crate) fn load_and_compile(
    rel_path: &str,
    env_values: &[(&str, md_tmpl_core::Value)],
) -> Result<(std::path::PathBuf, CompiledTemplateAst), String> {
    let manifest_dir = cargo_manifest_dir()?;
    let full_path = manifest_dir.join(rel_path);
    let source = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("failed to read template '{}': {e}", full_path.display()))?;
    let base_dir = full_path.parent().unwrap_or(std::path::Path::new("."));
    let ast = compile_template_to_ast(&source, base_dir, env_values)?;
    Ok((full_path, ast))
}

pub(crate) fn compile_template_to_ast(
    raw_source: &str,
    base_dir: &std::path::Path,
    env_values: &[(&str, md_tmpl_core::Value)],
) -> Result<CompiledTemplateAst, String> {
    let normalized = md_tmpl_core::consts::normalize_line_endings(raw_source);
    let source = normalized.as_ref();
    let source_hash = hash_source(source);
    let (mut fm, body) =
        md_tmpl_core::parse_frontmatter_with_base_dir(source, base_dir, env_values)
            .map_err(|e| e.to_string())?;

    let mut dependency_paths: Vec<PathBuf> = Vec::new();
    collect_import_deps(&fm, base_dir, &mut dependency_paths);

    let (mut segments, inline_templates) = md_tmpl_core::compiled::compile_source_body_with_notice(
        source,
        body,
        &fm.type_aliases,
        &fm.type_alias_sanitize,
        fm.sanitize_notice.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    md_tmpl_core::compiled::apply_frontmatter_sanitization(
        &mut segments,
        &fm.param_sanitize,
        fm.sanitize_notice.as_deref(),
    );
    md_tmpl_core::compiled::check_untrusted_include_paths(&segments).map_err(|e| e.to_string())?;

    // Run the full shared static analysis pass (undeclared vars, unused params,
    // name collisions, bare enum access, static enum `in` checks, `__kind__`,
    // match label validation, and flow-sensitive field type validation).
    md_tmpl_core::__private::run_static_analysis(&segments, &fm, &inline_templates, false)
        .map_err(|e| e.to_string())?;

    // Recursively resolve file & inline includes at compile time.
    resolve_compile_time_includes(
        &mut segments,
        base_dir,
        &fm,
        &inline_templates,
        env_values,
        &mut dependency_paths,
    )?;
    md_tmpl_core::compiled::apply_enclosing_xml_tags(&mut segments);
    md_tmpl_core::compiled::check_untrusted_include_paths(&segments).map_err(|e| e.to_string())?;

    // Re-run flow-sensitive type checking after file includes have been bound
    // so cross-file `with` overrides and `for` bindings are validated at compile time.
    validate_types(&fm, &segments)?;

    md_tmpl_core::__private::inject_enum_type_constants(&fm.type_aliases, &mut fm.imported_consts);

    dependency_paths.sort();
    dependency_paths.dedup();

    Ok(CompiledTemplateAst {
        frontmatter: fm,
        segments,
        inline_templates,
        source_hash,
        dependency_paths,
    })
}

fn collect_import_deps(
    fm: &md_tmpl_core::Frontmatter,
    base_dir: &std::path::Path,
    deps: &mut Vec<PathBuf>,
) {
    for import in &fm.imports {
        let resolved = if import.path.is_absolute() {
            import.path.clone()
        } else {
            base_dir.join(&import.path)
        };
        deps.push(resolved);
    }
}

fn resolve_compile_time_includes(
    segments: &mut [md_tmpl_core::compiled::Segment],
    base_dir: &std::path::Path,
    fm: &md_tmpl_core::Frontmatter,
    inline_templates: &HashMap<String, md_tmpl_core::compiled::CompiledInlineTemplate>,
    env_values: &[(&str, md_tmpl_core::Value)],
    deps: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let tmpl_params: HashSet<String> = fm
        .declarations
        .iter()
        .filter(|d| matches!(d.var_type, md_tmpl_core::VarType::Tmpl(_)))
        .map(|d| d.name.clone())
        .collect();
    let mut visited_paths = HashSet::new();
    let ctx = IncludeResolveCtx {
        inline_templates,
        tmpl_params: &tmpl_params,
        env_values,
    };
    resolve_includes_recursive(segments, base_dir, &mut visited_paths, &ctx, deps, 0)
}

struct IncludeResolveCtx<'a> {
    inline_templates: &'a HashMap<String, md_tmpl_core::compiled::CompiledInlineTemplate>,
    tmpl_params: &'a HashSet<String>,
    env_values: &'a [(&'a str, md_tmpl_core::Value)],
}

fn validate_types(
    fm: &md_tmpl_core::Frontmatter,
    segments: &[md_tmpl_core::compiled::Segment],
) -> Result<(), String> {
    let type_errors = fm.validate_field_types(segments);
    if !type_errors.is_empty() {
        return Err(type_errors.join("\n"));
    }
    Ok(())
}

const DEFAULT_MAX_COMPILE_INCLUDE_DEPTH: usize = 64;

pub(crate) fn max_compile_include_depth() -> Result<usize, String> {
    match std::env::var("MD_TMPL_MAX_INCLUDE_DEPTH") {
        Ok(v) => v.parse::<usize>().map_err(|e| {
            format!("invalid MD_TMPL_MAX_INCLUDE_DEPTH '{v}': expected non-negative integer ({e})")
        }),
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_MAX_COMPILE_INCLUDE_DEPTH),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("MD_TMPL_MAX_INCLUDE_DEPTH contains invalid UTF-8".to_string())
        }
    }
}

fn resolve_includes_recursive(
    segments: &mut [md_tmpl_core::compiled::Segment],
    base_dir: &std::path::Path,
    visited_paths: &mut HashSet<PathBuf>,
    ctx: &IncludeResolveCtx<'_>,
    deps: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    let max_depth = max_compile_include_depth()?;
    if depth > max_depth {
        return Err(format!(
            "compile-time include depth ({depth}) exceeds maximum ({max_depth}). \
             Set MD_TMPL_MAX_INCLUDE_DEPTH to increase the limit"
        ));
    }

    for seg in segments {
        match seg {
            md_tmpl_core::compiled::Segment::Include(inc) => {
                if ctx.tmpl_params.contains(inc.path.as_ref()) {
                    continue;
                }

                if let Some(compiled) = ctx.inline_templates.get(inc.path.as_ref()) {
                    if inc.inline_compiled.is_none() {
                        let mut compiled = compiled.clone();
                        if let Some(sctx) = inc.sanitize_ctx.as_deref() {
                            let declared = std::sync::Arc::clone(&compiled.declared_sanitize);
                            md_tmpl_core::compiled::apply_include_context(
                                std::sync::Arc::make_mut(&mut compiled.segments),
                                sctx,
                                &declared,
                            );
                        }
                        inc.inline_compiled = Some(compiled);
                    }
                    continue;
                }

                let include_path = base_dir.join(inc.path.as_ref());
                let canonical = include_path
                    .canonicalize()
                    .map_err(|e| format!("cannot read include {}: {e}", include_path.display()))?;

                if !visited_paths.insert(canonical.clone()) {
                    load_include_declarations(inc, &include_path, ctx.env_values, deps)?;
                    continue;
                }

                resolve_single_include(
                    inc,
                    base_dir,
                    visited_paths,
                    ctx.env_values,
                    deps,
                    depth + 1,
                )?;
                visited_paths.remove(&canonical);
            }
            md_tmpl_core::compiled::Segment::ForLoop {
                body, else_body, ..
            } => {
                for loop_body in [body, else_body] {
                    resolve_includes_recursive(
                        loop_body,
                        base_dir,
                        visited_paths,
                        ctx,
                        deps,
                        depth,
                    )?;
                }
            }
            md_tmpl_core::compiled::Segment::If {
                branches,
                else_body,
            } => {
                for (_, branch_body) in branches {
                    resolve_includes_recursive(
                        branch_body,
                        base_dir,
                        visited_paths,
                        ctx,
                        deps,
                        depth,
                    )?;
                }
                resolve_includes_recursive(else_body, base_dir, visited_paths, ctx, deps, depth)?;
            }
            md_tmpl_core::compiled::Segment::Match { arms, .. } => {
                for arm in arms {
                    resolve_includes_recursive(
                        &mut arm.body,
                        base_dir,
                        visited_paths,
                        ctx,
                        deps,
                        depth,
                    )?;
                }
            }
            md_tmpl_core::compiled::Segment::Panic(inner) => {
                resolve_includes_recursive(inner, base_dir, visited_paths, ctx, deps, depth)?;
            }
            md_tmpl_core::compiled::Segment::Static(_)
            | md_tmpl_core::compiled::Segment::Expr { .. }
            | md_tmpl_core::compiled::Segment::Raw(_)
            | md_tmpl_core::compiled::Segment::Comment(_) => {}
        }
    }
    Ok(())
}

pub(crate) fn load_include_declarations(
    inc: &mut md_tmpl_core::compiled::CompiledInclude,
    include_path: &std::path::Path,
    env_values: &[(&str, md_tmpl_core::Value)],
    deps: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if inc.inline_compiled.is_some() {
        return Ok(());
    }
    let raw_source = std::fs::read_to_string(include_path)
        .map_err(|e| format!("cannot read include {}: {e}", include_path.display()))?;
    let normalized = md_tmpl_core::consts::normalize_line_endings(&raw_source);
    let included_source = normalized.as_ref();
    deps.push(include_path.to_path_buf());
    let included_base_dir = include_path.parent().unwrap_or(std::path::Path::new("."));
    let (included_fm, included_body) = md_tmpl_core::parse_frontmatter_with_base_dir(
        included_source,
        included_base_dir,
        env_values,
    )
    .map_err(|e| format!("syntax error in include {}: {e}", include_path.display()))?;
    collect_import_deps(&included_fm, included_base_dir, deps);
    let (mut included_segments, _) = md_tmpl_core::compiled::compile_source_body_with_notice(
        included_source,
        included_body,
        &included_fm.type_aliases,
        &included_fm.type_alias_sanitize,
        included_fm.sanitize_notice.as_deref(),
    )
    .map_err(|e| {
        format!(
            "compilation error in include {}: {e}",
            include_path.display()
        )
    })?;
    let declared_sanitize = compile_as_include_target(&mut included_segments, &included_fm);
    if let Some(ctx) = inc.sanitize_ctx.as_deref() {
        md_tmpl_core::compiled::apply_include_context(
            &mut included_segments,
            ctx,
            &declared_sanitize,
        );
    }
    md_tmpl_core::compiled::check_untrusted_include_paths(&included_segments)
        .map_err(|e| format!("in include {}: {e}", include_path.display()))?;
    inc.inline_compiled = Some(included_inline_template(
        included_segments,
        included_fm,
        declared_sanitize,
    ));
    Ok(())
}

fn compile_as_include_target(
    segments: &mut [md_tmpl_core::compiled::Segment],
    fm: &md_tmpl_core::Frontmatter,
) -> md_tmpl_core::compiled::DeclaredSanitize {
    md_tmpl_core::compiled::apply_frontmatter_sanitization(
        segments,
        &hashbrown::HashMap::new(),
        fm.sanitize_notice.as_deref(),
    );
    md_tmpl_core::compiled::declared_sanitize(&fm.param_sanitize, fm.sanitize_notice.as_deref())
}

fn included_inline_template(
    segments: Vec<md_tmpl_core::compiled::Segment>,
    fm: md_tmpl_core::Frontmatter,
    declared_sanitize: md_tmpl_core::compiled::DeclaredSanitize,
) -> md_tmpl_core::compiled::CompiledInlineTemplate {
    let mut consts = hashbrown::HashMap::new();
    for d in &fm.consts {
        if let Some(ref v) = d.default_value {
            consts.insert(d.name.clone(), v.clone());
        }
    }
    for d in &fm.env {
        if let Some(ref v) = d.default_value {
            consts.entry(d.name.clone()).or_insert_with(|| v.clone());
        }
    }
    md_tmpl_core::__private::inject_enum_type_constants(&fm.type_aliases, &mut consts);
    md_tmpl_core::compiled::CompiledInlineTemplate {
        segments: std::sync::Arc::from(segments),
        declarations: std::sync::Arc::from(fm.declarations),
        consts: std::sync::Arc::new(consts),
        imported_consts: std::sync::Arc::new(fm.imported_consts),
        declared_sanitize: std::sync::Arc::new(declared_sanitize),
    }
}

pub(crate) fn resolve_single_include(
    inc: &mut md_tmpl_core::compiled::CompiledInclude,
    base_dir: &std::path::Path,
    visited_paths: &mut HashSet<PathBuf>,
    env_values: &[(&str, md_tmpl_core::Value)],
    deps: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    let include_path = base_dir.join(inc.path.as_ref());
    let raw_source = std::fs::read_to_string(&include_path)
        .map_err(|e| format!("cannot read include {}: {e}", include_path.display()))?;
    let normalized = md_tmpl_core::consts::normalize_line_endings(&raw_source);
    let included_source = normalized.as_ref();
    deps.push(include_path.clone());

    let included_base_dir = include_path.parent().unwrap_or(base_dir);
    let (included_fm, included_body) = md_tmpl_core::parse_frontmatter_with_base_dir(
        included_source,
        included_base_dir,
        env_values,
    )
    .map_err(|e| format!("syntax error in include {}: {e}", include_path.display()))?;
    collect_import_deps(&included_fm, included_base_dir, deps);

    let (mut included_segments, included_inline_templates) =
        md_tmpl_core::compiled::compile_source_body_with_notice(
            included_source,
            included_body,
            &included_fm.type_aliases,
            &included_fm.type_alias_sanitize,
            included_fm.sanitize_notice.as_deref(),
        )
        .map_err(|e| {
            format!(
                "compilation error in include {}: {e}",
                include_path.display()
            )
        })?;
    let declared_sanitize = compile_as_include_target(&mut included_segments, &included_fm);

    let child_base_dir = include_path.parent().unwrap_or(base_dir);
    {
        let child_tmpl_params: HashSet<String> = included_fm
            .declarations
            .iter()
            .filter(|d| matches!(d.var_type, md_tmpl_core::VarType::Tmpl(_)))
            .map(|d| d.name.clone())
            .collect();
        let child_ctx = IncludeResolveCtx {
            inline_templates: &included_inline_templates,
            tmpl_params: &child_tmpl_params,
            env_values,
        };
        resolve_includes_recursive(
            &mut included_segments,
            child_base_dir,
            visited_paths,
            &child_ctx,
            deps,
            depth,
        )?;
    }
    if let Some(ctx) = inc.sanitize_ctx.as_deref() {
        md_tmpl_core::compiled::apply_include_context(
            &mut included_segments,
            ctx,
            &declared_sanitize,
        );
    }
    md_tmpl_core::compiled::check_untrusted_include_paths(&included_segments)
        .map_err(|e| format!("in include {}: {e}", include_path.display()))?;

    inc.inline_compiled = Some(included_inline_template(
        included_segments,
        included_fm,
        declared_sanitize,
    ));
    Ok(())
}
