//! Scoped variable resolution for rendering.

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use crate::{
    compat::HashMap,
    compiled::{CompiledInlineTemplate, ParsedFilter},
    context::Context,
    error::TemplateError,
    value::Value,
};

/// Maximum nesting depth for template includes.
///
/// Prevents infinite recursion from circular includes.
pub(crate) const MAX_INCLUDE_DEPTH: usize = 16;

/// Empty inline template map used as default when no inline templates exist.
static EMPTY_INLINE_TEMPLATES: crate::compat::LazyLock<HashMap<String, CompiledInlineTemplate>> =
    crate::compat::LazyLock::new(HashMap::new);

/// Loop metadata for a for-loop binding.
///
/// Stored per-binding in the scope so that `{{ idx(binding) }}` works
/// correctly even from deeply nested loops.
#[derive(Debug, Clone)]
pub(crate) struct LoopMeta {
    /// 0-based iteration index.
    pub index: i64,
}

/// A pre-split and compiled dotted path (e.g. `item.nested.field`).
///
/// Parsing once at compile time avoids string splitting, trimming,
/// and allocations during rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPath {
    raw: Cow<'static, str>,
    parts: Arc<[String]>,
}

impl CompiledPath {
    /// Compile a raw path string into a `CompiledPath`.
    #[must_use]
    pub fn compile(raw: &str) -> Self {
        let raw: Cow<'static, str> = Cow::Owned(raw.to_string());
        let parts: Arc<[String]> = raw
            .split(crate::consts::PATH_SEP)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Self { raw, parts }
    }

    /// Build a `CompiledPath` from a raw string and pre-split parts.
    ///
    /// Used by compile-time macros to avoid re-splitting at runtime.
    /// Both `raw` and each element of `parts` are `&'static str` from
    /// string literals baked into the binary.
    #[must_use]
    pub fn from_static(raw: &'static str, parts: &[&'static str]) -> Self {
        Self {
            raw: Cow::Borrowed(raw),
            parts: parts.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    /// Get the original raw path string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.raw.as_ref()
    }

    /// Get the pre-split path parts.
    #[must_use]
    pub fn parts(&self) -> &[String] {
        &self.parts
    }
}

/// A pre-compiled expression (path or built-in function call).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledExpr {
    /// A dotted variable path lookup.
    Path(CompiledPath),
    /// Loop index lookup `idx(binding)`.
    Idx(Cow<'static, str>),
    /// Length lookup `len(path)`.
    Len(CompiledPath),
    /// Variant name lookup `kind(path)`.
    Kind(CompiledPath),
    /// Enum variant names list lookup `kinds(path)`.
    Kinds(CompiledPath),
    /// Option presence check `has(path)` — returns `true` if option is `Some`.
    Has(CompiledPath),
}

impl CompiledExpr {
    /// Compile a raw expression token into a `CompiledExpr`.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if the token is empty or represents
    /// an unknown function.
    pub fn compile(raw: &str) -> Result<Self, TemplateError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(TemplateError::syntax("empty token in expression"));
        }

        if let Some((func_name, arg)) = parse_function_call(raw) {
            match func_name {
                crate::consts::FN_IDX => Ok(Self::Idx(Cow::Owned(arg.to_string()))),
                crate::consts::FN_LEN => Ok(Self::Len(CompiledPath::compile(arg))),
                crate::consts::FN_KIND => Ok(Self::Kind(CompiledPath::compile(arg))),
                crate::consts::FN_KINDS => Ok(Self::Kinds(CompiledPath::compile(arg))),
                crate::consts::FN_HAS => Ok(Self::Has(CompiledPath::compile(arg))),
                _ => Err(TemplateError::syntax(format!(
                    "unknown function '{func_name}'"
                ))),
            }
        } else {
            Ok(Self::Path(CompiledPath::compile(raw)))
        }
    }
}

/// A pre-compiled condition operand.
#[derive(Debug, Clone)]
pub enum ConditionOperand {
    /// A literal value (string, int, float, bool).
    Literal(Value),
    /// An interpolated string literal containing `{{ expr }}` segments.
    /// Compiled at parse time, evaluated at render time.
    InterpolatedStr(alloc::vec::Vec<crate::compiled::Segment>),
    /// A dotted path lookup, optionally followed by filters.
    Path {
        /// Dotted path.
        path: CompiledPath,
        /// Filter chain.
        filters: Vec<ParsedFilter>,
    },
    /// Loop index lookup `idx(binding)`.
    Idx(Cow<'static, str>),
    /// Length lookup `len(path)`.
    Len(CompiledPath),
    /// Variant name lookup `kind(path)`.
    Kind(CompiledPath),
    /// Enum variant names list lookup `kinds(path)`.
    Kinds(CompiledPath),
    /// Option presence check `has(path)` — returns `true` if option is `Some`.
    Has(CompiledPath),
}

impl ConditionOperand {
    /// Compile a raw condition operand token into a `ConditionOperand`.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if the token is empty or invalid.
    pub fn compile(token: &str) -> Result<Self, TemplateError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(TemplateError::syntax("empty token in expression"));
        }

        // 1. String literals (with optional {{ expr }} interpolation)
        if let Some(inner) = crate::consts::strip_string_literal(token) {
            let inner = crate::consts::unescape_string_literal(inner);
            if inner.contains(crate::consts::EXPR_START) {
                let segments = crate::compiled::compile_body(&inner)?;
                return Ok(Self::InterpolatedStr(segments));
            }
            return Ok(Self::Literal(Value::Str(inner)));
        }

        // 2. Boolean literals
        if token == crate::consts::LIT_TRUE {
            return Ok(Self::Literal(Value::Bool(true)));
        }
        if token == crate::consts::LIT_FALSE {
            return Ok(Self::Literal(Value::Bool(false)));
        }

        // 3. Integer literals
        if let Ok(val) = token.parse::<i64>() {
            return Ok(Self::Literal(Value::Int(val)));
        }

        // 4. Float literals
        if let Ok(val) = token.parse::<f64>() {
            return Ok(Self::Literal(Value::Float(val)));
        }

        // 5. Function calls: idx(binding), len(list), kind(enum)
        if let Some((func_name, arg)) = parse_function_call(token) {
            match func_name {
                crate::consts::FN_IDX => return Ok(Self::Idx(Cow::Owned(arg.to_string()))),
                crate::consts::FN_LEN => return Ok(Self::Len(CompiledPath::compile(arg))),
                crate::consts::FN_KIND => return Ok(Self::Kind(CompiledPath::compile(arg))),
                crate::consts::FN_KINDS => return Ok(Self::Kinds(CompiledPath::compile(arg))),
                crate::consts::FN_HAS => return Ok(Self::Has(CompiledPath::compile(arg))),
                _ => {
                    return Err(TemplateError::syntax(format!(
                        "unknown function '{func_name}'"
                    )));
                }
            }
        }

        // 6. Dotted path (possibly with filters)
        let (path_part, filter_chain) = crate::parser::split_pipe_aware(token);
        let path = CompiledPath::compile(path_part.trim());
        let mut filters = Vec::new();
        if !filter_chain.is_empty() {
            for filter_str in crate::parser::split_filters_aware(filter_chain) {
                let filter_str = filter_str.trim();
                if filter_str.is_empty() {
                    continue;
                }
                let (name, args) = crate::filter::parse_filter(filter_str);
                let kind = crate::compiled::parse_filter_kind(name)?;
                let parsed_num = args.and_then(|a| a.parse::<usize>().ok());
                filters.push(ParsedFilter {
                    kind,
                    args: args.map(|a| Cow::Owned(a.to_string())),
                    parsed_num,
                });
            }
        }
        Ok(Self::Path { path, filters })
    }

    /// Resolve this operand against the scope.
    ///
    /// Returns a [`Cow`] to avoid cloning in the common case where the
    /// operand is a literal or a path without filters.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if variable resolution or filter execution fails.
    pub fn resolve<'s>(&'s self, scope: &'s Scope<'_>) -> Result<Cow<'s, Value>, TemplateError> {
        match self {
            Self::Literal(val) => Ok(Cow::Borrowed(val)),
            Self::InterpolatedStr(segments) => {
                let rendered = crate::compiled::render_interpolated_str(segments, scope)?;
                Ok(Cow::Owned(Value::Str(rendered)))
            }
            Self::Path { path, filters } => {
                let value = scope.resolve_path(path)?;
                if filters.is_empty() {
                    Ok(Cow::Borrowed(value))
                } else {
                    let mut owned_value = value.clone();
                    for f in filters {
                        owned_value = crate::filter::apply_filter_typed(
                            f.kind,
                            &owned_value,
                            f.args.as_ref().map(AsRef::as_ref),
                        )?;
                    }
                    Ok(Cow::Owned(owned_value))
                }
            }
            Self::Idx(binding) => {
                let meta = scope.get_loop_meta(binding).ok_or_else(|| {
                    TemplateError::syntax(format!("idx() requires active loop binding '{binding}'"))
                })?;
                Ok(Cow::Owned(Value::Int(meta.index)))
            }
            Self::Len(path) => {
                let val = scope.resolve_path(path)?;
                let count = match val {
                    Value::List(l) => i64::try_from(l.len())
                        .map_err(|_| TemplateError::syntax("list length exceeds i64::MAX"))?,
                    Value::Str(s) => i64::try_from(s.len())
                        .map_err(|_| TemplateError::syntax("string length exceeds i64::MAX"))?,
                    _ => {
                        return Err(TemplateError::syntax("len() requires a list or string"));
                    }
                };

                Ok(Cow::Owned(Value::Int(count)))
            }
            Self::Kind(path) => {
                let val = scope.resolve_path(path)?;
                if scope.is_option_path(path.as_str()) {
                    return match val {
                        Value::None => {
                            Ok(Cow::Owned(Value::Str(crate::consts::OPTION_NONE.into())))
                        }
                        _ => Ok(Cow::Owned(Value::Str(crate::consts::OPTION_SOME.into()))),
                    };
                }
                match val {
                    Value::Struct(d) => {
                        if let Some(Value::Str(kind)) = d.get(crate::consts::ENUM_TAG_KEY) {
                            Ok(Cow::Owned(Value::Str(kind.clone())))
                        } else {
                            Err(TemplateError::syntax(
                                "kind() requires an enum value (dict with variant tag)",
                            ))
                        }
                    }
                    Value::Str(s) => Ok(Cow::Owned(Value::Str(s.clone()))),
                    Value::None => Ok(Cow::Owned(Value::Str(crate::consts::OPTION_NONE.into()))),
                    _ => Err(TemplateError::syntax(format!(
                        "kind() requires an enum value, got {}",
                        val.type_name()
                    ))),
                }
            }
            Self::Kinds(path) => {
                let val = scope.resolve_path(path)?;
                match val {
                    Value::Struct(d) => {
                        if let Some(list_val) = d.get(crate::consts::ENUM_VARIANTS_KEY) {
                            Ok(Cow::Borrowed(list_val))
                        } else {
                            Err(TemplateError::syntax(
                                "kinds() requires an enum type namespace",
                            ))
                        }
                    }
                    _ => Err(TemplateError::syntax(format!(
                        "kinds() requires an enum type namespace, got {}",
                        val.type_name()
                    ))),
                }
            }
            Self::Has(path) => {
                let val = scope.resolve_path(path)?;
                Ok(Cow::Owned(Value::Bool(Scope::is_option_some(val))))
            }
        }
    }
}

/// Layered scope for variable resolution during rendering.
///
/// The context holds top-level variables. Each `{% for %}` loop pushes
/// a new layer with the bound variable and `idx()` metadata. Resolution walks
/// layers top-to-bottom, then falls through to the context.
pub struct Scope<'a> {
    ctx: &'a Context,
    layers: Vec<HashMap<String, Value>>,
    /// Loop metadata keyed by binding name, parallel to `layers`.
    loop_metas: Vec<HashMap<String, LoopMeta>>,
    active_len: usize,
    /// Lightweight stack-based loop bindings.
    ///
    /// For-loops push `(binding_name, item, loop_meta)` here instead of into
    /// a `HashMap` layer.  `resolve()` checks this stack first (innermost-out),
    /// so loop variables are resolved in O(depth) with no hashing overhead.
    active_loop_bindings: usize,
    loop_bindings: Vec<(String, Value, Option<LoopMeta>)>,
    include_depth: usize,
    max_include_depth: usize,
    /// Pre-compiled inline template definitions (borrowed from top-level `Template`).
    inline_templates: &'a HashMap<String, CompiledInlineTemplate>,
    /// Stack of owned inline templates from included files. Each file push its
    /// own `{% tmpl %}` definitions when entered, and pops them when exited.
    /// `get_inline_template` checks this stack (innermost first) before
    /// falling back to the top-level `inline_templates`.
    inline_template_stack: Vec<HashMap<String, CompiledInlineTemplate>>,
    /// Optional include resolver for cached include resolution.
    #[cfg(feature = "std")]
    cache: Option<&'a dyn crate::cache::IncludeResolver>,
    /// stack of local constants from the template frontmatter.
    consts_stack: Vec<Arc<HashMap<String, Value>>>,
    /// Stack of imported constants keyed by `stem.NAME`.
    imported_consts_stack: Vec<Arc<HashMap<String, Value>>>,
    /// Stack of parameter declarations (used to check option types).
    declarations_stack: Vec<Arc<[crate::types::VarDecl]>>,
    /// Option params that have been narrowed to Some (unwrapped) in an
    /// enclosing match/if-has arm.  `is_option_path` returns `false` for
    /// narrowed params so that `kind()` and inner `match` blocks see the
    /// unwrapped enum value.
    narrowed_options: Vec<String>,
    /// Compile-time environment values for propagation to included files.
    #[cfg(feature = "std")]
    env_values: Arc<[(String, Value)]>,
}

impl<'a> Scope<'a> {
    /// Create a new scope backed by the given context.
    #[must_use]
    pub fn new(ctx: &'a Context) -> Self {
        Self {
            ctx,
            layers: Vec::new(),
            loop_metas: Vec::new(),
            active_len: 0,
            active_loop_bindings: 0,
            loop_bindings: Vec::with_capacity(4),
            include_depth: 0,
            max_include_depth: MAX_INCLUDE_DEPTH,
            inline_templates: &EMPTY_INLINE_TEMPLATES,
            inline_template_stack: Vec::new(),
            #[cfg(feature = "std")]
            cache: None,
            consts_stack: Vec::new(),
            imported_consts_stack: Vec::new(),
            declarations_stack: Vec::new(),
            narrowed_options: Vec::new(),
            #[cfg(feature = "std")]
            env_values: Arc::from([]),
        }
    }

    /// Create a new scope with an include resolver for faster include resolution.
    #[cfg(feature = "std")]
    pub(crate) fn with_cache(
        ctx: &'a Context,
        cache: &'a dyn crate::cache::IncludeResolver,
    ) -> Self {
        Self {
            ctx,
            layers: Vec::new(),
            loop_metas: Vec::new(),
            active_len: 0,
            active_loop_bindings: 0,
            loop_bindings: Vec::with_capacity(4),
            include_depth: 0,
            max_include_depth: MAX_INCLUDE_DEPTH,
            inline_templates: &EMPTY_INLINE_TEMPLATES,
            inline_template_stack: Vec::new(),
            cache: Some(cache),
            consts_stack: Vec::new(),
            imported_consts_stack: Vec::new(),
            declarations_stack: Vec::new(),
            narrowed_options: Vec::new(),
            env_values: Arc::from([]),
        }
    }

    /// Get the optional include resolver.
    #[cfg(feature = "std")]
    #[must_use]
    pub(crate) fn cache(&self) -> Option<&'a dyn crate::cache::IncludeResolver> {
        self.cache
    }

    /// Set compile-time environment values for propagation to included files.
    #[cfg(feature = "std")]
    pub(crate) fn set_compile_env(&mut self, env: Arc<[(String, Value)]>) {
        self.env_values = env;
    }

    /// Get the compile-time environment values.
    #[cfg(feature = "std")]
    #[must_use]
    pub(crate) fn compile_env(&self) -> &[(String, Value)] {
        &self.env_values
    }
    /// Push a new empty layer, returning a mutable reference to populate it.
    pub fn push_layer(&mut self) -> &mut HashMap<String, Value> {
        if self.active_len < self.layers.len() {
            self.layers[self.active_len].clear();
            self.loop_metas[self.active_len].clear();
        } else {
            self.layers.push(HashMap::new());
            self.loop_metas.push(HashMap::new());
        }
        self.active_len += 1;
        &mut self.layers[self.active_len - 1]
    }

    /// Pop the topmost layer (and its loop metadata).
    pub fn pop_layer(&mut self) {
        if self.active_len > 0 {
            self.active_len -= 1;
        }
    }

    /// Push a loop binding from a reference, reusing the existing string
    /// allocation when both old and new values are strings.
    ///
    /// In a loop of N iterations over strings, this avoids N-1 heap
    /// allocations by clearing and reusing the buffer from iteration 0.
    /// For non-string types (`Int`, `Bool`, `Arc`-wrapped `List`/`Struct`),
    /// `clone()` is already cheap.
    ///
    /// Used by for-loops to avoid `HashMap` layer overhead. The binding is
    /// checked before `HashMap` layers in `resolve()`.
    #[inline]
    pub(crate) fn push_loop_binding(&mut self, key: &str, value: &Value) {
        if self.active_loop_bindings < self.loop_bindings.len() {
            let slot = &mut self.loop_bindings[self.active_loop_bindings];
            // Skip key update if it already matches (common in for-loops
            // where the binding name is the same every iteration).
            if slot.0 != key {
                slot.0.clear();
                slot.0.push_str(key);
            }
            // Reuse string allocation when both old and new are strings.
            match (&mut slot.1, value) {
                (Value::Str(old), Value::Str(new)) if old.capacity() > 0 => {
                    old.clear();
                    old.push_str(new);
                }
                _ => slot.1 = value.clone(),
            }
            slot.2 = None;
        } else {
            self.loop_bindings
                .push((key.to_string(), value.clone(), None));
        }
        self.active_loop_bindings += 1;
    }

    /// Pop the most recent loop binding.
    #[inline]
    pub(crate) fn pop_loop_binding(&mut self) {
        if self.active_loop_bindings > 0 {
            self.active_loop_bindings -= 1;
        }
    }

    /// Register loop metadata for a for-loop binding.
    ///
    /// Must be called after `push_loop_binding` or `push_layer` to associate
    /// metadata with the current binding.
    pub(crate) fn set_loop_meta(&mut self, binding: &str, meta: LoopMeta) {
        // Fast path: the most common case is setting meta right after
        // push_loop_binding, so the target is at the top of the stack.
        if self.active_loop_bindings > 0 {
            let top = &mut self.loop_bindings[self.active_loop_bindings - 1];
            if top.0 == binding {
                top.2 = Some(meta);
                return;
            }
        }
        // Slow path: search the rest of the stack (innermost first).
        for (k, _, m) in self.loop_bindings[..self.active_loop_bindings]
            .iter_mut()
            .rev()
        {
            if k == binding {
                *m = Some(meta);
                return;
            }
        }
        // Fall back to HashMap-based layers (used by includes with for_each).
        if self.active_len > 0 {
            self.loop_metas[self.active_len - 1].insert(binding.to_string(), meta);
        }
    }

    /// Look up loop metadata for a binding name.
    ///
    /// Searches layers top-to-bottom, so the innermost loop with that binding
    /// wins — but outer bindings with different names remain accessible.
    pub(crate) fn get_loop_meta(&self, binding: &str) -> Option<&LoopMeta> {
        // Check lightweight loop_bindings stack first.
        for (k, _, m) in self.loop_bindings[..self.active_loop_bindings].iter().rev() {
            if k == binding {
                return m.as_ref();
            }
        }
        // Fall back to HashMap-based layers.
        for layer in self.loop_metas[..self.active_len].iter().rev() {
            if let Some(meta) = layer.get(binding) {
                return Some(meta);
            }
        }
        None
    }

    /// Set the inline template definitions for this scope.
    pub fn set_inline_templates(&mut self, templates: &'a HashMap<String, CompiledInlineTemplate>) {
        self.inline_templates = templates;
    }

    /// Set the constants for this scope.
    pub fn set_consts(
        &mut self,
        consts: &Arc<HashMap<String, Value>>,
        imported_consts: &Arc<HashMap<String, Value>>,
    ) {
        if !consts.is_empty() {
            self.consts_stack.push(Arc::clone(consts));
        }
        if !imported_consts.is_empty() {
            self.imported_consts_stack.push(Arc::clone(imported_consts));
        }
    }

    /// Push new constants onto the scope stack (used by includes).
    pub(crate) fn push_consts(
        &mut self,
        consts: HashMap<String, Value>,
        imported_consts: HashMap<String, Value>,
    ) {
        self.consts_stack.push(Arc::new(consts));
        self.imported_consts_stack.push(Arc::new(imported_consts));
    }

    /// Pop the most recently pushed constants.
    pub(crate) fn pop_consts(&mut self) {
        self.consts_stack.pop();
        self.imported_consts_stack.pop();
    }

    /// Set parameter declarations for this scope.
    pub fn with_declarations(mut self, decls: &Arc<[crate::types::VarDecl]>) -> Self {
        if !decls.is_empty() {
            self.declarations_stack.push(Arc::clone(decls));
        }
        self
    }

    /// Push parameter declarations onto the stack.
    pub(crate) fn push_declarations(&mut self, decls: &[crate::types::VarDecl]) {
        if !decls.is_empty() {
            self.declarations_stack.push(Arc::from(decls));
        }
    }

    /// Pop parameter declarations from the stack.
    pub(crate) fn pop_declarations(&mut self, decls: &[crate::types::VarDecl]) {
        if !decls.is_empty() {
            self.declarations_stack.pop();
        }
    }

    /// Check if a dotted variable path resolves to an option type in declared parameters.
    pub(crate) fn is_option_path(&self, arg: &str) -> bool {
        // If this path has been narrowed (unwrapped via case Some / if has()),
        // it is no longer an option for kind()/match purposes.
        if self.narrowed_options.iter().any(|s| s == arg) {
            return false;
        }
        let root = arg.split(crate::consts::PATH_SEP).next().unwrap_or(arg);
        for decls in self.declarations_stack.iter().rev() {
            if let Some(decl) = decls.iter().find(|d| d.name == root) {
                let mut current_type = &decl.var_type;
                if arg == root {
                    return current_type.is_option();
                }
                for part in arg.split(crate::consts::PATH_SEP).skip(1) {
                    match current_type {
                        crate::types::VarType::Struct(fields)
                        | crate::types::VarType::List(fields) => {
                            if let Some(f) = fields.iter().find(|d| d.name == part) {
                                current_type = &f.var_type;
                            } else {
                                return false;
                            }
                        }
                        crate::types::VarType::Option(inner) => {
                            current_type = inner;
                        }
                        _ => return false,
                    }
                }
                return current_type.is_option();
            }
        }
        false
    }

    /// Mark an option param as narrowed (unwrapped) in the current scope.
    ///
    /// After narrowing, `is_option_path(name)` returns `false` so inner match
    /// blocks and `kind()` see the unwrapped enum value.
    pub(crate) fn narrow_option(&mut self, name: &str) {
        self.narrowed_options.push(name.to_string());
    }

    /// Remove the most recent narrowing for `name`.
    ///
    /// Call when leaving the scope where the narrowing was applied (e.g.
    /// after rendering a `{% case Some %}` arm body).
    pub(crate) fn unnarrow_option(&mut self, name: &str) {
        if let Some(pos) = self.narrowed_options.iter().rposition(|s| s == name) {
            self.narrowed_options.remove(pos);
        }
    }

    /// Push an included file's own inline templates onto the scope stack.
    /// These take priority over the top-level templates during resolution.
    pub(crate) fn push_inline_templates(
        &mut self,
        templates: HashMap<String, CompiledInlineTemplate>,
    ) {
        self.inline_template_stack.push(templates);
    }

    /// Pop the most recently pushed inline template layer.
    pub(crate) fn pop_inline_templates(&mut self) {
        self.inline_template_stack.pop();
    }

    /// Look up a pre-compiled inline template by name.
    ///
    /// When inside an included file (stack is non-empty), only the current
    /// file's templates are checked. The stack acts as a scope boundary —
    /// parent templates do NOT leak into included files.
    #[must_use]
    pub fn get_inline_template(&self, name: &str) -> Option<&CompiledInlineTemplate> {
        if let Some(current_file_templates) = self.inline_template_stack.last() {
            // Inside an included file: only see THIS file's templates.
            current_file_templates.get(name)
        } else {
            // Top-level: use the borrowed templates from the root Template.
            self.inline_templates.get(name)
        }
    }

    /// Try to evaluate a function call expression like `idx(item)` or `len(items)`.
    ///
    /// Returns `None` if the expression doesn't look like a function call,
    /// `Some(Ok(...))` on success, or `Some(Err(...))` on evaluation failure.
    pub(crate) fn try_call_function(&self, expr: &str) -> Option<Result<Value, TemplateError>> {
        use crate::consts::{FN_HAS, FN_IDX, FN_KIND, FN_KINDS, FN_LEN};
        let (func_name, arg) = parse_function_call(expr)?;
        match func_name {
            FN_IDX => self.call_idx(arg),
            FN_LEN => Some(self.call_len(arg)),
            FN_KIND => Some(self.call_kind(arg)),
            FN_KINDS => Some(self.call_kinds(arg)),
            FN_HAS => Some(self.call_has(arg)),
            _ => None,
        }
    }

    /// Evaluate `idx(binding)` — returns the current loop index.
    fn call_idx(&self, arg: &str) -> Option<Result<Value, TemplateError>> {
        let meta = self.get_loop_meta(arg)?;
        Some(Ok(Value::Int(meta.index)))
    }

    /// Evaluate `len(path)` — returns the length of a list or string.
    fn call_len(&self, arg: &str) -> Result<Value, TemplateError> {
        let val = self.resolve_path_str(arg)?;
        let count = match val {
            // `.len()` cannot exceed `isize::MAX`, which always fits in `i64`.
            Value::List(l) => i64::try_from(l.len()).expect("len <= isize::MAX < i64::MAX"),
            Value::Str(s) => i64::try_from(s.len()).expect("len <= isize::MAX < i64::MAX"),
            _ => {
                return Err(TemplateError::syntax(format!(
                    "len() requires a list or string, got {}",
                    val.type_name()
                )));
            }
        };
        Ok(Value::Int(count))
    }

    /// Evaluate `kind(path)` — returns the variant name of an enum value.
    fn call_kind(&self, arg: &str) -> Result<Value, TemplateError> {
        use crate::consts::ENUM_TAG_KEY;
        let val = self.resolve_path_str(arg)?;
        if self.is_option_path(arg) {
            return match val {
                Value::None => Ok(Value::Str(crate::consts::OPTION_NONE.into())),
                _ => Ok(Value::Str(crate::consts::OPTION_SOME.into())),
            };
        }
        match val {
            Value::Struct(d) => {
                if let Some(Value::Str(kind)) = d.get(ENUM_TAG_KEY) {
                    Ok(Value::Str(kind.clone()))
                } else {
                    Err(TemplateError::syntax(
                        "kind() requires an enum value (dict with variant tag)",
                    ))
                }
            }
            Value::Str(s) => Ok(Value::Str(s.clone())),
            Value::None => Ok(Value::Str(crate::consts::OPTION_NONE.into())),
            _ => Err(TemplateError::syntax(format!(
                "kind() requires an enum value, got {}",
                val.type_name()
            ))),
        }
    }

    /// Evaluate `kinds(path)` — returns the variant names list of an enum type namespace.
    fn call_kinds(&self, arg: &str) -> Result<Value, TemplateError> {
        use crate::consts::ENUM_VARIANTS_KEY;
        let val = self.resolve_path_str(arg)?;
        match val {
            Value::Struct(d) => {
                if let Some(list_val) = d.get(ENUM_VARIANTS_KEY) {
                    Ok(list_val.clone())
                } else {
                    Err(TemplateError::syntax(
                        "kinds() requires an enum type namespace",
                    ))
                }
            }
            _ => Err(TemplateError::syntax(format!(
                "kinds() requires an enum type namespace, got {}",
                val.type_name()
            ))),
        }
    }

    /// Evaluate `has(path)` — returns `true` if an option value is `Some`.
    fn call_has(&self, arg: &str) -> Result<Value, TemplateError> {
        let val = self.resolve_path_str(arg)?;
        Ok(Value::Bool(Self::is_option_some(val)))
    }

    /// Check if a value represents a `Some` variant of an option.
    ///
    /// An option is absent (`None`) only when represented by [`Value::None`].
    /// Every other value counts as present (`Some`), including:
    /// - A `Struct` tagged `__kind__ = "Some"` (struct variant representation).
    /// - The literal string `"None"`, which is the `Some(None)` escape used by
    ///   the shared conformance convention (a present option whose inner string
    ///   value happens to be `"None"`).
    ///
    /// This means a non-option enum `Str("Active")` would also return `true`,
    /// which is acceptable since `has()` should only be used on `option(T)` types.
    pub(crate) fn is_option_some(val: &Value) -> bool {
        use crate::consts::{ENUM_TAG_KEY, OPTION_SOME};
        match val {
            // Explicit absent value — always not-present.
            Value::None => false,
            // Struct variant: check __kind__ tag.
            Value::Struct(d) => {
                if let Some(Value::Str(tag)) = d.get(ENUM_TAG_KEY) {
                    tag == OPTION_SOME
                } else {
                    // Struct without tag — not an option, treat as truthy.
                    true
                }
            }
            // Any other value (including strings): treat as present.
            _ => true,
        }
    }

    /// Set the maximum include depth for this scope (builder style).
    #[must_use]
    pub fn with_max_include_depth(mut self, depth: usize) -> Self {
        self.max_include_depth = depth;
        self
    }

    /// Enter an include: increment depth and check against the limit.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if the maximum include depth is
    /// exceeded (likely a circular include).
    pub fn enter_include(&mut self) -> Result<(), TemplateError> {
        self.include_depth += 1;
        if self.include_depth > self.max_include_depth {
            Err(TemplateError::syntax(format!(
                "maximum include depth ({}) exceeded — \
                 check for circular includes",
                self.max_include_depth
            )))
        } else {
            Ok(())
        }
    }

    /// Exit an include: decrement depth.
    pub fn exit_include(&mut self) {
        self.include_depth = self.include_depth.saturating_sub(1);
    }

    /// Resolve a simple (non-dotted) variable name.
    #[inline]
    #[must_use]
    pub fn resolve(&self, key: &str) -> Option<&Value> {
        // Fast path: no consts, no imported consts, and no layers — go straight to loop bindings + context.
        if self.consts_stack.is_empty()
            && self.imported_consts_stack.is_empty()
            && self.active_len == 0
        {
            // Check loop bindings (innermost first).
            for (k, v, _) in self.loop_bindings[..self.active_loop_bindings].iter().rev() {
                if k == key {
                    return Some(v);
                }
            }
            return self.ctx.get(key);
        }
        // 1. Local constants (strictly immutable, highest priority).
        // Search stack innermost first.
        for consts in self.consts_stack.iter().rev() {
            if let Some(v) = consts.get(key) {
                return Some(v);
            }
        }
        // 1b. Imported constants (type aliases, included template consts).
        for imported in self.imported_consts_stack.iter().rev() {
            if let Some(v) = imported.get(key) {
                return Some(v);
            }
        }
        // 2. Loop bindings (lightweight stack, checked before HashMap layers).
        for (k, v, _) in self.loop_bindings[..self.active_loop_bindings].iter().rev() {
            if k == key {
                return Some(v);
            }
        }
        // 3. Layered bindings (from for-loops with includes, etc.).
        for layer in self.layers[..self.active_len].iter().rev() {
            if let Some(v) = layer.get(key) {
                return Some(v);
            }
        }
        // 4. Fallback to render context.
        self.ctx.get(key)
    }

    /// Resolve a pre-compiled dotted path.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::UndefinedVariable`] if the root key or any
    /// intermediate field is not found.
    #[inline]
    pub fn resolve_path(&self, path: &CompiledPath) -> Result<&Value, TemplateError> {
        // Fast path for simple variables (no dots).
        if path.parts.len() == 1 {
            let root_key = &path.parts[0];
            return self
                .resolve(root_key)
                .ok_or_else(|| TemplateError::UndefinedVariable(root_key.clone()));
        }

        // 1. Check if it's an imported constant (stem.NAME[.field]*).
        if !self.imported_consts_stack.is_empty() && path.parts.len() >= 2 {
            // Build the "stem.NAME" key without heap allocation when possible.
            // Most const keys are short (< 128 bytes), so a stack buffer suffices.
            let p0 = &path.parts[0];
            let p1 = &path.parts[1];
            let needed = p0.len() + 1 + p1.len();
            let mut stack_buf = [0u8; 128];
            let stem_key: &str = if needed <= stack_buf.len() {
                stack_buf[..p0.len()].copy_from_slice(p0.as_bytes());
                stack_buf[p0.len()] = b'.';
                stack_buf[p0.len() + 1..needed].copy_from_slice(p1.as_bytes());
                // Both parts are valid UTF-8 and '.' is ASCII, so this never fails.
                core::str::from_utf8(&stack_buf[..needed]).unwrap_or(&path.raw)
            } else {
                // Fallback for very long names: heap allocate.
                // This branch is extremely rare in practice.
                &path.raw
            };

            for imported in self.imported_consts_stack.iter().rev() {
                if let Some(v) = imported.get(stem_key) {
                    let mut current = v;
                    for part in &path.parts[2..] {
                        current = current.get_field_unchecked(part).ok_or_else(|| {
                            TemplateError::UndefinedVariable(format!(
                                "field '{part}' not found on {}",
                                current.type_name()
                            ))
                        })?;
                    }
                    return Ok(current);
                }
            }
        }

        let root_key = &path.parts[0];
        let root = self
            .resolve(root_key)
            .ok_or_else(|| TemplateError::UndefinedVariable(root_key.clone()))?;

        let mut current = root;
        for (i, part) in path.parts[1..].iter().enumerate() {
            current = current.get_field_unchecked(part).ok_or_else(|| {
                let traversed: Vec<&str> =
                    path.parts[..=i + 1].iter().map(String::as_str).collect();
                let available = current.field_names_hint();
                let hint = if available.is_empty() {
                    String::new()
                } else {
                    format!(". Available fields: {}", available.join(", "))
                };
                TemplateError::UndefinedVariable(format!(
                    "field '{part}' not found on {} at path '{}'{hint}",
                    current.type_name(),
                    traversed.join("."),
                ))
            })?;
        }
        Ok(current)
    }

    /// Resolve a raw path string (used primarily by tests and fallback lookups).
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::UndefinedVariable`] if the key is not found.
    pub(crate) fn resolve_path_str(&self, path: &str) -> Result<&Value, TemplateError> {
        let path = path.trim();

        // Strip common prefixes: consts., opts., options., params.
        let path = if let Some(s) = path.strip_prefix(crate::consts::PREFIX_CONSTS_DOT) {
            s.trim()
        } else if let Some(s) = path.strip_prefix(crate::consts::PREFIX_OPTS_DOT) {
            s.trim()
        } else if let Some(s) = path.strip_prefix(crate::consts::PREFIX_OPTIONS_DOT) {
            s.trim()
        } else if let Some(s) = path.strip_prefix(crate::consts::PREFIX_PARAMS_DOT) {
            s.trim()
        } else {
            path
        };

        // Fast path for simple variables (no dots).
        if !path.contains(crate::consts::PATH_SEP) {
            return self
                .resolve(path)
                .ok_or_else(|| TemplateError::UndefinedVariable(path.to_string()));
        }

        // 1. Check if it's an imported constant (stem.NAME[.field]*).
        if !self.imported_consts_stack.is_empty() {
            for imported in self.imported_consts_stack.iter().rev() {
                let mut parts = path.split(crate::consts::PATH_SEP);
                let first = parts.next().unwrap_or("").trim();
                if let Some(second) = parts.next() {
                    let stem_name = format!("{}.{}", first, second.trim());
                    if let Some(v) = imported.get(&stem_name) {
                        let mut current = v;
                        for part in parts {
                            let part = part.trim();
                            current = current.get_field(part).ok_or_else(|| {
                                TemplateError::UndefinedVariable(format!(
                                    "field '{part}' not found on {}",
                                    current.type_name()
                                ))
                            })?;
                        }
                        return Ok(current);
                    }
                }
            }
        }

        let mut parts = path.split(crate::consts::PATH_SEP);
        let root_key = parts.next().unwrap_or("").trim();
        let root = self
            .resolve(root_key)
            .ok_or_else(|| TemplateError::UndefinedVariable(root_key.to_string()))?;

        let mut current = root;
        let mut traversed = root_key.to_string();
        for part in parts {
            let part = part.trim();
            traversed.push('.');
            traversed.push_str(part);
            current = current.get_field(part).ok_or_else(|| {
                let available = current.field_names_hint();
                let hint = if available.is_empty() {
                    String::new()
                } else {
                    format!(". Available fields: {}", available.join(", "))
                };
                TemplateError::UndefinedVariable(format!(
                    "field '{part}' not found on {} at path '{traversed}'{hint}",
                    current.type_name(),
                ))
            })?;
        }
        Ok(current)
    }
}

/// Parse a function call expression like `idx(item)` or `len(items)`.
///
/// Returns `(func_name, arg)` if the expression matches `identifier(expression)`,
/// or `None` if it doesn't look like a function call.
fn parse_function_call(expr: &str) -> Option<(&str, &str)> {
    let expr = expr.trim();
    let open = expr.find(crate::consts::PAREN_OPEN)?;
    if !expr.ends_with(crate::consts::PAREN_CLOSE) {
        return None;
    }
    let func_name = expr[..open].trim();
    let arg = expr[open + 1..expr.len() - 1].trim();
    if func_name.is_empty() || arg.is_empty() {
        return None;
    }
    // Ensure func_name is a valid identifier (no dots, pipes, etc.).
    if !func_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some((func_name, arg))
}
