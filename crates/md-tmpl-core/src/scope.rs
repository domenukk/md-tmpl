//! Scoped variable resolution for rendering.

mod expr;
#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;

use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

pub(crate) use expr::parse_function_call;
pub use expr::{CompiledExpr, CompiledPath, ConditionOperand};

use crate::{
    compat::HashMap, compiled::CompiledInlineTemplate, context::Context, error::TemplateError,
    value::Value,
};

/// Maximum nesting depth for template includes.
///
/// Prevents infinite recursion from circular includes.
pub(crate) const MAX_INCLUDE_DEPTH: usize = 16;

/// Empty inline template map used as default when no inline templates exist.
static EMPTY_INLINE_TEMPLATES: crate::compat::LazyLock<HashMap<String, CompiledInlineTemplate>> =
    crate::compat::LazyLock::new(HashMap::new);

/// Maximum inline byte length for loop binding variable names before falling back to heap `String`.
const INLINE_KEY_CAP: usize = 23;

/// Number of nested `{% for %}` loop bindings stored inline on the `Scope` stack without heap allocation.
const INLINE_LOOP_SLOTS: usize = 4;

/// Compact loop variable name stored inline on the stack for identifiers up to 23 bytes.
#[derive(Debug, Clone)]
enum LoopKey {
    Inline { buf: [u8; INLINE_KEY_CAP], len: u8 },
    Heap(String),
}

impl LoopKey {
    const EMPTY: Self = Self::Inline {
        buf: [0u8; INLINE_KEY_CAP],
        len: 0,
    };

    #[inline]
    fn set(&mut self, s: &str) {
        let bytes = s.as_bytes();
        if let Ok(len) = u8::try_from(bytes.len())
            && bytes.len() <= INLINE_KEY_CAP
        {
            if let Self::Inline { buf, len: cur_len } = self {
                if usize::from(*cur_len) != bytes.len() || &buf[..bytes.len()] != bytes {
                    buf[..bytes.len()].copy_from_slice(bytes);
                    *cur_len = len;
                }
            } else {
                let mut buf = [0u8; INLINE_KEY_CAP];
                buf[..bytes.len()].copy_from_slice(bytes);
                *self = Self::Inline { buf, len };
            }
        } else if let Self::Heap(existing) = self {
            if existing != s {
                existing.clear();
                existing.push_str(s);
            }
        } else {
            *self = Self::Heap(s.to_string());
        }
    }

    #[inline]
    fn eq_str(&self, s: &str) -> bool {
        match self {
            Self::Inline { buf, len } => {
                let l = usize::from(*len);
                l == s.len() && &buf[..l] == s.as_bytes()
            }
            Self::Heap(h) => h == s,
        }
    }
}

/// Loop metadata for a for-loop binding.
///
/// Stored per-binding in the scope so that `idx()` works
/// correctly even from deeply nested loops.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoopMeta {
    /// 0-based iteration index.
    pub index: i64,
    /// Total number of items in the iterated collection.
    pub len: usize,
}

#[derive(Debug, Clone)]
struct LoopSlot {
    key: LoopKey,
    value: Value,
    meta: Option<LoopMeta>,
    val_index0: Value,
    val_index: Value,
    val_len: Value,
}

impl LoopSlot {
    const EMPTY: Self = Self {
        key: LoopKey::EMPTY,
        value: Value::None,
        meta: None,
        val_index0: Value::Int(0),
        val_index: Value::Int(1),
        val_len: Value::Int(0),
    };

    #[inline]
    fn set_value(&mut self, value: &Value) {
        match (&mut self.value, value) {
            (Value::Str(old), Value::Str(new)) if old.capacity() > 0 => {
                old.clear();
                old.push_str(new);
            }
            _ => self.value = value.clone(),
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
    /// Fallback loop values for HashMap-based include `for_each` iterations.
    fallback_loop_vals: Option<(LoopMeta, Value, Value, Value)>,
    active_len: usize,
    /// Number of active loop bindings across `inline_loops` and `overflow_loops`.
    active_loop_bindings: usize,
    /// Inline stack-allocated loop bindings for the first 4 nesting levels (zero heap allocation).
    inline_loops: [LoopSlot; INLINE_LOOP_SLOTS],
    /// Overflow loop bindings when loop nesting exceeds `INLINE_LOOP_SLOTS`.
    overflow_loops: Vec<LoopSlot>,
    include_depth: usize,
    max_include_depth: usize,
    /// Pre-compiled inline template definitions (borrowed from top-level `Template`).
    inline_templates: &'a HashMap<String, CompiledInlineTemplate>,
    /// Stack of owned inline templates from included files. Each file pushes its
    /// own `{% tmpl %}` definitions when entered, and pops them when exited.
    /// `get_inline_template` checks this stack (innermost first) before
    /// falling back to the top-level `inline_templates`.
    inline_template_stack: Vec<HashMap<String, CompiledInlineTemplate>>,
    /// Optional include resolver for cached include resolution.
    #[cfg(feature = "std")]
    cache: Option<&'a dyn crate::cache::IncludeResolver>,
    /// Borrowed root constants from the top-level `Template`.
    root_consts: Option<&'a HashMap<String, Value>>,
    /// Borrowed root imported constants from the top-level `Template`.
    root_imported_consts: Option<&'a HashMap<String, Value>>,
    /// Stack of local constants from included templates.
    consts_stack: Vec<Arc<HashMap<String, Value>>>,
    /// Stack of imported constants keyed by `stem.NAME` from included templates.
    imported_consts_stack: Vec<Arc<HashMap<String, Value>>>,
    /// Borrowed parameter declarations from the root `Template`.
    root_declarations: &'a [crate::types::VarDecl],
    /// Stack of parameter declarations from included templates.
    declarations_stack: Vec<Arc<[crate::types::VarDecl]>>,
    /// Fast flag: `true` if any active declaration in `root_declarations` or
    /// `declarations_stack` contains an `option(T)` type.
    has_options: bool,
    /// Option params that have been narrowed to Some (unwrapped) in an
    /// enclosing match/if-has arm.  `is_option_path` returns `false` for
    /// narrowed params so that `kind()` and inner `match` blocks see the
    /// unwrapped enum value.
    narrowed_options: Vec<String>,
    /// Borrowed compile-time environment values from the root `Template`.
    #[cfg(feature = "std")]
    root_compile_env: &'a [(String, Value)],
}

impl<'a> Scope<'a> {
    /// Create a new scope backed by the given context.
    #[must_use]
    pub fn new(ctx: &'a Context) -> Self {
        Self {
            ctx,
            layers: Vec::new(),
            loop_metas: Vec::new(),
            fallback_loop_vals: None,
            active_len: 0,
            active_loop_bindings: 0,
            inline_loops: [LoopSlot::EMPTY; INLINE_LOOP_SLOTS],
            overflow_loops: Vec::new(),
            include_depth: 0,
            max_include_depth: MAX_INCLUDE_DEPTH,
            inline_templates: &EMPTY_INLINE_TEMPLATES,
            inline_template_stack: Vec::new(),
            #[cfg(feature = "std")]
            cache: None,
            root_consts: None,
            root_imported_consts: None,
            consts_stack: Vec::new(),
            imported_consts_stack: Vec::new(),
            root_declarations: &[],
            declarations_stack: Vec::new(),
            has_options: false,
            narrowed_options: Vec::new(),
            #[cfg(feature = "std")]
            root_compile_env: &[],
        }
    }

    /// Create a new scope with an include resolver for faster include resolution.
    ///
    /// Equivalent to [`Scope::new`] with the include resolver attached — the
    /// two constructors share all other defaults via struct-update syntax so
    /// they cannot drift apart.
    #[cfg(feature = "std")]
    pub(crate) fn with_cache(
        ctx: &'a Context,
        cache: &'a dyn crate::cache::IncludeResolver,
    ) -> Self {
        Self {
            cache: Some(cache),
            ..Self::new(ctx)
        }
    }

    /// Get the optional include resolver.
    #[cfg(feature = "std")]
    #[must_use]
    pub(crate) fn cache(&self) -> Option<&'a dyn crate::cache::IncludeResolver> {
        self.cache
    }

    /// Borrow compile-time environment values directly from the root `Template` without atomic cloning.
    #[cfg(feature = "std")]
    #[inline]
    pub(crate) fn set_compile_env_slice(&mut self, env: &'a [(String, Value)]) {
        self.root_compile_env = env;
    }

    /// Get the compile-time environment values.
    #[cfg(feature = "std")]
    #[must_use]
    pub(crate) fn compile_env(&self) -> &[(String, Value)] {
        self.root_compile_env
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
            if self.active_len == 0 {
                self.fallback_loop_vals = None;
            }
        }
    }

    #[inline]
    fn loop_slot_mut(&mut self, idx: usize) -> &mut LoopSlot {
        if idx < INLINE_LOOP_SLOTS {
            &mut self.inline_loops[idx]
        } else {
            &mut self.overflow_loops[idx - INLINE_LOOP_SLOTS]
        }
    }

    #[inline]
    fn loop_slot_ref(&self, idx: usize) -> &LoopSlot {
        if idx < INLINE_LOOP_SLOTS {
            &self.inline_loops[idx]
        } else {
            &self.overflow_loops[idx - INLINE_LOOP_SLOTS]
        }
    }

    /// Acquire a loop binding slot for `key` once at the start of a `{% for %}` loop.
    #[inline]
    pub(crate) fn begin_loop(&mut self, key: &str) -> usize {
        let idx = self.active_loop_bindings;
        if idx < INLINE_LOOP_SLOTS {
            self.inline_loops[idx].key.set(key);
        } else {
            let overflow_idx = idx - INLINE_LOOP_SLOTS;
            if overflow_idx < self.overflow_loops.len() {
                self.overflow_loops[overflow_idx].key.set(key);
            } else {
                let mut slot = LoopSlot::EMPTY;
                slot.key.set(key);
                self.overflow_loops.push(slot);
            }
        }
        self.active_loop_bindings = idx + 1;
        idx
    }

    /// Update the value and iteration metadata of an active loop slot in-place.
    #[inline]
    pub(crate) fn update_loop_slot(
        &mut self,
        slot_idx: usize,
        value: &Value,
        index: i64,
        len: usize,
    ) {
        let slot = self.loop_slot_mut(slot_idx);
        slot.set_value(value);
        slot.meta = Some(LoopMeta { index, len });
        slot.val_index0 = Value::Int(index);
        slot.val_index = Value::Int(index + 1);
        slot.val_len = Value::Int(i64::try_from(len).expect("loop length fits i64"));
    }

    /// Push a loop binding from a reference, reusing the existing string
    /// allocation when both old and new values are strings.
    #[cfg(test)]
    #[inline]
    pub(crate) fn push_loop_binding(&mut self, key: &str, value: &Value) {
        let idx = self.begin_loop(key);
        let slot = self.loop_slot_mut(idx);
        slot.set_value(value);
        slot.meta = None;
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
        let mut i = self.active_loop_bindings;
        while i > 0 {
            i -= 1;
            let slot = self.loop_slot_mut(i);
            if slot.key.eq_str(binding) {
                slot.meta = Some(meta);
                slot.val_index0 = Value::Int(meta.index);
                slot.val_index = Value::Int(meta.index + 1);
                slot.val_len = Value::Int(i64::try_from(meta.len).expect("loop length fits i64"));
                return;
            }
        }
        // Fall back to HashMap-based layers (used by includes with for_each).
        if self.active_len > 0 {
            self.loop_metas[self.active_len - 1].insert(binding.to_string(), meta);
            self.fallback_loop_vals = Some((
                meta,
                Value::Int(meta.index),
                Value::Int(meta.index + 1),
                Value::Int(i64::try_from(meta.len).expect("loop length fits i64")),
            ));
        }
    }

    /// Look up loop metadata for a binding name.
    ///
    /// Searches layers top-to-bottom, so the innermost loop with that binding
    /// wins — but outer bindings with different names remain accessible.
    pub(crate) fn get_loop_meta(&self, binding: &str) -> Option<&LoopMeta> {
        let mut i = self.active_loop_bindings;
        while i > 0 {
            i -= 1;
            let slot = self.loop_slot_ref(i);
            if slot.key.eq_str(binding) {
                return slot.meta.as_ref();
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

    pub(crate) fn get_active_loop_slot(&self) -> Option<(&LoopMeta, &Value, &Value, &Value)> {
        let mut i = self.active_loop_bindings;
        while i > 0 {
            i -= 1;
            let slot = self.loop_slot_ref(i);
            if let Some(ref meta) = slot.meta {
                return Some((meta, &slot.val_index0, &slot.val_index, &slot.val_len));
            }
        }
        None
    }

    pub(crate) fn resolve_loop_prop(&self, prop: &str) -> Result<&Value, TemplateError> {
        static VAL_TRUE: Value = Value::Bool(true);
        static VAL_FALSE: Value = Value::Bool(false);

        if let Some((meta, val_index0, val_index, val_len)) = self.get_active_loop_slot() {
            return match prop {
                crate::consts::LOOP_FIRST => {
                    if meta.index == 0 {
                        Ok(&VAL_TRUE)
                    } else {
                        Ok(&VAL_FALSE)
                    }
                }
                crate::consts::LOOP_LAST => {
                    let is_last = meta.len > 0
                        && meta.index + 1 == i64::try_from(meta.len).expect("len fits i64");
                    if is_last {
                        Ok(&VAL_TRUE)
                    } else {
                        Ok(&VAL_FALSE)
                    }
                }
                crate::consts::LOOP_INDEX0 => Ok(val_index0),
                crate::consts::LOOP_INDEX => Ok(val_index),
                crate::consts::LOOP_LENGTH | crate::consts::LOOP_LEN => Ok(val_len),
                _ => Err(TemplateError::UndefinedVariable(alloc::format!(
                    "field '{prop}' not found on loop"
                ))),
            };
        }

        if let Some((meta, val_index0, val_index, val_len)) = &self.fallback_loop_vals {
            return match prop {
                crate::consts::LOOP_FIRST => {
                    if meta.index == 0 {
                        Ok(&VAL_TRUE)
                    } else {
                        Ok(&VAL_FALSE)
                    }
                }
                crate::consts::LOOP_LAST => {
                    let is_last = meta.len > 0
                        && meta.index + 1 == i64::try_from(meta.len).expect("len fits i64");
                    if is_last {
                        Ok(&VAL_TRUE)
                    } else {
                        Ok(&VAL_FALSE)
                    }
                }
                crate::consts::LOOP_INDEX0 => Ok(val_index0),
                crate::consts::LOOP_INDEX => Ok(val_index),
                crate::consts::LOOP_LENGTH | crate::consts::LOOP_LEN => Ok(val_len),
                _ => Err(TemplateError::UndefinedVariable(alloc::format!(
                    "field '{prop}' not found on loop"
                ))),
            };
        }

        Err(TemplateError::UndefinedVariable(
            "loop metadata is only available inside a for loop".into(),
        ))
    }

    /// Set the inline template definitions for this scope.
    pub fn set_inline_templates(&mut self, templates: &'a HashMap<String, CompiledInlineTemplate>) {
        self.inline_templates = templates;
    }

    /// Borrow root constants directly from the top-level `Template` without heap/atomic overhead.
    #[inline]
    pub fn set_root_consts(
        &mut self,
        consts: &'a HashMap<String, Value>,
        imported_consts: &'a HashMap<String, Value>,
    ) {
        if !consts.is_empty() {
            self.root_consts = Some(consts);
        }
        if !imported_consts.is_empty() {
            self.root_imported_consts = Some(imported_consts);
        }
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

    /// Set parameter declarations for this scope (borrowing directly without heap allocation).
    #[must_use]
    pub fn with_declarations(mut self, decls: &'a [crate::types::VarDecl]) -> Self {
        self.root_declarations = decls;
        self.has_options = decls.iter().any(|d| d.var_type.contains_option());
        self
    }

    /// Set parameter declarations with a precomputed `has_options` flag.
    #[inline]
    #[must_use]
    pub fn with_root_declarations(
        mut self,
        decls: &'a [crate::types::VarDecl],
        has_options: bool,
    ) -> Self {
        self.root_declarations = decls;
        self.has_options = has_options;
        self
    }

    /// Push parameter declarations onto the stack.
    pub(crate) fn push_declarations(&mut self, decls: &[crate::types::VarDecl]) {
        if !decls.is_empty() {
            if !self.has_options && decls.iter().any(|d| d.var_type.contains_option()) {
                self.has_options = true;
            }
            self.declarations_stack.push(Arc::from(decls));
        }
    }

    /// Pop parameter declarations from the stack.
    pub(crate) fn pop_declarations(&mut self, decls: &[crate::types::VarDecl]) {
        if !decls.is_empty() {
            self.declarations_stack.pop();
        }
    }

    fn resolve_in_decl_slice<'d>(
        decls: &'d [crate::types::VarDecl],
        root: &str,
        arg: &str,
    ) -> Option<&'d crate::types::VarType> {
        let decl = decls.iter().find(|d| d.name == root)?;
        let mut current_type = &decl.var_type;
        if arg == root {
            return Some(current_type);
        }
        for part in arg.split(crate::consts::PATH_SEP).skip(1) {
            match current_type {
                crate::types::VarType::Struct(fields) | crate::types::VarType::List(fields) => {
                    if let Some(f) = fields.iter().find(|d| d.name == part) {
                        current_type = &f.var_type;
                    } else {
                        return None;
                    }
                }
                crate::types::VarType::Option(inner) => {
                    current_type = inner;
                }
                _ => return None,
            }
        }
        Some(current_type)
    }

    /// Look up the declared `VarType` for a dotted variable path from the declaration stack.
    pub(crate) fn resolve_declared_type(&self, arg: &str) -> Option<&crate::types::VarType> {
        let root = arg.split(crate::consts::PATH_SEP).next().unwrap_or(arg);
        for decls in self.declarations_stack.iter().rev() {
            if let Some(ty) = Self::resolve_in_decl_slice(decls, root, arg) {
                return Some(ty);
            }
        }
        Self::resolve_in_decl_slice(self.root_declarations, root, arg)
    }

    /// Check if a dotted variable path resolves to an option type in declared parameters.
    #[inline]
    pub(crate) fn is_option_path(&self, arg: &str) -> bool {
        if !self.has_options {
            return false;
        }
        // If this path has been narrowed (unwrapped via case Some / if has()),
        // it is no longer an option for kind()/match purposes.
        if self.narrowed_options.iter().any(|s| s == arg) {
            return false;
        }
        self.resolve_declared_type(arg)
            .is_some_and(crate::types::VarType::is_option)
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
            Value::Str(s) => {
                if let Some(decl_ty) = self.resolve_declared_type(arg) {
                    let is_enum_or_option_enum = match decl_ty {
                        crate::types::VarType::Enum(_) => true,
                        crate::types::VarType::Option(inner) => {
                            matches!(inner.as_ref(), crate::types::VarType::Enum(_))
                        }
                        _ => false,
                    };
                    if !is_enum_or_option_enum {
                        return Err(TemplateError::syntax(format!(
                            "kind() requires an enum or option value, got {decl_ty} on '{arg}'"
                        )));
                    }
                }
                Ok(Value::Str(s.clone()))
            }
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
        if let Some(decl_ty) = self.resolve_declared_type(arg)
            && !decl_ty.is_option()
        {
            return Err(TemplateError::syntax(format!(
                "has() requires an option value, got {decl_ty} on '{arg}'"
            )));
        }
        let val = self.resolve_path_str(arg)?;
        Ok(Value::Bool(Self::is_option_some(val)))
    }

    /// Check if an option value is present (`Some`).
    pub(crate) fn is_option_some(val: &Value) -> bool {
        !matches!(val, Value::None)
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

    #[inline]
    fn resolve_loop_binding(&self, key: &str) -> Option<&Value> {
        let n = self.active_loop_bindings;
        if n <= INLINE_LOOP_SLOTS {
            let mut i = n;
            while i > 0 {
                i -= 1;
                let slot = &self.inline_loops[i];
                if slot.key.eq_str(key) {
                    return Some(&slot.value);
                }
            }
            None
        } else {
            let mut i = n;
            while i > 0 {
                i -= 1;
                let slot = self.loop_slot_ref(i);
                if slot.key.eq_str(key) {
                    return Some(&slot.value);
                }
            }
            None
        }
    }

    #[inline]
    fn has_any_imported_consts(&self) -> bool {
        self.root_imported_consts.is_some() || !self.imported_consts_stack.is_empty()
    }

    #[inline]
    fn get_imported_const(&self, stem_key: &str) -> Option<&Value> {
        for imported in self.imported_consts_stack.iter().rev() {
            if let Some(v) = imported.get(stem_key) {
                return Some(v);
            }
        }
        if let Some(root_imp) = self.root_imported_consts
            && let Some(v) = root_imp.get(stem_key)
        {
            return Some(v);
        }
        None
    }

    /// Resolve a simple (non-dotted) variable name.
    #[inline]
    #[must_use]
    pub fn resolve(&self, key: &str) -> Option<&Value> {
        // Fast path: no consts, no imported consts, and no layers — go straight to loop bindings + context.
        if self.root_consts.is_none()
            && self.root_imported_consts.is_none()
            && self.consts_stack.is_empty()
            && self.imported_consts_stack.is_empty()
            && self.active_len == 0
        {
            if self.active_loop_bindings > 0
                && let Some(v) = self.resolve_loop_binding(key)
            {
                return Some(v);
            }
            return self.ctx.get(key);
        }
        // 1. Local constants (strictly immutable, highest priority).
        for consts in self.consts_stack.iter().rev() {
            if let Some(v) = consts.get(key) {
                return Some(v);
            }
        }
        if let Some(root_c) = self.root_consts
            && let Some(v) = root_c.get(key)
        {
            return Some(v);
        }
        // 1b. Imported constants (type aliases, included template consts).
        if let Some(v) = self.get_imported_const(key) {
            return Some(v);
        }
        // 2. Loop bindings (lightweight stack, checked before HashMap layers).
        if let Some(v) = self.resolve_loop_binding(key) {
            return Some(v);
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

        // Loop metadata fast path (`loop.<prop>`).
        if path.parts[0] == crate::consts::LOOP {
            if path.parts.len() == 2 {
                return self.resolve_loop_prop(&path.parts[1]);
            }
            return Err(TemplateError::UndefinedVariable(alloc::format!(
                "field '{}' not found on loop",
                path.parts[1]
            )));
        }

        // Fast path for 2-part dotted paths (`obj.field`) when no imported consts or options exist.
        if path.parts.len() == 2 && !self.has_any_imported_consts() && !self.has_options {
            let root_key = &path.parts[0];
            let field_key = &path.parts[1];
            let root = self
                .resolve(root_key)
                .ok_or_else(|| TemplateError::UndefinedVariable(root_key.clone()))?;
            if let Some(val) = root.get_field_unchecked(field_key) {
                return Ok(val);
            }
            let available = root.field_names_hint();
            let hint = if available.is_empty() {
                String::new()
            } else {
                format!(". Available fields: {}", available.join(", "))
            };
            return Err(TemplateError::UndefinedVariable(format!(
                "field '{field_key}' not found on {} at path '{root_key}.{field_key}'{hint}",
                root.type_name(),
            )));
        }

        // 1. Check if it's an imported constant (stem.NAME[.field]*).
        if self.has_any_imported_consts() && path.parts.len() >= 2 {
            let p0 = &path.parts[0];
            let p1 = &path.parts[1];
            let needed = p0.len() + 1 + p1.len();
            let mut stack_buf = [0u8; 128];
            let stem_key: &str = if needed <= stack_buf.len() {
                stack_buf[..p0.len()].copy_from_slice(p0.as_bytes());
                stack_buf[p0.len()] = b'.';
                stack_buf[p0.len() + 1..needed].copy_from_slice(p1.as_bytes());
                core::str::from_utf8(&stack_buf[..needed]).unwrap_or(&path.raw)
            } else {
                &path.raw
            };

            if let Some(v) = self.get_imported_const(stem_key) {
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

        let root_key = &path.parts[0];
        if self.is_option_path(root_key) {
            return Err(TemplateError::syntax(format!(
                "cannot access field '{}' on option — unwrap with {{% if has({root_key}) %}} or {{% match {root_key} %}}",
                path.parts[1]
            )));
        }
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

        // Loop metadata fast path (e.g. `loop.first`, `loop.index`).
        if let Some(prop) = path.strip_prefix("loop.") {
            return self.resolve_loop_prop(prop.trim());
        }

        // 1. Check if it's an imported constant (stem.NAME[.field]*).
        if self.has_any_imported_consts() {
            let mut parts = path.split(crate::consts::PATH_SEP);
            let first = parts.next().unwrap_or("").trim();
            if let Some(second) = parts.next() {
                let stem_name = format!("{}.{}", first, second.trim());
                if let Some(v) = self.get_imported_const(&stem_name) {
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
