//! User-facing template rendering context.

use alloc::string::String;

use crate::{compat::HashMap, error::TemplateError, value::Value};

/// Template rendering context — holds all variables available during rendering.
///
/// # Examples
///
/// From an iterator of tuples:
/// ```
/// use md_tmpl_core::{Context, Value};
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
#[derive(Debug, Default)]
pub struct Context {
    pub(crate) values: HashMap<String, Value>,
    pub(crate) validated_token: core::sync::atomic::AtomicUsize,
}

impl Clone for Context {
    fn clone(&self) -> Self {
        Self {
            values: self.values.clone(),
            validated_token: core::sync::atomic::AtomicUsize::new(
                self.validated_token
                    .load(core::sync::atomic::Ordering::Relaxed),
            ),
        }
    }
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
            validated_token: core::sync::atomic::AtomicUsize::new(0),
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

    /// Fallibly insert a value into the context, rejecting reserved internal keys.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] if `key` is the reserved internal enum
    /// discriminant key (`__kind__`).
    pub fn try_set(
        &mut self,
        key: impl Into<String>,
        value: impl Into<Value>,
    ) -> Result<(), TemplateError> {
        let key = key.into();
        if key == crate::consts::ENUM_TAG_KEY {
            return Err(TemplateError::syntax(
                crate::consts::ERR_RESERVED_ENUM_TAG_KEY,
            ));
        }
        *self.validated_token.get_mut() = 0;
        self.values.insert(key, value.into());
        Ok(())
    }

    /// Insert a value into the context.
    ///
    /// This is the **dynamic** API — the key is a plain string and type
    /// mismatches (including reserved internal keys like `__kind__`) are caught at
    /// [`render_ctx()`](crate::Template::render_ctx) time without panicking. For
    /// immediate key validation, use [`try_set`](Self::try_set). For compile-time
    /// type safety, prefer one of:
    ///
    /// - `include_template!` (from `md-tmpl-macros`)
    ///   — generates a strongly-typed parameter struct from your template.
    /// - [`Template::render`](crate::Template::render) (feature `serde`)
    ///   — renders directly from any `Serialize` struct.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        let key = key.into();
        *self.validated_token.get_mut() = 0;
        self.values.insert(key, value.into());
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
    /// Returns `TemplateError::Syntax` if the value is not a dict/map or contains
    /// the reserved internal `__kind__` key.
    pub fn from_value(val: Value) -> Result<Self, TemplateError> {
        match val {
            Value::Struct(arc_map) => {
                if arc_map.contains_key(crate::consts::ENUM_TAG_KEY) {
                    return Err(TemplateError::syntax(
                        crate::consts::ERR_RESERVED_ENUM_TAG_KEY,
                    ));
                }
                let values =
                    alloc::sync::Arc::try_unwrap(arc_map).unwrap_or_else(|arc| (*arc).clone());
                Ok(Self {
                    values,
                    validated_token: core::sync::atomic::AtomicUsize::new(0),
                })
            }
            other => Err(TemplateError::syntax(alloc::format!(
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
    pub fn from_serialize<T: serde::Serialize>(value: &T) -> Result<Self, TemplateError> {
        crate::serde_support::to_context(value)
            .map_err(|e| TemplateError::syntax(alloc::format!("serde conversion failed: {e}")))
    }
}

/// CBOR support — behind the `cbor` feature, which implies `serde`.
///
/// Available in `no_std` — ciborium uses its own `Read` trait with a
/// blanket impl for `&[u8]`.
#[cfg(feature = "cbor")]
impl Context {
    /// Build a `Context` from a CBOR binary buffer.
    ///
    /// # Errors
    ///
    /// Returns `TemplateError::Syntax` if the buffer is invalid or not a dict/map.
    pub fn from_cbor(data: &[u8]) -> Result<Self, TemplateError> {
        let val: Value = ciborium::from_reader(data).map_err(|e| {
            TemplateError::syntax(alloc::format!("cbor deserialization failed: {e}"))
        })?;
        Self::from_value(val)
    }
}

/// `FlexBuffers` support — behind the `flexbuffers` feature, which implies
/// `std` and `serde` (the `flexbuffers` crate does not support `no_std`).
#[cfg(feature = "flexbuffers")]
impl Context {
    /// Build a `Context` from a `FlexBuffers` binary buffer.
    ///
    /// # Errors
    ///
    /// Returns `TemplateError::Syntax` if the buffer is invalid or not a dict/map.
    pub fn from_flexbuffers(data: &[u8]) -> Result<Self, TemplateError> {
        let r = flexbuffers::Reader::get_root(data)
            .map_err(|e| TemplateError::syntax(alloc::format!("flexbuffers root error: {e}")))?;
        let val: Value = serde::Deserialize::deserialize(r).map_err(|e| {
            TemplateError::syntax(alloc::format!("flexbuffers deserialization failed: {e}"))
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

    #[test]
    fn set_rejects_internal_kind_key_without_panic() {
        let mut ctx = Context::new();
        let err = ctx
            .try_set(crate::consts::ENUM_TAG_KEY, "Variant")
            .expect_err("try_set must reject reserved __kind__ key");
        assert!(err.to_string().contains("reserved internal key"));

        // Non-panicking `set` also causes both strict and allow_extra renders to return an error.
        ctx.set(crate::consts::ENUM_TAG_KEY, "Variant");
        let tmpl = crate::Template::from_source(
            r"---
params: []
---
ok",
        )
        .unwrap();
        assert!(tmpl.render_ctx(&ctx).is_err());
        assert!(tmpl.render_ctx_allowing_extra(&ctx).is_err());
    }

    #[test]
    fn set_allows_similar_but_different_keys() {
        let mut ctx = Context::new();
        ctx.set("kind", "some_kind");
        ctx.set("__type__", "some_type");
        ctx.set("kind_of", "something");
        assert!(ctx.get("kind").is_some());
        assert!(ctx.get("__type__").is_some());
    }

    #[cfg(feature = "cbor")]
    #[test]
    fn from_cbor_roundtrip() {
        use alloc::{collections::BTreeMap, vec::Vec};

        let source = BTreeMap::from([("name", "Alice"), ("role", "admin")]);
        let mut buf = Vec::new();
        ciborium::into_writer(&source, &mut buf).expect("cbor encode");

        let ctx = Context::from_cbor(&buf).expect("from_cbor");
        assert_eq!(ctx.get("name"), Some(&Value::Str("Alice".into())));
        assert_eq!(ctx.get("role"), Some(&Value::Str("admin".into())));
    }

    #[cfg(feature = "cbor")]
    #[test]
    fn from_cbor_rejects_non_map() {
        use alloc::vec::Vec;

        let mut buf = Vec::new();
        ciborium::into_writer(&42_i64, &mut buf).expect("cbor encode");
        Context::from_cbor(&buf).expect_err("a non-map CBOR value must not produce a Context");
    }

    #[cfg(feature = "cbor")]
    #[test]
    fn from_cbor_rejects_garbage() {
        Context::from_cbor(&[]).expect_err("empty buffer must error");
        Context::from_cbor(&[0xde, 0xad, 0xbe, 0xef])
            .expect_err("garbage bytes must error without panicking");
    }

    #[cfg(feature = "flexbuffers")]
    #[test]
    fn from_flexbuffers_roundtrip() {
        use serde::Serialize;

        let source = alloc::collections::BTreeMap::from([("name", "Alice"), ("role", "admin")]);
        let mut ser = flexbuffers::FlexbufferSerializer::new();
        source.serialize(&mut ser).expect("flexbuffers encode");

        let ctx = Context::from_flexbuffers(ser.view()).expect("from_flexbuffers");
        assert_eq!(ctx.get("name"), Some(&Value::Str("Alice".into())));
        assert_eq!(ctx.get("role"), Some(&Value::Str("admin".into())));
    }

    #[cfg(feature = "flexbuffers")]
    #[test]
    fn from_flexbuffers_rejects_garbage() {
        Context::from_flexbuffers(&[]).expect_err("empty flexbuffer must error");
        Context::from_flexbuffers(&[0xde, 0xad, 0xbe, 0xef])
            .expect_err("garbage flexbuffer must error without panicking");
    }
}
