use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
#[cfg(feature = "std")]
use std::path::{Path, PathBuf};

use crate::{
    compat::{HashMap, HashSet},
    compiled::{self, CompiledInlineTemplate, Segment},
    context::Context,
    error::TemplateError,
    frontmatter::{self, Frontmatter},
    scope::Scope,
    types::{VarDecl, VarType},
    value::Value,
};

/// Configuration for template compilation.
///
/// Collects all optional parameters that previously required separate
/// constructor functions. Use with [`Template::compile`] or
/// [`Template::compile_file`].
///
/// # Examples
///
/// ```
/// use prompt_templates::CompileOptions;
///
/// // Default options (strict mode):
/// let opts = CompileOptions::default();
///
/// // Allow unused declared parameters:
/// let opts = CompileOptions::default().allow_unused(true);
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default)]
pub struct CompileOptions<'a> {
    /// When `true`, declared parameters that are never referenced in the
    /// template body are allowed instead of producing an error.
    ///
    /// Equivalent to `allow_unused: true` in frontmatter.
    pub allow_unused: bool,
    /// Base directory for resolving `{% include %}` and `{% import %}` directives.
    ///
    /// When `None`, includes are not resolved (suitable for in-memory templates).
    #[cfg(feature = "std")]
    pub base_dir: Option<&'a std::path::Path>,
    // Lifetime anchor for no_std where base_dir doesn't exist.
    #[cfg(not(feature = "std"))]
    _phantom: core::marker::PhantomData<&'a ()>,
}

#[cfg(feature = "std")]
impl<'a> CompileOptions<'a> {
    /// Set the base directory for include resolution.
    #[must_use]
    pub fn base_dir(mut self, dir: &'a std::path::Path) -> Self {
        self.base_dir = Some(dir);
        self
    }
}

impl CompileOptions<'_> {
    /// Allow unused declared parameters.
    #[must_use]
    pub fn allow_unused(mut self, allow: bool) -> Self {
        self.allow_unused = allow;
        self
    }
}

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
    #[cfg(feature = "std")]
    base_dir: Option<PathBuf>,
    /// Pre-compiled inline template definitions (`{% tmpl name %}...{% /tmpl %}`).
    inline_templates: Arc<HashMap<String, CompiledInlineTemplate>>,
    source_hash: u64,
    max_include_depth: usize,
    /// Pre-computed: true if any declared variable has a default value.
    has_defaults: bool,
    /// Constants defined in this template.
    consts: Arc<HashMap<String, crate::value::Value>>,
    /// Imported constants keyed by `stem.NAME`.
    imported_consts: Arc<HashMap<String, crate::value::Value>>,
    /// Pre-computed estimated output capacity (cached from segment tree walk).
    estimated_capacity: usize,
}

impl Template {
    /// Load a template from a file, stripping YAML frontmatter.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Io`] if the file cannot be read.
    #[cfg(feature = "std")]
    pub fn from_file(path: &Path) -> Result<Self, TemplateError> {
        let source = std::fs::read_to_string(path)?;
        let (tmpl, _fm) =
            Self::compile_from_source(&source, Some(path.parent().unwrap_or(Path::new("."))))?;
        Ok(tmpl)
    }

    /// Parse a template from an in-memory string (no include resolution).
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the body contains a syntax error.
    pub fn from_source(source: &str) -> Result<Self, TemplateError> {
        #[cfg(feature = "std")]
        let (tmpl, _fm) = Self::compile_from_source(source, None)?;
        #[cfg(not(feature = "std"))]
        let (tmpl, _fm) = Self::compile_from_source_no_std(source)?;
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
    #[deprecated(
        since = "0.2.0",
        note = "Use `Template::compile(source, CompileOptions::default().allow_unused(true))` instead"
    )]
    pub fn from_source_allowing_unused(source: &str) -> Result<Self, TemplateError> {
        let (tmpl, _fm) = Self::compile(source, CompileOptions::default().allow_unused(true))?;
        Ok(tmpl)
    }

    /// Parse from source with a base directory for includes.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the body contains a syntax error.
    #[cfg(feature = "std")]
    #[deprecated(
        since = "0.2.0",
        note = "Use `Template::compile(source, CompileOptions::default().base_dir(dir))` instead"
    )]
    pub fn from_source_with_base_dir(source: &str, base_dir: &Path) -> Result<Self, TemplateError> {
        let (tmpl, _fm) = Self::compile(source, CompileOptions::default().base_dir(base_dir))?;
        Ok(tmpl)
    }

    /// Parse and return frontmatter too.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the body contains a syntax error.
    #[deprecated(
        since = "0.2.0",
        note = "Use `Template::compile(source, CompileOptions::default())` which always returns Frontmatter"
    )]
    pub fn from_source_with_frontmatter(
        source: &str,
    ) -> Result<(Self, Frontmatter), TemplateError> {
        Self::compile(source, CompileOptions::default())
    }

    /// Load and return frontmatter too.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Io`] if the file cannot be read.
    #[cfg(feature = "std")]
    #[deprecated(
        since = "0.2.0",
        note = "Use `Template::compile_file(path, CompileOptions::default())` which always returns Frontmatter"
    )]
    pub fn from_file_with_frontmatter(path: &Path) -> Result<(Self, Frontmatter), TemplateError> {
        Self::compile_file(path, CompileOptions::default())
    }

    /// Parse a template from source with compile options, returning both the
    /// template and its frontmatter.
    ///
    /// This is the unified entry point that replaces the family of
    /// `from_source_*` constructors.
    ///
    /// # Examples
    ///
    /// ```
    /// use prompt_templates::{CompileOptions, Template};
    ///
    /// let (tmpl, fm) = Template::compile(
    ///     r#"---
    /// params: [name = str]
    /// ---
    /// Hello {{ name }}!"#,
    ///     CompileOptions::default(),
    /// )
    /// .unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the body contains a syntax error.
    pub fn compile(
        source: &str,
        options: CompileOptions<'_>,
    ) -> Result<(Self, Frontmatter), TemplateError> {
        #[cfg(feature = "std")]
        return Self::compile_inner(source, options.base_dir, options.allow_unused);
        #[cfg(not(feature = "std"))]
        return Self::compile_inner_no_std(source, options.allow_unused);
    }

    /// Load a template from a file with compile options, returning both the
    /// template and its frontmatter.
    ///
    /// The file's parent directory is used as the base directory for include
    /// resolution unless overridden in `options`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    ///
    /// use prompt_templates::{CompileOptions, Template};
    ///
    /// let (tmpl, fm) =
    ///     Template::compile_file(Path::new("template.tmpl.md"), CompileOptions::default()).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Io`] if the file cannot be read.
    #[cfg(feature = "std")]
    pub fn compile_file(
        path: &Path,
        options: CompileOptions<'_>,
    ) -> Result<(Self, Frontmatter), TemplateError> {
        let source = std::fs::read_to_string(path)?;
        let base_dir = options.base_dir.or_else(|| path.parent());
        Self::compile_inner(&source, base_dir, options.allow_unused)
    }

    /// Shared compilation entry point — honours `allow_unused` from frontmatter.
    #[cfg(feature = "std")]
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
    #[cfg(feature = "std")]
    fn compile_inner(
        source: &str,
        base_dir: Option<&Path>,
        force_allow_unused: bool,
    ) -> Result<(Self, Frontmatter), TemplateError> {
        let source_hash = crate::cache::hash_source(source);
        let (fm, body) = if let Some(dir) = base_dir {
            frontmatter::parse_frontmatter_with_base_dir(source, dir)?
        } else {
            frontmatter::parse_frontmatter(source)?
        };
        let body = body.to_string();
        let (segments, inline_templates) = compiled::compile(&body, &fm.type_aliases)?;

        // --- Static analysis ---
        let referenced = compiled::collect_referenced_params(&segments);
        check_undeclared_variables(&referenced, &fm, &inline_templates)?;
        check_unused_params(
            &fm.declarations,
            &referenced,
            force_allow_unused || fm.allow_unused,
        )?;
        check_name_collisions(&fm, &inline_templates, &segments)?;
        let enum_keys = collect_enum_type_keys(&fm);
        check_bare_enum_access(&segments, &enum_keys)?;

        let has_defaults = fm.declarations.iter().any(|d| d.default_value.is_some());
        let mut consts: HashMap<String, Value> = fm
            .consts
            .iter()
            .filter_map(|d| d.default_value.clone().map(|v| (d.name.clone(), v)))
            .collect();
        // Inject enum type aliases as namespace constants (e.g. Stage.Design).
        inject_enum_type_constants(&fm.type_aliases, &mut consts);
        let segments: Arc<[Segment]> = Arc::from(segments);
        let estimated_capacity = compiled::render::estimate_output_capacity(&segments);
        let tmpl = Self {
            body,
            segments,
            declared_variables: Arc::from(fm.declarations.clone()),
            base_dir: base_dir.map(Path::to_path_buf),
            inline_templates: Arc::new(inline_templates),
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
            consts: Arc::new(consts),
            imported_consts: Arc::new(fm.imported_consts.clone()),
            estimated_capacity,
        };
        Ok((tmpl, fm))
    }

    /// `no_std` compilation entry point (no base directory, no imports).
    #[cfg(not(feature = "std"))]
    fn compile_from_source_no_std(source: &str) -> Result<(Self, Frontmatter), TemplateError> {
        Self::compile_inner_no_std(source, false)
    }

    /// `no_std` core compilation.
    #[cfg(not(feature = "std"))]
    fn compile_inner_no_std(
        source: &str,
        force_allow_unused: bool,
    ) -> Result<(Self, Frontmatter), TemplateError> {
        let source_hash = hash_source_no_std(source);
        let (fm, body) = frontmatter::parse_frontmatter(source)?;
        let body = body.to_string();
        let (segments, inline_templates) = compiled::compile(&body, &fm.type_aliases)?;

        let referenced = compiled::collect_referenced_params(&segments);
        check_undeclared_variables(&referenced, &fm, &inline_templates)?;
        check_unused_params(
            &fm.declarations,
            &referenced,
            force_allow_unused || fm.allow_unused,
        )?;
        check_name_collisions(&fm, &inline_templates, &segments)?;
        let enum_keys = collect_enum_type_keys(&fm);
        check_bare_enum_access(&segments, &enum_keys)?;

        let has_defaults = fm.declarations.iter().any(|d| d.default_value.is_some());
        let mut consts: HashMap<String, Value> = fm
            .consts
            .iter()
            .filter_map(|d| d.default_value.clone().map(|v| (d.name.clone(), v)))
            .collect();
        // Inject enum type aliases as namespace constants (e.g. Stage.Design).
        inject_enum_type_constants(&fm.type_aliases, &mut consts);
        let segments: Arc<[Segment]> = Arc::from(segments);
        let estimated_capacity = compiled::render::estimate_output_capacity(&segments);
        let tmpl = Self {
            body,
            segments,
            declared_variables: Arc::from(fm.declarations.clone()),
            inline_templates: Arc::new(inline_templates),
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
            consts: Arc::new(consts),
            imported_consts: Arc::new(fm.imported_consts.clone()),
            estimated_capacity,
        };
        Ok((tmpl, fm))
    }

    /// Construct a `Template` from pre-compiled segments (used by [`TemplateCache`]).
    ///
    /// Skips parsing and compilation entirely — the caller is responsible for
    /// providing correct, pre-compiled data.
    ///
    /// [`TemplateCache`]: crate::TemplateCache
    #[cfg(feature = "std")]
    pub(crate) fn from_cached(
        segments: Arc<[Segment]>,
        declared_variables: Arc<[VarDecl]>,
        base_dir: Option<PathBuf>,
        inline_templates: Arc<HashMap<String, CompiledInlineTemplate>>,
        source_hash: u64,
        consts: Arc<HashMap<String, crate::value::Value>>,
        imported_consts: Arc<HashMap<String, crate::value::Value>>,
    ) -> Self {
        let has_defaults = declared_variables.iter().any(|d| d.default_value.is_some());
        let estimated_capacity = compiled::render::estimate_output_capacity(&segments);
        Self {
            body: String::new(),
            segments,
            declared_variables,
            base_dir,
            inline_templates,
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
            consts,
            imported_consts,
            estimated_capacity,
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
        consts: &[(&str, crate::value::Value)],
        imported_consts: &[(&str, crate::value::Value)],
    ) -> Self {
        let inline_map = inline_templates
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        let const_map = consts
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        let imported_const_map = imported_consts
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        let has_defaults = declared_variables.iter().any(|d| d.default_value.is_some());
        let segments: Arc<[Segment]> = Arc::from(segments);
        let estimated_capacity = compiled::render::estimate_output_capacity(&segments);
        Self {
            body: String::new(),
            segments,
            declared_variables: Arc::from(declared_variables),
            #[cfg(feature = "std")]
            base_dir: None,
            inline_templates: Arc::new(inline_map),
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
            consts: Arc::new(const_map),
            imported_consts: Arc::new(imported_const_map),
            estimated_capacity,
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
            let mut declared: HashSet<&str> = self
                .declared_variables
                .iter()
                .map(|d| d.name.as_str())
                .collect();
            for name in self.consts.keys() {
                declared.insert(name.as_str());
            }
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
    pub fn defaults(&self) -> HashMap<String, crate::value::Value> {
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
    ///     r#"---
    /// params:
    ///   - name = str
    ///   - count = int := 5
    /// ---
    /// {{ name }} ({{ count }})"#,
    /// )
    /// .unwrap();
    /// let mut ctx = tmpl.defaults_context();
    /// ctx.set("name", "Alice"); // count already has default 5
    /// assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Alice (5)");
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

    pub(crate) fn segments(&self) -> &[crate::compiled::Segment] {
        &self.segments
    }

    /// Returns the base directory used for resolving filesystem `{% include %}` paths.
    #[cfg(feature = "std")]
    #[must_use]
    pub fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_deref()
    }

    /// Returns the constants defined in this template's frontmatter.
    ///
    /// Constants are defined with `consts:` in frontmatter and are automatically
    /// available during rendering without being passed in the context.
    ///
    /// # Examples
    ///
    /// ```
    /// use prompt_templates::Template;
    ///
    /// let tmpl = Template::from_source(
    ///     r#"---
    /// consts:
    ///   - MAX = int := 100
    /// params: []
    /// ---
    /// {{ MAX }}"#,
    /// )
    /// .unwrap();
    /// let consts = tmpl.consts();
    /// assert_eq!(consts.get("MAX").unwrap().as_int(), Some(100));
    /// ```
    #[must_use]
    pub fn consts(&self) -> Arc<HashMap<String, Value>> {
        self.consts.clone()
    }

    /// Returns a borrowed reference to the constants defined in this
    /// template's frontmatter, avoiding the [`Arc`] clone of [`consts`](Self::consts).
    #[must_use]
    pub fn consts_ref(&self) -> &HashMap<String, Value> {
        &self.consts
    }

    /// Returns the imported constants (from `{% import %}` directives).
    ///
    /// These are constants imported from other template files and are
    /// automatically available during rendering alongside regular constants.
    #[must_use]
    pub fn imported_consts(&self) -> Arc<HashMap<String, Value>> {
        self.imported_consts.clone()
    }

    /// Returns a borrowed reference to the imported constants, avoiding
    /// the [`Arc`] clone of [`imported_consts`](Self::imported_consts).
    #[must_use]
    pub fn imported_consts_ref(&self) -> &HashMap<String, Value> {
        &self.imported_consts
    }

    pub(crate) fn inline_templates(&self) -> &HashMap<String, CompiledInlineTemplate> {
        &self.inline_templates
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
        let current: HashMap<&str, &crate::types::VarType> = self
            .declared_variables
            .iter()
            .map(|d| (d.name.as_str(), &d.var_type))
            .collect();
        let expected_map: HashMap<&str, &crate::types::VarType> = expected
            .iter()
            .map(|d| (d.name.as_str(), &d.var_type))
            .collect();

        let current_names: HashSet<&str> = current.keys().copied().collect();
        let expected_names: HashSet<&str> = expected_map.keys().copied().collect();

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
    /// Use [`render_ctx_allowing_extra`](Self::render_ctx_allowing_extra) to permit
    /// undeclared parameters (e.g. when sharing a context across templates).
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs.
    pub fn render_ctx(&self, ctx: &Context) -> Result<String, TemplateError> {
        self.render_inner(ctx, false)
    }

    /// Render the template, allowing extra (undeclared) parameters.
    ///
    /// Like [`render_ctx`](Self::render_ctx), but extra context keys that aren't
    /// declared in frontmatter are silently ignored instead of producing
    /// an error. Useful when forwarding a shared context to multiple
    /// templates.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs.
    pub fn render_ctx_allowing_extra(&self, ctx: &Context) -> Result<String, TemplateError> {
        self.render_inner(ctx, true)
    }

    /// Render a template that takes no user-provided parameters.
    ///
    /// If the template declares parameters, those **must** all have defaults.
    /// Calling `render_empty()` on a template with required (no-default)
    /// parameters returns [`TemplateError::MissingParams`].
    ///
    /// This is more efficient than `render(&empty_struct)` (no serde overhead)
    /// and more explicit than `render_ctx(&Context::new())`.
    ///
    /// # Examples
    ///
    /// ```
    /// use prompt_templates::Template;
    ///
    /// // No params — renders as-is
    /// let tmpl = Template::from_source("---\nparams: []\n---\nHello world!").unwrap();
    /// assert_eq!(tmpl.render_empty().unwrap(), "Hello world!");
    ///
    /// // All params have defaults
    /// let tmpl = Template::from_source(
    ///     r#"---
    /// params:
    ///   - greeting = str := "Hi"
    /// ---
    /// {{ greeting }}!"#,
    /// )
    /// .unwrap();
    /// assert_eq!(tmpl.render_empty().unwrap(), "Hi!");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::MissingParams`] if any declared parameter
    /// lacks a default value.
    pub fn render_empty(&self) -> Result<String, TemplateError> {
        let ctx = if self.has_defaults {
            self.defaults_context()
        } else {
            Context::new()
        };
        self.render_ctx(&ctx)
    }

    /// Like [`render_empty`](Self::render_empty), but appends to an existing buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::MissingParams`] if any declared parameter
    /// lacks a default value.
    pub fn render_empty_into(&self, output: &mut String) -> Result<(), TemplateError> {
        let ctx = if self.has_defaults {
            self.defaults_context()
        } else {
            Context::new()
        };
        self.render_ctx_into(&ctx, output)
    }

    /// Internal render path with configurable strictness.
    fn render_inner(&self, ctx: &Context, allow_extra: bool) -> Result<String, TemplateError> {
        let mut output = String::with_capacity(self.estimated_capacity);
        self.render_into_inner(ctx, allow_extra, &mut output)?;
        Ok(output)
    }

    /// Render the template directly into an existing `String` buffer.
    ///
    /// Unlike [`render_ctx`](Self::render_ctx), this appends to `output` without
    /// allocating a new `String`. Useful when composing multiple template
    /// outputs into a single buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs. On error, `output` may contain partial results.
    pub fn render_ctx_into(&self, ctx: &Context, output: &mut String) -> Result<(), TemplateError> {
        self.render_into_inner(ctx, false, output)
    }

    /// Like [`render_ctx_into`](Self::render_ctx_into), but allows extra (undeclared)
    /// parameters.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs.
    pub fn render_ctx_into_allowing_extra(
        &self,
        ctx: &Context,
        output: &mut String,
    ) -> Result<(), TemplateError> {
        self.render_into_inner(ctx, true, output)
    }

    /// Shared implementation for all render-into paths.
    fn render_into_inner(
        &self,
        ctx: &Context,
        allow_extra: bool,
        output: &mut String,
    ) -> Result<(), TemplateError> {
        self.validate_context(ctx, allow_extra)?;
        self.render_core(ctx, output)
    }

    /// Core rendering without any context validation.
    ///
    /// Used by both `render_into_inner` (after validation) and
    /// `render_ctx_unchecked` (no validation at all).
    fn render_core(&self, ctx: &Context, output: &mut String) -> Result<(), TemplateError> {
        let ctx = self.inject_defaults(ctx);
        let mut scope = Scope::new(&ctx).with_max_include_depth(self.max_include_depth);
        // Skip Arc clones when there are no constants (common case).
        if !self.consts.is_empty() || !self.imported_consts.is_empty() {
            scope.set_consts(&self.consts, &self.imported_consts);
        }
        scope.set_inline_templates(&self.inline_templates);
        #[cfg(feature = "std")]
        return compiled::render::render_segments_into(
            &self.segments,
            &mut scope,
            self.base_dir.as_deref(),
            output,
        );
        #[cfg(not(feature = "std"))]
        return compiled::render_segments_into_no_std(&self.segments, &mut scope, output);
    }

    /// Render the template **without** context validation.
    ///
    /// Skips the parameter presence, type, and extra-key checks that
    /// [`render_ctx`](Self::render_ctx) performs on every call. This is a safe
    /// operation — rendering errors (e.g. undefined variable) are still
    /// reported via `Err` — but the upfront validation overhead is removed.
    ///
    /// Use this when the context is known-good (e.g. constructed from a
    /// strongly-typed params struct, or pre-validated once at startup).
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if a rendering error occurs (e.g.
    /// undefined variable, filter error).
    pub fn render_ctx_unchecked(&self, ctx: &Context) -> Result<String, TemplateError> {
        let mut output = String::with_capacity(self.estimated_capacity);
        self.render_core(ctx, &mut output)?;
        Ok(output)
    }

    /// Render into a buffer **without** context validation.
    ///
    /// Like [`render_ctx_unchecked`](Self::render_ctx_unchecked), but appends to
    /// an existing buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if a rendering error occurs.
    pub fn render_ctx_into_unchecked(
        &self,
        ctx: &Context,
        output: &mut String,
    ) -> Result<(), TemplateError> {
        self.render_core(ctx, output)
    }

    /// Render the template using a [`TemplateCache`](crate::TemplateCache) for include resolution.
    ///
    /// Like [`render_ctx`](Self::render_ctx), but included templates are resolved
    /// through the cache — unchanged includes are not re-read or re-compiled.
    /// This is the recommended rendering path for hot-reload scenarios where
    /// templates are re-rendered frequently.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs.
    #[cfg(feature = "std")]
    pub fn render_ctx_cached<S: core::hash::BuildHasher + Send + Sync>(
        &self,
        ctx: &Context,
        cache: &crate::TemplateCache<S>,
    ) -> Result<String, TemplateError> {
        self.validate_context(ctx, false)?;
        let ctx = self.inject_defaults(ctx);
        let mut scope =
            Scope::with_cache(&ctx, cache).with_max_include_depth(self.max_include_depth);
        if !self.consts.is_empty() || !self.imported_consts.is_empty() {
            scope.set_consts(&self.consts, &self.imported_consts);
        }
        scope.set_inline_templates(&self.inline_templates);
        compiled::render_segments(&self.segments, &mut scope, self.base_dir.as_deref())
    }

    /// Render with caching, allowing extra parameters in the context.
    ///
    /// Like [`render_ctx_cached()`](Self::render_ctx_cached) but does not
    /// reject parameters not declared in the template frontmatter.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if validation fails or a rendering error
    /// occurs.
    #[cfg(feature = "std")]
    pub fn render_ctx_cached_allowing_extra<S: core::hash::BuildHasher + Send + Sync>(
        &self,
        ctx: &Context,
        cache: &crate::TemplateCache<S>,
    ) -> Result<String, TemplateError> {
        self.validate_context(ctx, true)?;
        let ctx = self.inject_defaults(ctx);
        let mut scope =
            Scope::with_cache(&ctx, cache).with_max_include_depth(self.max_include_depth);
        if !self.consts.is_empty() || !self.imported_consts.is_empty() {
            scope.set_consts(&self.consts, &self.imported_consts);
        }
        scope.set_inline_templates(&self.inline_templates);
        compiled::render_segments(&self.segments, &mut scope, self.base_dir.as_deref())
    }

    /// Inject default values for any declared params not present in `ctx`.
    ///
    /// Returns a `Cow::Borrowed` if no defaults needed, avoiding allocation.
    fn inject_defaults<'a>(&self, ctx: &'a Context) -> alloc::borrow::Cow<'a, Context> {
        if !self.has_defaults {
            return alloc::borrow::Cow::Borrowed(ctx);
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
            Some(ctx) => alloc::borrow::Cow::Owned(ctx),
            None => alloc::borrow::Cow::Borrowed(ctx),
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
    ///     r#"---
    /// params: [name = str, count = int]
    /// ---
    /// {{ name }} has {{ count }} items"#,
    /// )
    /// .unwrap();
    /// let output = tmpl
    ///     .render(&Data {
    ///         name: "Alice".into(),
    ///         count: 3,
    ///     })
    ///     .unwrap();
    /// assert_eq!(output, "Alice has 3 items");
    /// ```
    pub fn render<T: serde::Serialize>(
        &self,
        value: &T,
    ) -> Result<String, crate::error::TemplateError> {
        let ctx = Context::from_serialize(value)?;
        self.render_ctx(&ctx)
    }

    /// Like [`render`](Self::render), but appends output into an
    /// existing buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if serialization fails, the value is not a
    /// struct/map, or rendering encounters an error.
    pub fn render_into<T: serde::Serialize>(
        &self,
        value: &T,
        output: &mut String,
    ) -> Result<(), crate::error::TemplateError> {
        let ctx = Context::from_serialize(value)?;
        self.render_ctx_into(&ctx, output)
    }
}

// ---------------------------------------------------------------------------
// Trait impls for embedding Template in macro-generated structs
// ---------------------------------------------------------------------------

/// Two templates are considered equal if they were compiled from the same
/// source (compared via non-cryptographic 64-bit hash).
///
/// **Note:** This is an approximate comparison — different sources that
/// produce the same hash would incorrectly compare as equal.  Do not use
/// `Template` as a `HashMap` key or rely on `Eq` for deduplication in
/// security-sensitive contexts.  For exact source comparison, compare
/// [`body()`](Self::body) and [`declarations()`](Self::declarations).
impl PartialEq for Template {
    fn eq(&self, other: &Self) -> bool {
        self.source_hash == other.source_hash
    }
}

impl Eq for Template {}

/// Serialize a [`Template`] as a source-hash identifier.
///
/// Templates embedded in macro-generated parameter structs need `Serialize`
/// to satisfy derive bounds, even when the struct is never actually
/// serialized.  The hash lets debug/logging code produce something readable.
#[cfg(feature = "serde")]
impl serde::Serialize for Template {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("template:{:016x}", self.source_hash))
    }
}

/// Deserialize always fails — [`Template`] must be constructed from source.
///
/// This impl exists solely to satisfy derive bounds on macro-generated
/// parameter structs.  Actual deserialization of a compiled template is not
/// meaningful.
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Template {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let _ = <serde::de::IgnoredAny as serde::Deserialize>::deserialize(deserializer)?;
        Err(serde::de::Error::custom(
            "Template cannot be deserialized; construct from source with \
             Template::from_source() or Template::from_file()",
        ))
    }
}

/// Load a named template from a directory.
///
/// Looks for `<name>.tmpl.md` in `dir`.
///
/// # Errors
///
/// Returns [`TemplateError::Io`] if the file is not found or cannot be read.
#[cfg(feature = "std")]
pub fn load_template(dir: &Path, name: &str) -> Result<Template, TemplateError> {
    let path = dir.join(format!("{name}.tmpl.md"));
    Template::from_file(&path)
}
/// Inject enum type aliases as namespace constants.
///
/// For each enum type alias like `Stage = enum<Design, Build>`, this creates
/// a dict constant `Stage` → `{Design: "Design", Build: "Build"}`.
/// This enables expressions like `{{ kind(Stage.Design) }}` in templates.
/// Bare access like `{{ Stage.Design }}` is rejected at compile time —
/// users must wrap enum literals in `kind()` for explicit variant name extraction.
///
/// Unit variants map to `Value::Str(name)`. Struct variants map to a tagged
/// dict with just `__kind__` set (a partial value suitable for `kind()` and
/// match arms).
fn inject_enum_type_constants(
    type_aliases: &HashMap<String, VarType>,
    consts: &mut HashMap<String, Value>,
) {
    for (type_name, var_type) in type_aliases {
        let VarType::Enum(variants) = var_type else {
            continue;
        };
        // Don't overwrite a user-defined constant with the same name.
        if consts.contains_key(type_name) {
            continue;
        }
        let mut variant_map = HashMap::new();
        for variant in variants {
            if variant.fields.is_empty() {
                // Unit variant → simple string.
                variant_map.insert(variant.name.clone(), Value::Str(variant.name.clone()));
            } else {
                // Struct variant → tagged dict with __kind__ only.
                let mut partial = HashMap::new();
                partial.insert(
                    crate::consts::ENUM_TAG_KEY.into(),
                    Value::Str(variant.name.clone()),
                );
                variant_map.insert(variant.name.clone(), Value::Struct(Arc::new(partial)));
            }
        }
        consts.insert(type_name.clone(), Value::Struct(Arc::new(variant_map)));
    }
}

/// Collect the set of enum type names (both local and imported).
///
/// For local types, stores just the type name (e.g. `"Stage"`).
/// For imported types, stores the full `stem.TypeName` key (e.g.
/// `"lib.Color"`), so that non-enum imports like `lib.MAX_TIMEOUT`
/// are not incorrectly flagged.
fn collect_enum_type_keys(fm: &Frontmatter) -> HashSet<String> {
    let mut keys = HashSet::new();
    // Local enum types.
    for (name, ty) in &fm.type_aliases {
        if matches!(ty, VarType::Enum(_)) {
            keys.insert(name.clone());
        }
    }
    // Imported enum types: recorded during frontmatter import resolution.
    for key in &fm.imported_enum_type_keys {
        keys.insert(key.clone());
    }
    keys
}

/// Reject bare enum literal expressions like `{{ Stage.Design }}`.
///
/// Enum type namespaces are injected as dict constants so that `kind()` can
/// access them, but they should not be rendered directly. Instead, users
/// must wrap them in `kind()`: `{{ kind(Stage.Design) }}`.
///
/// This prevents accidental confusion between enum type access and regular
/// variable dot-access.
fn check_bare_enum_access(
    segments: &[compiled::Segment],
    enum_keys: &HashSet<String>,
) -> Result<(), TemplateError> {
    for seg in segments {
        match seg {
            compiled::Segment::Expr {
                expr: compiled::CompiledExpr::Path(path),
                ..
            } => {
                let parts = path.parts();
                if parts.len() >= 2 && is_enum_path(parts, enum_keys) {
                    return Err(TemplateError::syntax(format!(
                        "bare enum literal '{}' is not allowed — \
                         use kind({}) to get the variant name as a string",
                        path.as_str(),
                        path.as_str(),
                    )));
                }
            }
            compiled::Segment::ForLoop { body, .. } => {
                check_bare_enum_access(body, enum_keys)?;
            }
            compiled::Segment::If {
                branches,
                else_body,
            } => {
                for (_, branch_body) in branches {
                    check_bare_enum_access(branch_body, enum_keys)?;
                }
                check_bare_enum_access(else_body, enum_keys)?;
            }
            compiled::Segment::Match { arms, .. } => {
                for (_, arm_body) in arms {
                    check_bare_enum_access(arm_body, enum_keys)?;
                }
            }
            compiled::Segment::Include(inc) => {
                if let Some(ref inline) = inc.inline_compiled {
                    check_bare_enum_access(&inline.segments, enum_keys)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Check if a dotted path matches a known enum type namespace.
///
/// - Local: `["Stage", "Design"]` → checks `"Stage"` in keys.
/// - Imported: `["lib", "Color", "Red"]` → checks `"lib.Color"` in keys.
fn is_enum_path(parts: &[String], enum_keys: &HashSet<String>) -> bool {
    // Try 1-part root (local enum type).
    if enum_keys.contains(&parts[0]) {
        return true;
    }
    // Try 2-part root (imported enum type: stem.TypeName).
    if parts.len() >= 3 {
        let key = format!("{}.{}", parts[0], parts[1]);
        if enum_keys.contains(&key) {
            return true;
        }
    }
    false
}

/// Enforce that all parameters referenced in the body are declared.
///
/// Builds the full set of declared names (params, consts, import stems, inline
/// template names) and checks every referenced variable against it. Produces
/// 'did you mean?' suggestions via Levenshtein distance for near-misses.
fn check_undeclared_variables(
    referenced: &HashSet<String>,
    fm: &Frontmatter,
    inline_templates: &HashMap<String, CompiledInlineTemplate>,
) -> Result<(), TemplateError> {
    let mut declared: HashSet<String> = fm.params.iter().cloned().collect();
    for c in &fm.consts {
        declared.insert(c.name.clone());
    }
    for import in &fm.imports {
        declared.insert(import.stem.clone());
    }
    // Enum type aliases are auto-injected as namespace constants,
    // so references like `Stage.Design` (root = `Stage`) are valid.
    for (name, ty) in &fm.type_aliases {
        if matches!(ty, VarType::Enum(_)) {
            declared.insert(name.clone());
        }
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
    if undeclared.is_empty() {
        return Ok(());
    }

    let mut names: Vec<&str> = undeclared.iter().map(|s| s.as_str()).collect();
    names.sort_unstable();

    // Collect 'did you mean?' suggestions for each undeclared name.
    let mut suggestions = Vec::new();
    for name in &names {
        let mut best: Option<(&str, usize)> = None;
        for candidate in &declared {
            let dist = crate::error::levenshtein_distance(name, candidate);
            if dist > 0 && dist <= 2 && best.is_none_or(|b| dist < b.1) {
                best = Some((candidate, dist));
            }
        }
        if let Some((suggestion, _)) = best {
            suggestions.push(format!("'{name}' (did you mean '{suggestion}'?)"));
        }
    }
    let suffix = if suggestions.is_empty() {
        String::new()
    } else {
        format!(". Suggestions: {}", suggestions.join(", "))
    };
    Err(TemplateError::syntax(format!(
        "{}{}{suffix}",
        crate::consts::ERR_UNDECLARED_PREFIX,
        names.join(", ")
    )))
}

/// Reject declared parameters that are never referenced in the body.
///
/// Skipped when `allow_unused` is `true` (set via frontmatter or API).
fn check_unused_params(
    declarations: &[VarDecl],
    referenced: &HashSet<String>,
    allow_unused: bool,
) -> Result<(), TemplateError> {
    if allow_unused {
        return Ok(());
    }
    let unused: Vec<&str> = declarations
        .iter()
        .filter(|decl| !referenced.contains(&decl.name))
        .map(|decl| decl.name.as_str())
        .collect();
    if unused.is_empty() {
        return Ok(());
    }
    Err(TemplateError::syntax(format!(
        "unused declared parameter(s): {}. Reference them in the template body, \
         in a {{# comment #}}, or remove them from the frontmatter `params:` list. \
         To suppress this check, add `allow_unused: true` to the frontmatter",
        unused.join(", ")
    )))
}

/// Check for namespace collisions between imports, params/consts, and inline
/// templates (Rules 11, 12, 13).
///
/// - **Rule 11**: Import stem vs inline template name.
/// - **Rule 12**: Param/const name vs inline template name.
/// - **Rule 13**: For-loop bindings must not shadow any declared name.
fn check_name_collisions(
    fm: &Frontmatter,
    inline_templates: &HashMap<String, CompiledInlineTemplate>,
    segments: &[Segment],
) -> Result<(), TemplateError> {
    // Rule 11: Import stem vs inline template name collision.
    for import in &fm.imports {
        if inline_templates.contains_key(&import.stem) {
            return Err(TemplateError::syntax(format!(
                "import stem '{}' conflicts with inline template name",
                import.stem
            )));
        }
    }

    // Rule 12: Param/const name vs inline template name collision.
    // Check against params + consts only — NOT the full declared set which
    // already contains inline template names (for undeclared-var analysis).
    let param_and_const_names: HashSet<&str> = fm
        .params
        .iter()
        .map(String::as_str)
        .chain(fm.consts.iter().map(|c| c.name.as_str()))
        .collect();
    for inline_name in inline_templates.keys() {
        if param_and_const_names.contains(inline_name.as_str()) {
            return Err(TemplateError::syntax(format!(
                "inline template name '{inline_name}' conflicts with a declared parameter or constant"
            )));
        }
    }

    // Rule 13: for-loop bindings must not shadow declared names.
    let protected_names: HashSet<&str> = fm
        .params
        .iter()
        .map(String::as_str)
        .chain(fm.consts.iter().map(|c| c.name.as_str()))
        .chain(fm.imports.iter().map(|i| i.stem.as_str()))
        .chain(inline_templates.keys().map(String::as_str))
        .collect();
    validate_for_bindings(segments, &protected_names)
}

/// Walk compiled segments and reject any for-loop binding that shadows a
/// protected name (param, const, import stem, or inline template).
///
/// Sequential for-loops with the same binding are allowed — the binding
/// is scoped to the loop body and does not persist.
fn validate_for_bindings(
    segments: &[crate::compiled::Segment],
    protected: &HashSet<&str>,
) -> Result<(), TemplateError> {
    use crate::compiled::Segment;
    for seg in segments {
        match seg {
            Segment::ForLoop { binding, body, .. } => {
                if protected.contains(binding.as_ref()) {
                    return Err(TemplateError::syntax(format!(
                        "{} declared name '{binding}'",
                        crate::consts::ERR_FOR_BINDING_SHADOWS,
                    )));
                }
                validate_for_bindings(body, protected)?;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (_cond, branch_body) in branches {
                    validate_for_bindings(branch_body, protected)?;
                }
                validate_for_bindings(else_body, protected)?;
            }
            Segment::Match { arms, .. } => {
                for (_variants, arm_body) in arms {
                    validate_for_bindings(arm_body, protected)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Simple FNV-1a hash for `no_std` environments.
///
/// Delegates to the shared implementation in [`crate::__private::fnv1a_hash`].
#[cfg(not(feature = "std"))]
fn hash_source_no_std(source: &str) -> u64 {
    crate::__private::fnv1a_hash(source.as_bytes())
}

#[cfg(all(test, feature = "std"))]
mod adversarial_tests;
#[cfg(all(test, feature = "std"))]
mod collision_and_scope_tests;
#[cfg(all(test, feature = "std"))]
mod const_tests;
#[cfg(all(test, feature = "std"))]
mod error_diagnostic_tests;
#[cfg(all(test, feature = "std"))]
mod higher_order_tests;
#[cfg(all(test, feature = "std"))]
mod inline_edge_tests;
#[cfg(all(test, feature = "std"))]
mod render_integration_tests;
#[cfg(all(test, feature = "std"))]
mod shared_tests;
#[cfg(all(test, feature = "std"))]
mod tests;
