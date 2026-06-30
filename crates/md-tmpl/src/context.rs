//! User-facing template rendering context.

use alloc::string::String;

use crate::{compat::HashMap, value::Value};

/// Template rendering context — holds all variables available during rendering.
///
/// # Examples
///
/// From an iterator of tuples:
/// ```
/// use md_tmpl::{Context, Value};
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
    /// mismatches are caught at [`render_ctx()`](crate::Template::render_ctx) time,
    /// not here. For compile-time type safety, prefer one of:
    ///
    /// - `include_template!` (from `md-tmpl-macros`)
    ///   — generates a strongly-typed parameter struct from your template.
    /// - [`Template::render`](crate::Template::render) (feature `serde`)
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
    /// Useful for converting a context into a [`Value::Struct`](crate::Value::Struct).
    #[must_use]
    pub fn into_inner(self) -> HashMap<String, Value> {
        self.values
    }
}

/// Collect `(&str, Value)` tuples into a `Context`.
impl<K: Into<String>, V: Into<Value>> FromIterator<(K, V)> for Context {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        let mut ctx = Self::with_capacity(lower);
        for (k, v) in iter {
            ctx.set(k, v);
        }
        ctx
    }
}

#[cfg(feature = "serde")]
impl Context {
    /// Build a `Context` directly from a [`Value::Struct`](crate::Value::Struct).
    ///
    /// # Errors
    ///
    /// Returns `TemplateError::Syntax` if the value is not a dict/map.
    pub fn from_value(val: Value) -> Result<Self, crate::error::TemplateError> {
        match val {
            Value::Struct(arc_map) => {
                // Try to unwrap the Arc to avoid cloning — we're the sole owner
                // if it was just deserialized or serialized.
                let values =
                    alloc::sync::Arc::try_unwrap(arc_map).unwrap_or_else(|arc| (*arc).clone());
                Ok(Self { values })
            }
            other => Err(crate::error::TemplateError::syntax(format!(
                "expected struct/map, got {}",
                other.type_name()
            ))),
        }
    }

    /// Build a `Context` from any `Serialize` type that serializes as a map/struct.
    ///
    /// # Errors
    ///
    /// Returns `TemplateError::Syntax` if the serialized value is not a dict/map.
    pub fn from_serialize<T: serde::Serialize>(
        value: &T,
    ) -> Result<Self, crate::error::TemplateError> {
        let val = crate::serde_support::to_value(value).map_err(|e| {
            crate::error::TemplateError::syntax(format!("serde conversion failed: {e}"))
        })?;
        Self::from_value(val)
    }

    /// Build a `Context` from a CBOR binary buffer.
    ///
    /// Available in `no_std` — ciborium uses its own `Read` trait with a
    /// blanket impl for `&[u8]`.
    ///
    /// # Errors
    ///
    /// Returns `TemplateError::Syntax` if the buffer is invalid or not a dict/map.
    pub fn from_cbor(data: &[u8]) -> Result<Self, crate::error::TemplateError> {
        let val: Value = ciborium::from_reader(data).map_err(|e| {
            crate::error::TemplateError::syntax(format!("cbor deserialization failed: {e}"))
        })?;
        Self::from_value(val)
    }
}

/// `FlexBuffers` support — requires `std` (the `flexbuffers` crate does not
/// support `no_std`).
#[cfg(feature = "std")]
#[cfg(feature = "serde")]
impl Context {
    /// Build a `Context` from a `FlexBuffers` binary buffer.
    ///
    /// # Errors
    ///
    /// Returns `TemplateError::Syntax` if the buffer is invalid or not a dict/map.
    pub fn from_flexbuffers(data: &[u8]) -> Result<Self, crate::error::TemplateError> {
        let r = flexbuffers::Reader::get_root(data).map_err(|e| {
            crate::error::TemplateError::syntax(format!("flexbuffers root error: {e}"))
        })?;
        let val: Value = serde::Deserialize::deserialize(r).map_err(|e| {
            crate::error::TemplateError::syntax(format!("flexbuffers deserialization failed: {e}"))
        })?;
        Self::from_value(val)
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
