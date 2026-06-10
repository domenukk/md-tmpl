use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    cache::hash_source,
    compiled::{self, CompiledInlineTemplate, Segment},
    context::Context,
    error::TemplateError,
    frontmatter::{self, Frontmatter},
    scope::Scope,
    types::VarDecl,
};

/// A parsed template ready for rendering.
///
/// Templates can be loaded from files or parsed from in-memory strings.
/// Variable declarations from frontmatter are used for context validation
/// before rendering.
#[derive(Debug, Clone)]
pub struct Template {
    /// The template body text (after stripping frontmatter).
    body: String,
    /// Pre-compiled segment instructions (the fast render path).
    segments: Arc<[Segment]>,
    /// Declared variables from frontmatter.
    declared_variables: Arc<[VarDecl]>,
    /// Base directory for resolving includes (from file path).
    base_dir: Option<PathBuf>,
    /// Pre-compiled inline template definitions (`{% tmpl name %}...{% /tmpl %}`).
    inline_templates: Arc<HashMap<String, CompiledInlineTemplate>>,
    source_hash: u64,
    max_include_depth: usize,
    /// Pre-computed: true if any declared variable has a default value.
    has_defaults: bool,
}

impl Template {
    /// Load a template from a file, stripping YAML frontmatter.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Io`] if the file cannot be read.
    pub fn from_file(path: &Path) -> Result<Self, TemplateError> {
        let source = std::fs::read_to_string(path)?;
        let (tmpl, _fm) = Self::compile_from_source(&source, path.parent())?;
        Ok(tmpl)
    }

    /// Parse a template from an in-memory string (no include resolution).
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the body contains a syntax error.
    pub fn from_source(source: &str) -> Result<Self, TemplateError> {
        let (tmpl, _fm) = Self::compile_from_source(source, None)?;
        Ok(tmpl)
    }

    /// Parse a template from source, allowing declared parameters that are
    /// not referenced in the template body.
    ///
    /// Equivalent to setting `allow_unused: true` in the frontmatter.
    /// Useful for dynamically-loaded templates where parameters may be
    /// conditionally used or forwarded to includes.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the body contains a syntax error.
    pub fn from_source_allowing_unused(source: &str) -> Result<Self, TemplateError> {
        let (tmpl, _fm) = Self::compile_inner(source, None, true)?;
        Ok(tmpl)
    }

    /// Parse from source with a base directory for includes.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the body contains a syntax error.
    pub fn from_source_with_base_dir(source: &str, base_dir: &Path) -> Result<Self, TemplateError> {
        let (tmpl, _fm) = Self::compile_from_source(source, Some(base_dir))?;
        Ok(tmpl)
    }

    /// Parse and return frontmatter too.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the body contains a syntax error.
    pub fn from_source_with_frontmatter(
        source: &str,
    ) -> Result<(Self, Frontmatter), TemplateError> {
        Self::compile_from_source(source, None)
    }

    /// Load and return frontmatter too.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Io`] if the file cannot be read.
    pub fn from_file_with_frontmatter(path: &Path) -> Result<(Self, Frontmatter), TemplateError> {
        let source = std::fs::read_to_string(path)?;
        Self::compile_from_source(&source, path.parent())
    }

    /// Shared compilation entry point — honours `allow_unused` from frontmatter.
    fn compile_from_source(
        source: &str,
        base_dir: Option<&Path>,
    ) -> Result<(Self, Frontmatter), TemplateError> {
        Self::compile_inner(source, base_dir, false)
    }

    /// Core compilation: parse frontmatter → compile body → static analysis → build `Template`.
    ///
    /// When `force_allow_unused` is `true` the unused-params check is skipped
    /// regardless of the frontmatter setting.
    fn compile_inner(
        source: &str,
        base_dir: Option<&Path>,
        force_allow_unused: bool,
    ) -> Result<(Self, Frontmatter), TemplateError> {
        let source_hash = hash_source(source);
        let (fm, body) = frontmatter::parse_frontmatter(source)?;
        let body = body.to_string();
        let (segments, inline_templates) = compiled::compile(&body)?;

        // Static analysis: Enforce that all parameters referenced in the body are declared.
        let referenced = compiled::collect_referenced_params(&segments);
        let declared: std::collections::HashSet<&str> =
            fm.params.iter().map(String::as_str).collect();
        let undeclared: Vec<&String> = referenced
            .iter()
            .filter(|v| !declared.contains(v.as_str()))
            .collect();
        if !undeclared.is_empty() {
            let mut names: Vec<&str> = undeclared.iter().map(|s| s.as_str()).collect();
            names.sort_unstable();
            return Err(TemplateError::syntax(format!(
                "{}{}",
                crate::consts::ERR_UNDECLARED_PREFIX,
                names.join(", ")
            )));
        }

        // Static analysis: Reject declared parameters that are never referenced
        // (unless explicitly allowed via frontmatter or API).
        let allow_unused = force_allow_unused || fm.allow_unused;
        if !allow_unused {
            let unused: Vec<&str> = fm
                .declarations
                .iter()
                .filter(|decl| !referenced.contains(&decl.name))
                .map(|decl| decl.name.as_str())
                .collect();
            if !unused.is_empty() {
                return Err(TemplateError::syntax(format!(
                    "unused declared parameter(s): {}. Reference them in the template body, \
                     in a {{# comment #}}, or remove them from the frontmatter `params:` list. \
                     To suppress this check, add `allow_unused: true` to the frontmatter",
                    unused.join(", ")
                )));
            }
        }

        let has_defaults = fm.declarations.iter().any(|d| d.default_value.is_some());
        let tmpl = Self {
            body,
            segments: Arc::from(segments),
            declared_variables: Arc::from(fm.declarations.clone()),
            base_dir: base_dir.map(Path::to_path_buf),
            inline_templates: Arc::new(inline_templates),
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
        };
        Ok((tmpl, fm))
    }

    /// Construct a `Template` from pre-compiled segments (used by [`TemplateCache`]).
    ///
    /// Skips parsing and compilation entirely — the caller is responsible for
    /// providing correct, pre-compiled data.
    ///
    /// [`TemplateCache`]: crate::TemplateCache
    pub(crate) fn from_cached(
        segments: Arc<[Segment]>,
        declared_variables: Arc<[VarDecl]>,
        base_dir: Option<PathBuf>,
        inline_templates: Arc<HashMap<String, CompiledInlineTemplate>>,
        source_hash: u64,
    ) -> Self {
        let has_defaults = declared_variables.iter().any(|d| d.default_value.is_some());
        Self {
            body: String::new(),
            segments,
            declared_variables,
            base_dir,
            inline_templates,
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
        }
    }

    /// Construct a `Template` from pre-compiled static structures (used by compile-time macros).
    #[doc(hidden)]
    #[must_use]
    pub fn from_precompiled(
        segments: &[Segment],
        declared_variables: &[VarDecl],
        inline_templates: &[(&str, CompiledInlineTemplate)],
        source_hash: u64,
    ) -> Self {
        let inline_map = inline_templates
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        let has_defaults = declared_variables.iter().any(|d| d.default_value.is_some());
        Self {
            body: String::new(),
            segments: Arc::from(segments),
            declared_variables: Arc::from(declared_variables),
            base_dir: None,
            inline_templates: Arc::new(inline_map),
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
        }
    }

    /// Validate context: check presence, types, AND no extra variables.
    ///
    /// By default the engine is strict: passing undeclared parameters is
    /// an error. Set `allow_extra` to `true` to suppress the extra-params
    /// check (useful when forwarding a shared context to multiple templates).
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::MissingParams`] if any declared variable
    /// is absent, [`TemplateError::TypeMismatch`] if a value has the
    /// wrong type, or [`TemplateError::ExtraParams`] if undeclared keys
    /// are present (and `allow_extra` is false).
    fn validate_context(&self, ctx: &Context, allow_extra: bool) -> Result<(), TemplateError> {
        let mut missing = Vec::new();
        let mut mismatch: Option<(String, crate::types::TypeCheckError)> = None;
        for decl in self.declared_variables.iter() {
            match ctx.get(&decl.name) {
                None => {
                    // Skip params with defaults — they'll be injected.
                    if decl.default_value.is_none() {
                        missing.push(decl.name.as_str());
                    }
                }
                Some(value) => {
                    if mismatch.is_none()
                        && let Err(e) = decl.var_type.check(value)
                    {
                        mismatch = Some((decl.name.clone(), e));
                    }
                }
            }
        }
        // Report missing params first (most fundamental).
        if !missing.is_empty() {
            return Err(TemplateError::MissingParams(
                missing.into_iter().map(String::from).collect(),
            ));
        }
        if let Some((name, check_err)) = mismatch {
            let detail = if check_err.path.is_empty() {
                String::new()
            } else {
                format!(" (at .{})", check_err.path)
            };
            return Err(TemplateError::TypeMismatch {
                name: format!("{name}{detail}"),
                expected: check_err.expected,
                actual: check_err.actual,
                actual_value: check_err.actual_value,
            });
        }
        // Reject extra (undeclared) parameters unless explicitly allowed.
        if !allow_extra {
            let declared: std::collections::HashSet<&str> = self
                .declared_variables
                .iter()
                .map(|d| d.name.as_str())
                .collect();
            let extra: Vec<String> = ctx
                .values
                .keys()
                .filter(|k| !declared.contains(k.as_str()))
                .cloned()
                .collect();
            if !extra.is_empty() {
                return Err(TemplateError::ExtraParams(extra));
            }
        }
        Ok(())
    }

    /// Returns default values for all params that have them.
    #[must_use]
    pub fn defaults(&self) -> std::collections::HashMap<String, crate::value::Value> {
        self.declared_variables
            .iter()
            .filter_map(|d| {
                d.default_value
                    .as_ref()
                    .map(|v| (d.name.clone(), v.clone()))
            })
            .collect()
    }

    /// Returns the default value for a single parameter, if it has one.
    #[must_use]
    pub fn default(&self, name: &str) -> Option<&crate::value::Value> {
        self.declared_variables
            .iter()
            .find(|d| d.name == name)
            .and_then(|d| d.default_value.as_ref())
    }

    /// Returns a [`Context`] pre-filled with all default values.
    ///
    /// Use this as a starting point, then override only the params you need:
    /// ```
    /// # use prompt_templates::{Template, Context};
    /// let tmpl = Template::from_source(
    ///     "---\nparams:\n  - name = str\n  - count = int := 5\n---\n{{ name }} ({{ count }})",
    /// )
    /// .unwrap();
    /// let mut ctx = tmpl.defaults_context();
    /// ctx.set("name", "Alice"); // count already has default 5
    /// assert_eq!(tmpl.render(&ctx).unwrap(), "Alice (5)");
    /// ```
    #[must_use]
    pub fn defaults_context(&self) -> Context {
        let defaults = self.defaults();
        let mut ctx = Context::with_capacity(defaults.len());
        for (k, v) in defaults {
            ctx.set(k, v);
        }
        ctx
    }

    /// Return the raw template body text (after frontmatter stripping).
    ///
    /// Useful for compile-time validation and macro integration.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Set the maximum include depth for rendering this template.
    pub fn set_max_include_depth(&mut self, depth: usize) {
        self.max_include_depth = depth;
    }

    /// Set the maximum include depth for rendering this template (builder style).
    #[must_use]
    pub fn with_max_include_depth(mut self, depth: usize) -> Self {
        self.max_include_depth = depth;
        self
    }

    /// Return the declared variables from frontmatter.
    ///
    /// Used by generated param structs to validate that a reloaded template
    /// still matches the compile-time variable declarations.
    #[must_use]
    pub fn declarations(&self) -> &[VarDecl] {
        &self.declared_variables
    }

    /// Content hash of the raw source — use to detect unchanged files on
    /// hot-reload without re-parsing.
    ///
    /// Same source → same hash.  Different source → (very likely) different
    /// hash.  This is a fast non-cryptographic hash, not suitable for
    /// security purposes.
    #[must_use]
    pub fn source_hash(&self) -> u64 {
        self.source_hash
    }

    /// Validate that a (possibly reloaded) template's variable declarations
    /// match an expected set.
    ///
    /// Call this after re-loading a template from disk to ensure that
    /// nobody (e.g. an autonomous agent editing markdown files at runtime)
    /// has modified the `params:` block in the frontmatter.
    ///
    /// The template body may be changed freely — only the variable
    /// declarations must remain stable.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::DeclarationsMutated`] with a human-readable
    /// diff if the declarations don't match.
    pub fn validate_declarations(&self, expected: &[VarDecl]) -> Result<(), TemplateError> {
        let current: std::collections::HashMap<&str, &crate::types::VarType> = self
            .declared_variables
            .iter()
            .map(|d| (d.name.as_str(), &d.var_type))
            .collect();
        let expected_map: std::collections::HashMap<&str, &crate::types::VarType> = expected
            .iter()
            .map(|d| (d.name.as_str(), &d.var_type))
            .collect();

        let current_names: std::collections::HashSet<&str> = current.keys().copied().collect();
        let expected_names: std::collections::HashSet<&str> =
            expected_map.keys().copied().collect();

        let missing: Vec<&str> = expected_names.difference(&current_names).copied().collect();
        let extra: Vec<&str> = current_names.difference(&expected_names).copied().collect();

        // Check for type changes on variables that exist in both.
        let retyped: Vec<String> = current_names
            .intersection(&expected_names)
            .filter_map(|name| {
                let cur_type = current[name];
                let exp_type = expected_map[name];
                if cur_type == exp_type {
                    None
                } else {
                    Some(format!("{name}: {exp_type} → {cur_type}"))
                }
            })
            .collect();

        if missing.is_empty() && extra.is_empty() && retyped.is_empty() {
            return Ok(());
        }

        let mut parts = Vec::new();
        if !missing.is_empty() {
            parts.push(format!("removed: {}", missing.join(", ")));
        }
        if !extra.is_empty() {
            parts.push(format!("added: {}", extra.join(", ")));
        }
        if !retyped.is_empty() {
            parts.push(format!("retyped: {}", retyped.join(", ")));
        }

        Err(TemplateError::DeclarationsMutated {
            details: parts.join("; "),
        })
    }

    /// Render the template with the given context (strict mode).
    ///
    /// Validates the context against frontmatter declarations:
    /// - Missing declared parameters → error
    /// - Type mismatches → error
    /// - Extra undeclared parameters → error
    ///
    /// Use [`render_allowing_extra`](Self::render_allowing_extra) to permit
    /// undeclared parameters (e.g. when sharing a context across templates).
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs.
    pub fn render(&self, ctx: &Context) -> Result<String, TemplateError> {
        self.render_inner(ctx, false)
    }

    /// Render the template, allowing extra (undeclared) parameters.
    ///
    /// Like [`render`](Self::render), but extra context keys that aren't
    /// declared in frontmatter are silently ignored instead of producing
    /// an error. Useful when forwarding a shared context to multiple
    /// templates.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs.
    pub fn render_allowing_extra(&self, ctx: &Context) -> Result<String, TemplateError> {
        self.render_inner(ctx, true)
    }

    /// Internal render path with configurable strictness.
    fn render_inner(&self, ctx: &Context, allow_extra: bool) -> Result<String, TemplateError> {
        self.validate_context(ctx, allow_extra)?;
        let ctx = self.inject_defaults(ctx);
        let mut scope = Scope::new(&ctx).with_max_include_depth(self.max_include_depth);
        scope.set_inline_templates(&self.inline_templates);
        compiled::render_segments(&self.segments, &mut scope, self.base_dir.as_deref())
    }

    /// Render the template using a [`TemplateCache`](crate::TemplateCache) for include resolution.
    ///
    /// Like [`render`](Self::render), but included templates are resolved
    /// through the cache — unchanged includes are not re-read or re-compiled.
    /// This is the recommended rendering path for hot-reload scenarios where
    /// templates are re-rendered frequently.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs.
    pub fn render_cached<S: std::hash::BuildHasher + Send + Sync>(
        &self,
        ctx: &Context,
        cache: &crate::TemplateCache<S>,
    ) -> Result<String, TemplateError> {
        self.validate_context(ctx, false)?;
        let ctx = self.inject_defaults(ctx);
        let mut scope =
            Scope::with_cache(&ctx, cache).with_max_include_depth(self.max_include_depth);
        scope.set_inline_templates(&self.inline_templates);
        compiled::render_segments(&self.segments, &mut scope, self.base_dir.as_deref())
    }

    /// Inject default values for any declared params not present in `ctx`.
    ///
    /// Returns a `Cow::Borrowed` if no defaults needed, avoiding allocation.
    fn inject_defaults<'a>(&self, ctx: &'a Context) -> std::borrow::Cow<'a, Context> {
        if !self.has_defaults {
            return std::borrow::Cow::Borrowed(ctx);
        }
        let mut owned: Option<Context> = None;
        for decl in self.declared_variables.iter() {
            if let Some(ref default) = decl.default_value {
                let effective = owned.as_ref().unwrap_or(ctx);
                if effective.get(&decl.name).is_none() {
                    let ctx_mut = owned.get_or_insert_with(|| ctx.clone());
                    ctx_mut.set(decl.name.clone(), default.clone());
                }
            }
        }
        match owned {
            Some(ctx) => std::borrow::Cow::Owned(ctx),
            None => std::borrow::Cow::Borrowed(ctx),
        }
    }
}

#[cfg(feature = "serde")]
impl Template {
    /// Render the template from any `Serialize` struct.
    ///
    /// Struct fields become template variables — no manual `Context`
    /// construction needed.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if serialization fails, the value is not a
    /// struct/map, or rendering encounters an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use prompt_templates::Template;
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct Data {
    ///     name: String,
    ///     count: i64,
    /// }
    ///
    /// let tmpl = Template::from_source(
    ///     "---\nparams: [name = str, count = int]\n---\n{{ name }} has {{ count }} items",
    /// )
    /// .unwrap();
    /// let output = tmpl
    ///     .render_serde(&Data {
    ///         name: "Alice".into(),
    ///         count: 3,
    ///     })
    ///     .unwrap();
    /// assert_eq!(output, "Alice has 3 items");
    /// ```
    pub fn render_serde<T: serde::Serialize>(
        &self,
        value: &T,
    ) -> Result<String, crate::error::TemplateError> {
        let ctx = Context::from_serialize(value)?;
        self.render(&ctx)
    }
}

/// Load a named template from a directory.
///
/// Looks for `<name>.tmpl.md` in `dir`.
///
/// # Errors
///
/// Returns [`TemplateError::Io`] if the file is not found or cannot be read.
pub fn load_template(dir: &Path, name: &str) -> Result<Template, TemplateError> {
    let path = dir.join(format!("{name}.tmpl.md"));
    Template::from_file(&path)
}

#[cfg(test)]
mod tests;
