//! Streaming `serde::Serializer` adapter for automatic prompt-injection sanitization.
//!
//! Enabled by the `serde` feature flag. Allows any `T: serde::Serialize` to be
//! serialized into JSON, CBOR, `md_tmpl_core::Value`, or any other `serde` target
//! while automatically neutralizing LLM control tokens or wrapping untrusted strings
//! in XML quarantine tags in a single streaming pass.

use alloc::{borrow::Cow, string::ToString};
use core::fmt;

use serde::ser::{
    Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

use crate::{
    filter::{
        DEFAULT_QUARANTINE_TAG, DEFAULT_SANITIZE_TAG, is_quarantined_str, is_sanitized_block_str,
        quarantine_untrusted_idempotent_str, sanitize_block_idempotent_str, sanitize_str,
        sanitize_tokens_str, sanitize_untrusted_str,
    },
    serde_support::{SerError, to_value},
    value::Value,
};

/// String sanitization mode applied by [`TransformingSerializer`] and [`serialize_with_sanitization`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringSanitizeMode {
    /// Neutralize all known LLM control/turn/tool delimiters ([`crate::TOKEN_DELIMITERS`])
    /// and generic `<|...|>` / `<｜...｜>` special tokens via [`sanitize_tokens_str`].
    SanitizeTokens,
    /// Neutralize all known LLM control/turn/tool delimiters and any XML breakout tags
    /// matching `tag_spec` via [`sanitize_untrusted_str`] without wrapping in an outer
    /// XML envelope (already-quarantined envelopes matching `tag_spec` are preserved unchanged).
    SanitizeUntrusted {
        /// Comma-separated XML `NCName` boundary tag specification (e.g. `"tool_output_quarantine"`).
        tag_spec: &'static str,
    },
    /// Neutralize all known LLM control/turn/tool delimiters and XML breakout tags matching
    /// `tag_spec`, and idempotently wrap each string in `<primary_tag>\n...\n</primary_tag>`
    /// via [`quarantine_untrusted_idempotent_str`].
    Quarantine {
        /// Comma-separated XML `NCName` boundary tag specification (e.g. `"tool_output_quarantine"`).
        tag_spec: &'static str,
    },
    /// Unified untrusted-data block sanitization: neutralize LLM control tokens and XML breakout
    /// tags matching `tag_spec`, and idempotently wrap each string in
    /// `<primary_tag>\n{notice}\n{sanitized}\n</primary_tag>` via [`sanitize_block_idempotent_str`].
    SanitizeBlock {
        /// Comma-separated XML `NCName` boundary tag specification (e.g. `"untrusted_content"`).
        tag_spec: &'static str,
        /// Optional custom boundary notice (or `None` for [`crate::DEFAULT_SANITIZE_NOTICE`]).
        custom_notice: Option<&'static str>,
    },
}

impl StringSanitizeMode {
    /// Apply this sanitization mode to `s`, returning [`Cow::Borrowed`] when no modification is needed.
    #[must_use]
    pub fn apply(self, s: &str) -> Cow<'_, str> {
        match self {
            Self::SanitizeTokens => sanitize_tokens_str(s),
            Self::SanitizeUntrusted { tag_spec } => {
                if is_quarantined_str(s, Some(tag_spec))
                    || is_sanitized_block_str(s, Some(tag_spec), None)
                {
                    Cow::Borrowed(s)
                } else {
                    sanitize_str(s, Some(tag_spec))
                        .unwrap_or_else(|_| sanitize_untrusted_str(s, DEFAULT_SANITIZE_TAG))
                }
            }
            Self::Quarantine { tag_spec } => {
                if is_quarantined_str(s, Some(tag_spec)) {
                    Cow::Borrowed(s)
                } else {
                    let wrapped = quarantine_untrusted_idempotent_str(s, Some(tag_spec))
                        .unwrap_or_else(|_| {
                            let clean = sanitize_untrusted_str(s, DEFAULT_QUARANTINE_TAG);
                            alloc::format!(
                                "<{DEFAULT_QUARANTINE_TAG}>\n{clean}\n</{DEFAULT_QUARANTINE_TAG}>"
                            )
                        });
                    Cow::Owned(wrapped)
                }
            }
            Self::SanitizeBlock {
                tag_spec,
                custom_notice,
            } => {
                if is_sanitized_block_str(s, Some(tag_spec), custom_notice) {
                    Cow::Borrowed(s)
                } else {
                    let wrapped = sanitize_block_idempotent_str(s, Some(tag_spec), custom_notice)
                        .unwrap_or_else(|_| {
                            let clean = sanitize_untrusted_str(s, DEFAULT_SANITIZE_TAG);
                            let notice = crate::filter::format_sanitize_notice(
                                DEFAULT_SANITIZE_TAG,
                                custom_notice,
                            );
                            if notice.is_empty() {
                                alloc::format!(
                                    "<{DEFAULT_SANITIZE_TAG}>\n{clean}\n</{DEFAULT_SANITIZE_TAG}>"
                                )
                            } else {
                                alloc::format!(
                                    "<{DEFAULT_SANITIZE_TAG}>\n{notice}\n{clean}\n</{DEFAULT_SANITIZE_TAG}>"
                                )
                            }
                        });
                    Cow::Owned(wrapped)
                }
            }
        }
    }

    /// Mode used for map keys (always single-line token sanitization without multiline XML wrappers).
    #[must_use]
    pub const fn for_map_key(self) -> Self {
        match self {
            Self::SanitizeTokens => Self::SanitizeTokens,
            Self::SanitizeUntrusted { tag_spec }
            | Self::Quarantine { tag_spec }
            | Self::SanitizeBlock { tag_spec, .. } => Self::SanitizeUntrusted { tag_spec },
        }
    }
}

/// Serialize `value` into `serializer`, transforming all emitted strings according to `mode`.
///
/// # Errors
/// Returns `S::Error` if the underlying serializer fails.
pub fn serialize_with_sanitization<T, S>(
    value: &T,
    serializer: S,
    mode: StringSanitizeMode,
) -> Result<S::Ok, S::Error>
where
    T: Serialize + ?Sized,
    S: Serializer,
{
    value.serialize(TransformingSerializer::new(serializer, mode))
}

/// Convert any `Serialize` type into a [`Value`] while transforming all strings according to `mode`.
///
/// # Errors
/// Returns [`SerError`] if the type contains unsupported serde data (e.g. raw byte arrays).
pub fn to_sanitized_value<T: Serialize + ?Sized>(
    value: &T,
    mode: StringSanitizeMode,
) -> Result<Value, SerError> {
    to_value(&TransformedRef { value, mode })
}

struct TransformedRef<'a, T: ?Sized> {
    value: &'a T,
    mode: StringSanitizeMode,
}

impl<T: Serialize + ?Sized> Serialize for TransformedRef<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_with_sanitization(self.value, serializer, self.mode)
    }
}

/// Streaming [`serde::Serializer`] adapter that applies [`StringSanitizeMode`] to every
/// serialized string value.
#[derive(Debug)]
pub struct TransformingSerializer<S> {
    inner: S,
    mode: StringSanitizeMode,
}

impl<S> TransformingSerializer<S> {
    /// Wrap `inner` serializer with `mode`.
    #[must_use]
    pub const fn new(inner: S, mode: StringSanitizeMode) -> Self {
        Self { inner, mode }
    }
}

impl<S: Serializer> Serializer for TransformingSerializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;

    type SerializeSeq = TransformingCompound<S::SerializeSeq>;
    type SerializeTuple = TransformingCompound<S::SerializeTuple>;
    type SerializeTupleStruct = TransformingCompound<S::SerializeTupleStruct>;
    type SerializeTupleVariant = TransformingCompound<S::SerializeTupleVariant>;
    type SerializeMap = TransformingCompound<S::SerializeMap>;
    type SerializeStruct = TransformingCompound<S::SerializeStruct>;
    type SerializeStructVariant = TransformingCompound<S::SerializeStructVariant>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_bool(v)
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_i8(v)
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_i16(v)
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_i32(v)
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_i64(v)
    }

    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_i128(v)
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_u8(v)
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_u16(v)
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_u32(v)
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_u64(v)
    }

    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_u128(v)
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_f32(v)
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_f64(v)
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_char(v)
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        let transformed = self.mode.apply(v);
        self.inner.serialize_str(&transformed)
    }

    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + fmt::Display,
    {
        let raw = value.to_string();
        let transformed = self.mode.apply(&raw);
        self.inner.serialize_str(&transformed)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_bytes(v)
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_none()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_some(&TransformedRef {
            value,
            mode: self.mode,
        })
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit()
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_struct(name)
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_unit_variant(name, variant_index, variant)
    }

    fn serialize_newtype_struct<T>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_newtype_struct(
            name,
            &TransformedRef {
                value,
                mode: self.mode,
            },
        )
    }

    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_newtype_variant(
            name,
            variant_index,
            variant,
            &TransformedRef {
                value,
                mode: self.mode,
            },
        )
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(TransformingCompound {
            inner: self.inner.serialize_seq(len)?,
            mode: self.mode,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(TransformingCompound {
            inner: self.inner.serialize_tuple(len)?,
            mode: self.mode,
        })
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(TransformingCompound {
            inner: self.inner.serialize_tuple_struct(name, len)?,
            mode: self.mode,
        })
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(TransformingCompound {
            inner: self
                .inner
                .serialize_tuple_variant(name, variant_index, variant, len)?,
            mode: self.mode,
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(TransformingCompound {
            inner: self.inner.serialize_map(len)?,
            mode: self.mode,
        })
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(TransformingCompound {
            inner: self.inner.serialize_struct(name, len)?,
            mode: self.mode,
        })
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(TransformingCompound {
            inner: self
                .inner
                .serialize_struct_variant(name, variant_index, variant, len)?,
            mode: self.mode,
        })
    }
}

/// Compound serializer state wrapper used by [`TransformingSerializer`].
#[derive(Debug)]
pub struct TransformingCompound<C> {
    inner: C,
    mode: StringSanitizeMode,
}

impl<C: SerializeSeq> SerializeSeq for TransformingCompound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_element(&TransformedRef {
            value,
            mode: self.mode,
        })
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

impl<C: SerializeTuple> SerializeTuple for TransformingCompound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_element(&TransformedRef {
            value,
            mode: self.mode,
        })
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

impl<C: SerializeTupleStruct> SerializeTupleStruct for TransformingCompound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_field(&TransformedRef {
            value,
            mode: self.mode,
        })
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

impl<C: SerializeTupleVariant> SerializeTupleVariant for TransformingCompound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_field(&TransformedRef {
            value,
            mode: self.mode,
        })
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

impl<C: SerializeMap> SerializeMap for TransformingCompound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_key(&TransformedRef {
            value: key,
            mode: self.mode.for_map_key(),
        })
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_value(&TransformedRef {
            value,
            mode: self.mode,
        })
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

impl<C: SerializeStruct> SerializeStruct for TransformingCompound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_field(
            key,
            &TransformedRef {
                value,
                mode: self.mode,
            },
        )
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

impl<C: SerializeStructVariant> SerializeStructVariant for TransformingCompound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_field(
            key,
            &TransformedRef {
                value,
                mode: self.mode,
            },
        )
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}
