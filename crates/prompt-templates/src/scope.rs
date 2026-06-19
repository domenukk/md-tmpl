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
static EMPTY_INLINE_TEMPLATES: crate::compat::Lazy<HashMap<String, CompiledInlineTemplate>> =
    crate::compat::Lazy::new(HashMap::new);

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
            .split('.')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Self { raw, parts }
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
    Idx(String),
    /// Length lookup `len(path)`.
    Len(CompiledPath),
    /// Variant name lookup `kind(path)`.
    Kind(CompiledPath),
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
                crate::consts::FN_IDX => Ok(Self::Idx(arg.to_string())),
                crate::consts::FN_LEN => Ok(Self::Len(CompiledPath::compile(arg))),
                crate::consts::FN_KIND => Ok(Self::Kind(CompiledPath::compile(arg))),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionOperand {
    /// A literal value (string, int, float, bool).
    Literal(Value),
    /// A dotted path lookup, optionally followed by filters.
    Path {
        /// Dotted path.
        path: CompiledPath,
        /// Filter chain.
        filters: Vec<ParsedFilter>,
    },
    /// Loop index lookup `idx(binding)`.
    Idx(String),
    /// Length lookup `len(path)`.
    Len(CompiledPath),
    /// Variant name lookup `kind(path)`.
    Kind(CompiledPath),
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

        // 1. String literals
        if let Some(inner) = crate::consts::strip_string_literal(token) {
            return Ok(Self::Literal(Value::Str(inner.to_string())));
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
                crate::consts::FN_IDX => return Ok(Self::Idx(arg.to_string())),
                crate::consts::FN_LEN => return Ok(Self::Len(CompiledPath::compile(arg))),
                crate::consts::FN_KIND => return Ok(Self::Kind(CompiledPath::compile(arg))),
                crate::consts::FN_HAS => return Ok(Self::Has(CompiledPath::compile(arg))),
                _ => {
                    return Err(TemplateError::syntax(format!(
                        "unknown function '{func_name}'"
                    )));
                }
            }
        }

        // Reject .length suffix
        if let Some(base_path) = token.strip_suffix(crate::consts::PSEUDO_FIELD_LENGTH) {
            return Err(TemplateError::syntax(format!(
                "'.length' is not supported — use len({base_path}) instead"
            )));
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
                filters.push(ParsedFilter {
                    kind,
                    args: args.map(|a| Cow::Owned(a.to_string())),
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
                    Value::Struct(d) => i64::try_from(d.len())
                        .map_err(|_| TemplateError::syntax("struct length exceeds i64::MAX"))?,
                    _ => {
                        return Err(TemplateError::syntax(
                            "len() requires a list, string, or struct",
                        ));
                    }
                };

                Ok(Cow::Owned(Value::Int(count)))
            }
            Self::Kind(path) => {
                let val = scope.resolve_path(path)?;
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
                    _ => Err(TemplateError::syntax(format!(
                        "kind() requires an enum value, got {}",
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
        }
    }

    /// Get the optional include resolver.
    #[cfg(feature = "std")]
    #[must_use]
    pub(crate) fn cache(&self) -> Option<&'a dyn crate::cache::IncludeResolver> {
        self.cache
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

    /// Push a loop binding onto the lightweight stack.
    ///
    /// Used by for-loops to avoid `HashMap` layer overhead. The binding is
    /// checked before `HashMap` layers in `resolve()`.
    #[inline]
    pub(crate) fn push_loop_binding(&mut self, key: &str, value: Value) {
        if self.active_loop_bindings < self.loop_bindings.len() {
            let slot = &mut self.loop_bindings[self.active_loop_bindings];
            // Skip key update if it already matches (common in for-loops
            // where the binding name is the same every iteration).
            if slot.0 != key {
                slot.0.clear();
                slot.0.push_str(key);
            }
            slot.1 = value;
            slot.2 = None;
        } else {
            self.loop_bindings.push((key.to_string(), value, None));
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
        self.consts_stack.push(Arc::clone(consts));
        self.imported_consts_stack.push(Arc::clone(imported_consts));
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
        use crate::consts::{FN_HAS, FN_IDX, FN_KIND, FN_LEN};
        let (func_name, arg) = parse_function_call(expr)?;
        match func_name {
            FN_IDX => self.call_idx(arg),
            FN_LEN => Some(self.call_len(arg)),
            FN_KIND => Some(self.call_kind(arg)),
            FN_HAS => Some(self.call_has(arg)),
            _ => None,
        }
    }

    /// Evaluate `idx(binding)` — returns the current loop index.
    fn call_idx(&self, arg: &str) -> Option<Result<Value, TemplateError>> {
        let meta = self.get_loop_meta(arg)?;
        Some(Ok(Value::Int(meta.index)))
    }

    /// Evaluate `len(path)` — returns the length of a list, string, or dict.
    fn call_len(&self, arg: &str) -> Result<Value, TemplateError> {
        let val = self.resolve_path_str(arg)?;
        let count = match val {
            // `.len()` cannot exceed `isize::MAX`, which always fits in `i64`.
            Value::List(l) => i64::try_from(l.len()).expect("len <= isize::MAX < i64::MAX"),
            Value::Str(s) => i64::try_from(s.len()).expect("len <= isize::MAX < i64::MAX"),
            Value::Struct(d) => i64::try_from(d.len()).expect("len <= isize::MAX < i64::MAX"),
            _ => {
                return Err(TemplateError::syntax(format!(
                    "len() requires a list, string, or struct, got {}",
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
            _ => Err(TemplateError::syntax(format!(
                "kind() requires an enum value, got {}",
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
    /// An option value is `Some` if it is:
    /// - A `Struct` with `__kind__ = "Some"` (struct variant representation), or
    /// - Any value that is NOT `Str("None")` (unit variant representation).
    ///
    /// This means a non-option enum `Str("Active")` would also return `true`,
    /// which is acceptable since `has()` should only be used on `option<T>` types.
    pub(crate) fn is_option_some(val: &Value) -> bool {
        use crate::consts::{ENUM_TAG_KEY, OPTION_NONE, OPTION_SOME};
        match val {
            // Struct variant: check __kind__ tag.
            Value::Struct(d) => {
                if let Some(Value::Str(tag)) = d.get(ENUM_TAG_KEY) {
                    tag == OPTION_SOME
                } else {
                    // Struct without tag — not an option, treat as truthy.
                    true
                }
            }
            // Unit variant: "None" → false, anything else → true.
            Value::Str(s) => s != OPTION_NONE,
            // Other value types: treat as present.
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
        // Fast path: no consts and no layers — go straight to loop bindings + context.
        if self.consts_stack.is_empty() && self.active_len == 0 {
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

        let root_key = &path.parts[0];
        let root = self
            .resolve(root_key)
            .ok_or_else(|| TemplateError::UndefinedVariable(root_key.clone()))?;

        let mut current = root;
        for part in &path.parts[1..] {
            current = current.get_field(part).ok_or_else(|| {
                TemplateError::UndefinedVariable(format!(
                    "field '{part}' not found on {}",
                    current.type_name()
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
        for part in parts {
            let part = part.trim();
            current = current.get_field(part).ok_or_else(|| {
                TemplateError::UndefinedVariable(format!(
                    "field '{part}' not found on {}",
                    current.type_name()
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context() -> Context {
        let mut ctx = Context::new();
        ctx.set("name", "Alice");
        ctx.set("count", 3_i64);
        ctx
    }

    // -- simple resolution --

    #[test]
    fn resolve_from_context() {
        let ctx = make_context();
        let scope = Scope::new(&ctx);
        assert_eq!(scope.resolve("name"), Some(&Value::Str("Alice".into())));
        assert_eq!(scope.resolve("count"), Some(&Value::Int(3)));
    }

    #[test]
    fn resolve_missing_returns_none() {
        let ctx = Context::new();
        let scope = Scope::new(&ctx);
        assert_eq!(scope.resolve("nope"), None);
    }

    // -- layered resolution --

    #[test]
    fn resolve_from_pushed_layer() {
        let ctx = make_context();
        let mut scope = Scope::new(&ctx);
        let layer = scope.push_layer();
        layer.insert("index".into(), Value::Int(1));
        layer.insert("item".into(), Value::Str("task-a".into()));

        assert_eq!(scope.resolve("index"), Some(&Value::Int(1)));
        assert_eq!(scope.resolve("item"), Some(&Value::Str("task-a".into())));
        // Context values still accessible.
        assert_eq!(scope.resolve("name"), Some(&Value::Str("Alice".into())));
    }

    #[test]
    fn pop_layer_restores_previous() {
        let ctx = make_context();
        let mut scope = Scope::new(&ctx);
        let layer = scope.push_layer();
        layer.insert("name".into(), Value::Str("shadowed".into()));
        assert_eq!(scope.resolve("name"), Some(&Value::Str("shadowed".into())));

        scope.pop_layer();
        assert_eq!(scope.resolve("name"), Some(&Value::Str("Alice".into())));
    }

    // -- shadowing --

    #[test]
    fn inner_layer_shadows_outer() {
        let ctx = make_context();
        let mut scope = Scope::new(&ctx);

        let layer1 = scope.push_layer();
        layer1.insert("x".into(), Value::Int(10));

        let layer2 = scope.push_layer();
        layer2.insert("x".into(), Value::Int(20));

        assert_eq!(scope.resolve("x"), Some(&Value::Int(20)));

        scope.pop_layer();
        assert_eq!(scope.resolve("x"), Some(&Value::Int(10)));

        scope.pop_layer();
        assert_eq!(scope.resolve("x"), None);
    }

    // -- dotted path resolution --

    #[test]
    fn resolve_path_simple() {
        let ctx = make_context();
        let scope = Scope::new(&ctx);
        let val = scope.resolve_path_str("name").unwrap();
        assert_eq!(val, &Value::Str("Alice".into()));
    }

    #[test]
    fn resolve_path_dotted() {
        let mut ctx = Context::new();
        let inner = Value::Struct(Arc::new(
            [("label".into(), Value::Str("important".into()))]
                .into_iter()
                .collect(),
        ));
        ctx.set("task", inner);

        let scope = Scope::new(&ctx);
        let val = scope.resolve_path_str("task.label").unwrap();
        assert_eq!(val, &Value::Str("important".into()));
    }

    #[test]
    fn resolve_path_deeply_nested() {
        let mut ctx = Context::new();
        let deep = Value::Struct(Arc::new(
            [(
                "a".into(),
                Value::Struct(Arc::new(
                    [(
                        "b".into(),
                        Value::Struct(Arc::new(
                            [("c".into(), Value::Int(42))].into_iter().collect(),
                        )),
                    )]
                    .into_iter()
                    .collect(),
                )),
            )]
            .into_iter()
            .collect(),
        ));
        ctx.set("root", deep);

        let scope = Scope::new(&ctx);
        assert_eq!(
            scope.resolve_path_str("root.a.b.c").unwrap(),
            &Value::Int(42)
        );
    }

    #[test]
    fn resolve_path_missing_root() {
        let ctx = Context::new();
        let scope = Scope::new(&ctx);
        let err = scope.resolve_path_str("absent").unwrap_err();
        assert!(matches!(err, TemplateError::UndefinedVariable(_)));
    }

    #[test]
    fn resolve_path_missing_field() {
        let mut ctx = Context::new();
        ctx.set(
            "item",
            Value::Struct(Arc::new(
                [("name".into(), Value::Str("x".into()))]
                    .into_iter()
                    .collect(),
            )),
        );
        let scope = Scope::new(&ctx);
        let err = scope.resolve_path_str("item.missing").unwrap_err();
        assert!(matches!(err, TemplateError::UndefinedVariable(_)));
    }

    #[test]
    fn resolve_path_field_on_non_dict() {
        let mut ctx = Context::new();
        ctx.set("val", 10_i64);
        let scope = Scope::new(&ctx);
        let err = scope.resolve_path_str("val.field").unwrap_err();
        assert!(matches!(err, TemplateError::UndefinedVariable(_)));
    }

    // -- dotted path in layers --

    #[test]
    fn resolve_path_through_layer() {
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        let layer = scope.push_layer();
        layer.insert(
            "item".into(),
            Value::Struct(Arc::new(
                [("name".into(), Value::Str("from-layer".into()))]
                    .into_iter()
                    .collect(),
            )),
        );

        let val = scope.resolve_path_str("item.name").unwrap();
        assert_eq!(val, &Value::Str("from-layer".into()));
    }

    #[test]
    fn test_layer_allocation_reuse() {
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);

        // Initially empty
        assert_eq!(scope.layers.len(), 0);
        assert_eq!(scope.active_len, 0);

        // Push 1
        {
            let layer = scope.push_layer();
            layer.insert("k1".into(), Value::Int(100));
        }
        assert_eq!(scope.layers.len(), 1);
        assert_eq!(scope.active_len, 1);
        assert_eq!(scope.resolve("k1"), Some(&Value::Int(100)));

        // Pop 1
        scope.pop_layer();
        assert_eq!(scope.layers.len(), 1); // Allocation kept
        assert_eq!(scope.active_len, 0);
        assert_eq!(scope.resolve("k1"), None); // k1 should not resolve because active_len is 0

        // Push again - should reuse
        {
            let layer = scope.push_layer();
            // Verify it was cleared! It shouldn't contain "k1" anymore.
            assert!(layer.is_empty());
            layer.insert("k2".into(), Value::Int(200));
        }
        assert_eq!(scope.layers.len(), 1); // Still 1! Reused!
        assert_eq!(scope.active_len, 1);
        assert_eq!(scope.resolve("k1"), None);
        assert_eq!(scope.resolve("k2"), Some(&Value::Int(200)));
    }

    // -- kind() function tests --

    #[test]
    fn kind_extracts_enum_variant_name() {
        let tmpl = crate::Template::from_source(
            "---\nparams: [outcome = struct<evidence = str>]\n---\n{{ kind(outcome) }}",
        )
        .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set(
            "outcome",
            Value::Struct(Arc::new(HashMap::from([
                (
                    crate::consts::ENUM_TAG_KEY.into(),
                    Value::Str("Confirmed".into()),
                ),
                ("evidence".into(), Value::Str("confirmed finding".into())),
            ]))),
        );
        assert_eq!(tmpl.render(&ctx).unwrap(), "Confirmed");
    }

    #[test]
    fn kind_rejects_non_dict() {
        let tmpl =
            crate::Template::from_source("---\nparams: [count = int]\n---\n{{ kind(count) }}")
                .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set("count", 42);
        let err = tmpl.render(&ctx).unwrap_err();
        assert!(
            err.to_string().contains("enum"),
            "should mention enum requirement: {err}"
        );
    }

    #[test]
    fn kind_rejects_dict_without_variant_tag() {
        let tmpl = crate::Template::from_source(
            "---\nparams: [data = struct<name = str>]\n---\n{{ kind(data) }}",
        )
        .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set(
            "data",
            Value::Struct(Arc::new(HashMap::from([(
                "name".into(),
                Value::Str("x".into()),
            )]))),
        );
        let err = tmpl.render(&ctx).unwrap_err();
        assert!(
            err.to_string().contains("enum"),
            "should mention enum requirement: {err}"
        );
    }

    #[test]
    fn kind_key_not_accessible_via_dot_path() {
        // The internal __kind__ key must not be accessible as {{ outcome.__kind__ }}.
        let tmpl = crate::Template::from_source(
            "---\nparams: [outcome = struct<evidence = str>]\n---\n{{ outcome.__kind__ }}",
        )
        .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set(
            "outcome",
            Value::Struct(Arc::new(HashMap::from([
                (
                    crate::consts::ENUM_TAG_KEY.into(),
                    Value::Str("Confirmed".into()),
                ),
                ("evidence".into(), Value::Str("found it".into())),
            ]))),
        );
        let err = tmpl.render(&ctx).unwrap_err();
        assert!(
            err.to_string().contains("not found") || err.to_string().contains("undefined"),
            "__kind__ should not be accessible from templates: {err}"
        );
    }

    #[test]
    fn user_field_named_tag_does_not_collide() {
        // A user field named "tag" must not collide with the internal __kind__ key.
        let tmpl = crate::Template::from_source(
            "---\nparams: [entry = struct<tag = str>]\n---\n{{ kind(entry) }}: {{ entry.tag }}",
        )
        .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set(
            "entry",
            Value::Struct(Arc::new(HashMap::from([
                (
                    crate::consts::ENUM_TAG_KEY.into(),
                    Value::Str("Woche".into()),
                ),
                ("tag".into(), Value::Str("Montag".into())),
            ]))),
        );
        assert_eq!(tmpl.render(&ctx).unwrap(), "Woche: Montag");
    }

    // -- parse_function_call edge cases --

    #[test]
    fn parse_function_call_valid() {
        let result = parse_function_call("idx(item)");
        assert_eq!(result, Some(("idx", "item")));
    }

    #[test]
    fn parse_function_call_empty_func_returns_none() {
        // `(arg)` — empty function name.
        assert_eq!(parse_function_call("(arg)"), None);
    }

    #[test]
    fn parse_function_call_empty_arg_returns_none() {
        // `func()` — empty argument.
        assert_eq!(parse_function_call("func()"), None);
    }

    #[test]
    fn parse_function_call_no_parens_returns_none() {
        assert_eq!(parse_function_call("just_a_name"), None);
    }

    #[test]
    fn parse_function_call_dotted_name_returns_none() {
        // Dotted names are not valid function identifiers.
        assert_eq!(parse_function_call("foo.bar(x)"), None);
    }

    // -- ConditionOperand compile & resolve --

    #[test]
    fn resolve_value_or_literal_string_literal() {
        let ctx = Context::new();
        let scope = Scope::new(&ctx);
        let operand = ConditionOperand::compile("\"hello\"").unwrap();
        let val = operand.resolve(&scope).unwrap();
        assert_eq!(*val, Value::Str("hello".into()));
    }

    #[test]
    fn resolve_value_or_literal_bool_true() {
        let ctx = Context::new();
        let scope = Scope::new(&ctx);
        let operand = ConditionOperand::compile("true").unwrap();
        assert_eq!(*operand.resolve(&scope).unwrap(), Value::Bool(true));
    }

    #[test]
    fn resolve_value_or_literal_integer() {
        let ctx = Context::new();
        let scope = Scope::new(&ctx);
        let operand = ConditionOperand::compile("42").unwrap();
        assert_eq!(*operand.resolve(&scope).unwrap(), Value::Int(42));
    }

    #[test]
    fn resolve_value_or_literal_float() {
        let ctx = Context::new();
        let scope = Scope::new(&ctx);
        let operand = ConditionOperand::compile("2.78").unwrap();
        assert_eq!(*operand.resolve(&scope).unwrap(), Value::Float(2.78));
    }

    #[test]
    fn resolve_value_or_literal_empty_token_returns_error() {
        let err = ConditionOperand::compile("").unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- include depth tracking --

    #[test]
    fn enter_include_enforces_max_depth() {
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx).with_max_include_depth(2);
        scope.enter_include().unwrap();
        scope.enter_include().unwrap();
        // Third should exceed depth of 2.
        let err = scope.enter_include().unwrap_err();
        assert!(err.to_string().contains("maximum include depth"));
    }

    #[test]
    fn exit_include_decrements_and_allows_reentry() {
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx).with_max_include_depth(1);
        scope.enter_include().unwrap();
        scope.exit_include();
        // After exiting, re-entering should succeed.
        scope.enter_include().unwrap();
    }

    // -- pop_layer on empty scope --

    #[test]
    fn pop_layer_on_empty_scope_is_noop() {
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);
        // Should not panic.
        scope.pop_layer();
        scope.pop_layer();
        assert_eq!(scope.resolve("anything"), None);
    }

    // -- constants resolution --

    #[test]
    fn consts_take_priority_over_context() {
        let mut ctx = Context::new();
        ctx.set("x", "from_ctx");
        let mut scope = Scope::new(&ctx);
        let consts = Arc::new(HashMap::from([(
            "x".into(),
            Value::Str("from_const".into()),
        )]));
        let imported = Arc::new(HashMap::new());
        scope.set_consts(&consts, &imported);
        // Constants should shadow context values.
        assert_eq!(scope.resolve("x"), Some(&Value::Str("from_const".into())));
    }

    #[test]
    fn push_pop_consts_restores_context_value() {
        let mut ctx = Context::new();
        ctx.set("y", "original");
        let mut scope = Scope::new(&ctx);
        scope.push_consts(
            HashMap::from([("y".into(), Value::Str("overridden".into()))]),
            HashMap::new(),
        );
        assert_eq!(scope.resolve("y"), Some(&Value::Str("overridden".into())));
        scope.pop_consts();
        assert_eq!(scope.resolve("y"), Some(&Value::Str("original".into())));
    }
}
