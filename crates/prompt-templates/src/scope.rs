//! Scoped variable resolution for rendering.

use std::collections::HashMap;

use crate::{
    compiled::CompiledInlineTemplate, context::Context, error::TemplateError, value::Value,
};

/// Maximum nesting depth for template includes.
///
/// Prevents infinite recursion from circular includes.
pub(crate) const MAX_INCLUDE_DEPTH: usize = 16;

/// Empty inline template map used as default when no inline templates exist.
static EMPTY_INLINE_TEMPLATES: std::sync::LazyLock<HashMap<String, CompiledInlineTemplate>> =
    std::sync::LazyLock::new(HashMap::new);

/// Loop metadata for a for-loop binding.
///
/// Stored per-binding in the scope so that `{{ idx(binding) }}` works
/// correctly even from deeply nested loops.
#[derive(Debug, Clone)]
pub(crate) struct LoopMeta {
    /// 0-based iteration index.
    pub index: i64,
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
    include_depth: usize,
    max_include_depth: usize,
    /// Pre-compiled inline template definitions (borrowed from `Template`).
    inline_templates: &'a HashMap<String, CompiledInlineTemplate>,
    /// Optional include resolver for cached include resolution.
    cache: Option<&'a dyn crate::cache::IncludeResolver>,
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
            include_depth: 0,
            max_include_depth: MAX_INCLUDE_DEPTH,
            inline_templates: &EMPTY_INLINE_TEMPLATES,
            cache: None,
        }
    }

    /// Create a new scope with an include resolver for faster include resolution.
    #[must_use]
    pub fn with_cache(ctx: &'a Context, cache: &'a dyn crate::cache::IncludeResolver) -> Self {
        Self {
            ctx,
            layers: Vec::new(),
            loop_metas: Vec::new(),
            active_len: 0,
            include_depth: 0,
            max_include_depth: MAX_INCLUDE_DEPTH,
            inline_templates: &EMPTY_INLINE_TEMPLATES,
            cache: Some(cache),
        }
    }

    /// Get the optional include resolver.
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

    /// Register loop metadata for a for-loop binding.
    ///
    /// Must be called after `push_layer` to associate metadata with the
    /// current layer's binding.
    pub(crate) fn set_loop_meta(&mut self, binding: &str, meta: LoopMeta) {
        if self.active_len > 0 {
            self.loop_metas[self.active_len - 1].insert(binding.to_string(), meta);
        }
    }

    /// Look up loop metadata for a binding name.
    ///
    /// Searches layers top-to-bottom, so the innermost loop with that binding
    /// wins — but outer bindings with different names remain accessible.
    pub(crate) fn get_loop_meta(&self, binding: &str) -> Option<&LoopMeta> {
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

    /// Look up a pre-compiled inline template by name.
    #[must_use]
    pub fn get_inline_template(&self, name: &str) -> Option<&CompiledInlineTemplate> {
        self.inline_templates.get(name)
    }

    /// Try to evaluate a function call expression like `idx(bug)` or `len(items)`.
    ///
    /// Returns `None` if the expression doesn't look like a function call,
    /// `Some(Ok(...))` on success, or `Some(Err(...))` on evaluation failure.
    pub(crate) fn try_call_function(&self, expr: &str) -> Option<Result<Value, TemplateError>> {
        use crate::consts::{FN_IDX, FN_LEN, FN_STR};
        let (func_name, arg) = parse_function_call(expr)?;
        match func_name {
            FN_IDX => self.call_idx(arg),
            FN_LEN => Some(self.call_len(arg)),
            FN_STR => Some(self.call_str(arg)),
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
        let val = self.resolve_path(arg)?;
        let count = match val {
            // `.len()` cannot exceed `isize::MAX`, which always fits in `i64`.
            Value::List(l) => i64::try_from(l.len()).expect("len <= isize::MAX < i64::MAX"),
            Value::Str(s) => i64::try_from(s.len()).expect("len <= isize::MAX < i64::MAX"),
            Value::Dict(d) => i64::try_from(d.len()).expect("len <= isize::MAX < i64::MAX"),
            _ => {
                return Err(TemplateError::syntax(format!(
                    "len() requires a list, string, or dict, got {}",
                    val.type_name()
                )));
            }
        };
        Ok(Value::Int(count))
    }

    /// Evaluate `str(path)` — converts a value to its string representation.
    fn call_str(&self, arg: &str) -> Result<Value, TemplateError> {
        use crate::consts::ENUM_TAG_KEY;
        let val = self.resolve_path(arg)?;
        let result = match val {
            Value::Str(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            // Enum unit variants stored as Dict with tag field:
            // str(outcome) → "Confirmed"
            Value::Dict(d) => {
                if let Some(Value::Str(tag)) = d.get(ENUM_TAG_KEY) {
                    tag.clone()
                } else {
                    return Err(TemplateError::syntax(
                        "str() cannot convert dict without 'tag' field",
                    ));
                }
            }
            Value::List(_) => {
                return Err(TemplateError::syntax(
                    "str() cannot convert a list to string \
                     (use join filter instead)",
                ));
            }
        };
        Ok(Value::Str(result))
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
    #[must_use]
    pub fn resolve(&self, key: &str) -> Option<&Value> {
        for layer in self.layers[..self.active_len].iter().rev() {
            if let Some(v) = layer.get(key) {
                return Some(v);
            }
        }
        self.ctx.get(key)
    }

    /// Resolve a dotted path like `item.name` or `item.nested.field`.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::UndefinedVariable`] if the root key or any
    /// intermediate field is not found.
    pub fn resolve_path(&self, path: &str) -> Result<&Value, TemplateError> {
        let mut parts = path.split('.');
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

    pub(crate) fn resolve_value_or_literal(&self, token: &str) -> Result<Value, TemplateError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(TemplateError::syntax(
                "empty token in expression".to_string(),
            ));
        }

        // 1. String literals
        if (token.starts_with('"') && token.ends_with('"'))
            || (token.starts_with('\'') && token.ends_with('\''))
        {
            return Ok(Value::Str(token[1..token.len() - 1].to_string()));
        }

        // 2. Boolean literals
        if token == crate::consts::LIT_TRUE {
            return Ok(Value::Bool(true));
        }
        if token == crate::consts::LIT_FALSE {
            return Ok(Value::Bool(false));
        }

        // 3. Integer literals
        if let Ok(val) = token.parse::<i64>() {
            return Ok(Value::Int(val));
        }

        // 4. Float literals
        if let Ok(val) = token.parse::<f64>() {
            return Ok(Value::Float(val));
        }

        // 5. Function calls: idx(binding), len(list)
        if let Some(result) = self.try_call_function(token) {
            return result;
        }

        if let Some(base_path) = token.strip_suffix(".length") {
            return Err(TemplateError::syntax(format!(
                "'.length' is not supported — use len({base_path}) instead"
            )));
        }

        // 6. Dotted path resolution
        crate::parser::eval_expr(token, self)
    }
}

/// Parse a function call expression like `idx(bug)` or `len(items)`.
///
/// Returns `(func_name, arg)` if the expression matches `identifier(expression)`,
/// or `None` if it doesn't look like a function call.
fn parse_function_call(expr: &str) -> Option<(&str, &str)> {
    let expr = expr.trim();
    let open = expr.find('(')?;
    if !expr.ends_with(')') {
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
        let val = scope.resolve_path("name").unwrap();
        assert_eq!(val, &Value::Str("Alice".into()));
    }

    #[test]
    fn resolve_path_dotted() {
        let mut ctx = Context::new();
        let inner = Value::Dict(
            [("label".into(), Value::Str("important".into()))]
                .into_iter()
                .collect(),
        );
        ctx.set("task", inner);

        let scope = Scope::new(&ctx);
        let val = scope.resolve_path("task.label").unwrap();
        assert_eq!(val, &Value::Str("important".into()));
    }

    #[test]
    fn resolve_path_deeply_nested() {
        let mut ctx = Context::new();
        let deep = Value::Dict(
            [(
                "a".into(),
                Value::Dict(
                    [(
                        "b".into(),
                        Value::Dict([("c".into(), Value::Int(42))].into_iter().collect()),
                    )]
                    .into_iter()
                    .collect(),
                ),
            )]
            .into_iter()
            .collect(),
        );
        ctx.set("root", deep);

        let scope = Scope::new(&ctx);
        assert_eq!(scope.resolve_path("root.a.b.c").unwrap(), &Value::Int(42));
    }

    #[test]
    fn resolve_path_missing_root() {
        let ctx = Context::new();
        let scope = Scope::new(&ctx);
        let err = scope.resolve_path("absent").unwrap_err();
        assert!(matches!(err, TemplateError::UndefinedVariable(_)));
    }

    #[test]
    fn resolve_path_missing_field() {
        let mut ctx = Context::new();
        ctx.set(
            "item",
            Value::Dict(
                [("name".into(), Value::Str("x".into()))]
                    .into_iter()
                    .collect(),
            ),
        );
        let scope = Scope::new(&ctx);
        let err = scope.resolve_path("item.missing").unwrap_err();
        assert!(matches!(err, TemplateError::UndefinedVariable(_)));
    }

    #[test]
    fn resolve_path_field_on_non_dict() {
        let mut ctx = Context::new();
        ctx.set("val", 10_i64);
        let scope = Scope::new(&ctx);
        let err = scope.resolve_path("val.field").unwrap_err();
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
            Value::Dict(
                [("name".into(), Value::Str("from-layer".into()))]
                    .into_iter()
                    .collect(),
            ),
        );

        let val = scope.resolve_path("item.name").unwrap();
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

    // -- str() function tests --

    #[test]
    fn str_converts_int() {
        let tmpl =
            crate::Template::from_source("---\nparams: [count = int]\n---\n{{ str(count) }}")
                .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set("count", 42);
        assert_eq!(tmpl.render(&ctx).unwrap(), "42");
    }

    #[test]
    fn str_converts_float() {
        let tmpl = crate::Template::from_source("---\nparams: [val = float]\n---\n{{ str(val) }}")
            .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set("val", Value::Float(2.72));
        assert_eq!(tmpl.render(&ctx).unwrap(), "2.72");
    }

    #[test]
    fn str_converts_bool() {
        let tmpl = crate::Template::from_source("---\nparams: [flag = bool]\n---\n{{ str(flag) }}")
            .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set("flag", true);
        assert_eq!(tmpl.render(&ctx).unwrap(), "true");
    }

    #[test]
    fn str_passes_through_string() {
        let tmpl = crate::Template::from_source("---\nparams: [name = str]\n---\n{{ str(name) }}")
            .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set("name", "hello");
        assert_eq!(tmpl.render(&ctx).unwrap(), "hello");
    }

    #[test]
    fn str_extracts_enum_tag() {
        let tmpl = crate::Template::from_source(
            "---\nparams: [outcome = dict<>]\n---\n{{ str(outcome) }}",
        )
        .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set(
            "outcome",
            Value::Dict(HashMap::from([
                ("tag".into(), Value::Str("Confirmed".into())),
                ("evidence".into(), Value::Str("buffer overflow".into())),
            ])),
        );
        assert_eq!(tmpl.render(&ctx).unwrap(), "Confirmed");
    }

    #[test]
    fn str_rejects_list() {
        let tmpl =
            crate::Template::from_source("---\nparams: [items = list<>]\n---\n{{ str(items) }}")
                .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set("items", Value::List(vec![Value::Int(1)]));
        let err = tmpl.render(&ctx).unwrap_err();
        assert!(
            err.to_string().contains("list"),
            "should mention list: {err}"
        );
    }

    #[test]
    fn str_rejects_dict_without_tag() {
        let tmpl =
            crate::Template::from_source("---\nparams: [data = dict<>]\n---\n{{ str(data) }}")
                .unwrap();
        let mut ctx = crate::Context::new();
        ctx.set(
            "data",
            Value::Dict(HashMap::from([("name".into(), Value::Str("x".into()))])),
        );
        let err = tmpl.render(&ctx).unwrap_err();
        assert!(
            err.to_string().contains("tag"),
            "should mention missing 'tag': {err}"
        );
    }
}
