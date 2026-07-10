use alloc::string::String;

use crate::{Template, compiled, context::Context, error::TemplateError, scope::Scope};

impl Template {
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
    /// use md_tmpl_core::Template;
    ///
    /// // No params — renders as-is
    /// let tmpl = Template::from_source(
    ///     r#"---
    /// params: []
    /// ---
    /// Hello world!"#,
    /// )
    /// .unwrap();
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
        let mut scope = Scope::new(&ctx)
            .with_max_include_depth(self.max_include_depth)
            .with_declarations(&self.declared_variables);
        if !self.consts.is_empty() || !self.imported_consts.is_empty() {
            scope.set_consts(&self.consts, &self.imported_consts);
        }
        scope.set_inline_templates(&self.inline_templates);
        #[cfg(feature = "std")]
        if !self.env_values.is_empty() {
            scope.set_compile_env(self.env_values.clone());
        }
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
        let mut scope = Scope::with_cache(&ctx, cache)
            .with_max_include_depth(self.max_include_depth)
            .with_declarations(&self.declared_variables);
        if !self.consts.is_empty() || !self.imported_consts.is_empty() {
            scope.set_consts(&self.consts, &self.imported_consts);
        }
        scope.set_inline_templates(&self.inline_templates);
        if !self.env_values.is_empty() {
            scope.set_compile_env(self.env_values.clone());
        }
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
        let mut scope = Scope::with_cache(&ctx, cache)
            .with_max_include_depth(self.max_include_depth)
            .with_declarations(&self.declared_variables);
        if !self.consts.is_empty() || !self.imported_consts.is_empty() {
            scope.set_consts(&self.consts, &self.imported_consts);
        }
        scope.set_inline_templates(&self.inline_templates);
        if !self.env_values.is_empty() {
            scope.set_compile_env(self.env_values.clone());
        }
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
    /// On the **first** call with a given `T`, the context is fully validated
    /// against frontmatter declarations. If validation passes,
    /// `TypeId::of::<T>()` is cached so that subsequent calls with the
    /// **same concrete type** skip validation entirely. This gives the
    /// safety of runtime type-checking with near-zero amortized cost.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if serialization fails, the value is not a
    /// struct/map, or rendering encounters an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use md_tmpl_core::Template;
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
    pub fn render<T: serde::Serialize + 'static>(
        &self,
        value: &T,
    ) -> Result<String, crate::error::TemplateError> {
        let ctx = Context::from_serialize(value)?;
        self.render_typed::<T>(&ctx)
    }

    /// Like [`render`](Self::render), but appends output into an
    /// existing buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if serialization fails, the value is not a
    /// struct/map, or rendering encounters an error.
    pub fn render_into<T: serde::Serialize + 'static>(
        &self,
        value: &T,
        output: &mut String,
    ) -> Result<(), crate::error::TemplateError> {
        let ctx = Context::from_serialize(value)?;
        self.render_into_typed::<T>(&ctx, output)
    }

    /// Render with TypeId-based validation caching.
    ///
    /// If `T` has been validated before (`TypeId` is cached), skips
    /// `validate_context` and goes straight to `render_core`.
    fn render_typed<T: 'static>(
        &self,
        ctx: &Context,
    ) -> Result<String, crate::error::TemplateError> {
        let mut output = String::with_capacity(self.estimated_capacity);
        self.render_into_typed::<T>(ctx, &mut output)?;
        Ok(output)
    }

    /// Render-into with TypeId-based validation caching.
    fn render_into_typed<T: 'static>(
        &self,
        ctx: &Context,
        output: &mut String,
    ) -> Result<(), crate::error::TemplateError> {
        #[cfg(feature = "std")]
        {
            let type_id = core::any::TypeId::of::<T>();
            let already_checked = self
                .checked_type_ids
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(&type_id);

            if already_checked {
                // Type has been validated before — skip straight to render.
                return self.render_core(ctx, output);
            }
            // First time seeing this type — validate, then cache on success.
            self.validate_context(ctx, false)?;
            self.checked_type_ids
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(type_id);
            self.render_core(ctx, output)
        }
        #[cfg(not(feature = "std"))]
        {
            self.validate_context(ctx, false)?;
            self.render_core(ctx, output)
        }
    }
}
