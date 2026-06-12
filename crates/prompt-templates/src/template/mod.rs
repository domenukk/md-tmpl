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
    value::Value,
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
    /// Constants defined in this template.
    consts: Arc<HashMap<String, crate::value::Value>>,
    /// Imported constants keyed by `stem.NAME`.
    imported_consts: Arc<HashMap<String, crate::value::Value>>,
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

        let has_defaults = fm.declarations.iter().any(|d| d.default_value.is_some());
        let consts = fm
            .consts
            .iter()
            .filter_map(|d| d.default_value.clone().map(|v| (d.name.clone(), v)))
            .collect();
        let tmpl = Self {
            body,
            segments: Arc::from(segments),
            declared_variables: Arc::from(fm.declarations.clone()),
            base_dir: base_dir.map(Path::to_path_buf),
            inline_templates: Arc::new(inline_templates),
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
            consts: Arc::new(consts),
            imported_consts: Arc::new(fm.imported_consts.clone()),
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
        consts: Arc<HashMap<String, crate::value::Value>>,
        imported_consts: Arc<HashMap<String, crate::value::Value>>,
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
            consts,
            imported_consts,
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
        Self {
            body: String::new(),
            segments: Arc::from(segments),
            declared_variables: Arc::from(declared_variables),
            base_dir: None,
            inline_templates: Arc::new(inline_map),
            source_hash,
            max_include_depth: crate::scope::MAX_INCLUDE_DEPTH,
            has_defaults,
            consts: Arc::new(const_map),
            imported_consts: Arc::new(imported_const_map),
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
            let mut declared: std::collections::HashSet<&str> = self
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

    pub(crate) fn segments(&self) -> &[crate::compiled::Segment] {
        &self.segments
    }

    pub(crate) fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_deref()
    }

    pub(crate) fn consts(&self) -> Arc<HashMap<String, Value>> {
        self.consts.clone()
    }

    pub(crate) fn imported_consts(&self) -> Arc<HashMap<String, Value>> {
        self.imported_consts.clone()
    }

    pub(crate) fn inline_templates(&self) -> Arc<HashMap<String, CompiledInlineTemplate>> {
        self.inline_templates.clone()
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
        scope.set_consts(&self.consts, &self.imported_consts);
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
        scope.set_consts(&self.consts, &self.imported_consts);
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

// ---------------------------------------------------------------------------
// Trait impls for embedding Template in macro-generated structs
// ---------------------------------------------------------------------------

/// Two templates are equal if they were compiled from the same source.
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
pub fn load_template(dir: &Path, name: &str) -> Result<Template, TemplateError> {
    let path = dir.join(format!("{name}.tmpl.md"));
    Template::from_file(&path)
}

/// Enforce that all parameters referenced in the body are declared.
///
/// Builds the full set of declared names (params, consts, import stems, inline
/// template names) and checks every referenced variable against it. Produces
/// 'did you mean?' suggestions via Levenshtein distance for near-misses.
fn check_undeclared_variables(
    referenced: &std::collections::HashSet<String>,
    fm: &Frontmatter,
    inline_templates: &HashMap<String, CompiledInlineTemplate>,
) -> Result<(), TemplateError> {
    let mut declared: std::collections::HashSet<String> = fm.params.iter().cloned().collect();
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
            if dist > 0 && dist <= 2 && (best.is_none() || dist < best.unwrap().1) {
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
    referenced: &std::collections::HashSet<String>,
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
    let param_and_const_names: std::collections::HashSet<&str> = fm
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
    let protected_names: std::collections::HashSet<&str> = fm
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
    protected: &std::collections::HashSet<&str>,
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

#[cfg(test)]
mod adversarial_tests;
#[cfg(test)]
mod const_tests;
#[cfg(test)]
mod error_diagnostic_tests;
#[cfg(test)]
mod higher_order_tests;
#[cfg(test)]
mod tests;
