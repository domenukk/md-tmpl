//! User-facing template rendering context.

use std::collections::HashMap;

use crate::value::Value;

/// Template rendering context — holds all variables available during rendering.
///
/// # Examples
///
/// From an iterator of tuples:
/// ```
/// use prompt_templates::{Context, Value};
///
/// let ctx: Context = vec![
///     ("name", Value::from("Alice")),
///     ("count", Value::from(3_i64)),
/// ]
/// .into_iter()
/// .collect();
///
/// assert!(ctx.get("name").is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub(crate) values: HashMap<String, Value>,
}

impl Context {
    /// Create an empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty context pre-allocated for `capacity` variables.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: HashMap::with_capacity(capacity),
        }
    }

    /// Returns the number of variables in this context.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` if this context contains no variables.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns `true` if a variable with the given key exists.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// Insert a value into the context.
    ///
    /// This is the **dynamic** API — the key is a plain string and type
    /// mismatches are caught at [`render()`](crate::Template::render) time,
    /// not here. For compile-time type safety, prefer one of:
    ///
    /// - `template_params_struct!` (from `prompt-templates-macros`)
    ///   — generates a strongly-typed parameter struct from your template.
    /// - [`Template::render_serde`](crate::Template::render_serde) (feature `serde`)
    ///   — renders directly from any `Serialize` struct.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.values.insert(key.into(), value.into());
    }

    /// Builder-style insert — returns `self` for chaining.
    #[must_use]
    pub fn var(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.set(key, value);
        self
    }

    /// Look up a top-level variable.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Consume this context and return the inner variable map.
    ///
    /// Useful for converting a context into a [`Value::Dict`](crate::Value::Dict).
    #[must_use]
    pub fn into_inner(self) -> HashMap<String, Value> {
        self.values
    }
}

/// Collect `(&str, Value)` tuples into a `Context`.
impl<K: Into<String>, V: Into<Value>> FromIterator<(K, V)> for Context {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut ctx = Self::new();
        for (k, v) in iter {
            ctx.set(k, v);
        }
        ctx
    }
}

#[cfg(feature = "serde")]
impl Context {
    /// Build a `Context` from any `Serialize` type that serializes as a map/struct.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`](crate::TemplateError::Syntax) if the serialized value is not a dict/map.
    pub fn from_serialize<T: serde::Serialize>(
        value: &T,
    ) -> Result<Self, crate::error::TemplateError> {
        let val = crate::serde_support::to_value(value).map_err(|e| {
            crate::error::TemplateError::syntax(format!("serde conversion failed: {e}"))
        })?;
        match val {
            Value::Dict(map) => {
                let mut ctx = Self::new();
                for (k, v) in map {
                    ctx.values.insert(k, v);
                }
                Ok(ctx)
            }
            other => Err(crate::error::TemplateError::syntax(format!(
                "expected struct/map, got {}",
                other.type_name()
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_context_is_empty() {
        let ctx = Context::new();
        assert!(ctx.get("anything").is_none());
    }

    #[test]
    fn set_and_get_str() {
        let mut ctx = Context::new();
        ctx.set("greeting", "hello");
        assert_eq!(ctx.get("greeting"), Some(&Value::Str("hello".into())));
    }

    #[test]
    fn set_and_get_bool() {
        let mut ctx = Context::new();
        ctx.set("flag", true);
        assert_eq!(ctx.get("flag"), Some(&Value::Bool(true)));
    }

    #[test]
    fn set_and_get_int() {
        let mut ctx = Context::new();
        ctx.set("count", 42_i64);
        assert_eq!(ctx.get("count"), Some(&Value::Int(42)));
    }

    #[test]
    fn overwrite_value() {
        let mut ctx = Context::new();
        ctx.set("k", "first");
        ctx.set("k", "second");
        assert_eq!(ctx.get("k"), Some(&Value::Str("second".into())));
    }

    #[test]
    fn get_missing_returns_none() {
        let ctx = Context::new();
        assert_eq!(ctx.get("nonexistent"), None);
    }

    #[test]
    fn default_is_same_as_new() {
        let a = Context::new();
        let b = Context::default();
        assert!(a.values.is_empty());
        assert!(b.values.is_empty());
    }
}
