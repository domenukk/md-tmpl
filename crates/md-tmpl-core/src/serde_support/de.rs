//! Deserializer implementation converting [`Value`] into `Deserialize` types.

use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::fmt;

use serde::de::{self, Deserialize};

use crate::{compat::HashMap, value::Value};

/// Error type for Value-to-Deserialize conversion.
#[derive(Debug)]
pub struct DeError(String);

impl fmt::Display for DeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl core::error::Error for DeError {}

impl de::Error for DeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

/// Convert a [`Value`] back into any `Deserialize` type.
///
/// This is the inverse of [`to_value`](super::to_value). Enums are supported:
/// - `Value::Str("Variant")` deserializes as a unit variant.
/// - `Value::Struct({"__kind__": "Variant", ...})` deserializes as a struct variant.
///
/// # Errors
///
/// Returns an error if the value shape doesn't match the target type.
///
/// # Examples
///
/// ```
/// use md_tmpl_core::Value;
/// use serde::Deserialize;
///
/// #[derive(Deserialize, Debug, PartialEq)]
/// struct Agent {
///     name: String,
///     score: i64,
/// }
///
/// let val = Value::new_struct([
///     ("name", Value::Str("Alice".into())),
///     ("score", Value::Int(95)),
/// ]);
/// let agent: Agent = md_tmpl_core::from_value(&val).unwrap();
/// assert_eq!(
///     agent,
///     Agent {
///         name: "Alice".into(),
///         score: 95
///     }
/// );
/// ```
pub fn from_value<'de, T: Deserialize<'de>>(value: &'de Value) -> Result<T, DeError> {
    T::deserialize(ValueDeserializer(value))
}

struct ValueDeserializer<'de>(&'de Value);

impl<'de> de::Deserializer<'de> for ValueDeserializer<'de> {
    type Error = DeError;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Str(s) => visitor.visit_borrowed_str(s),
            Value::Int(i) => visitor.visit_i64(*i),
            Value::Float(f) => visitor.visit_f64(*f),
            Value::Bool(b) => visitor.visit_bool(*b),
            Value::List(v) => visitor.visit_seq(SeqDeserializer::new(v)),
            Value::Struct(m) => visitor.visit_map(MapDeserializer::new(m)),
            Value::Tmpl(_) => Err(DeError(
                "cannot deserialize a Tmpl value — templates are not data".into(),
            )),
            Value::None => visitor.visit_none(),
        }
    }

    fn deserialize_bool<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Bool(b) => visitor.visit_bool(*b),
            other => Err(DeError(format!("expected bool, got {}", other.type_name()))),
        }
    }

    fn deserialize_i64<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Int(i) => visitor.visit_i64(*i),
            other => Err(DeError(format!("expected int, got {}", other.type_name()))),
        }
    }

    fn deserialize_f64<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Float(f) => visitor.visit_f64(*f),
            other => Err(DeError(format!(
                "expected float, got {}",
                other.type_name()
            ))),
        }
    }

    fn deserialize_str<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Str(s) => visitor.visit_borrowed_str(s),
            other => Err(DeError(format!("expected str, got {}", other.type_name()))),
        }
    }

    fn deserialize_string<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_seq<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::List(v) => visitor.visit_seq(SeqDeserializer::new(v)),
            other => Err(DeError(format!("expected list, got {}", other.type_name()))),
        }
    }

    fn deserialize_map<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Struct(m) => visitor.visit_map(MapDeserializer::new(m)),
            other => Err(DeError(format!("expected dict, got {}", other.type_name()))),
        }
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        match self.0 {
            // Unit variant: Value::Str("Variant")
            Value::Str(s) => visitor.visit_enum(EnumDeserializer::Unit(s)),
            // Struct variant: Value::Struct({"__kind__": "Variant", ...fields})
            Value::Struct(m) => {
                let tag_key = crate::consts::ENUM_TAG_KEY;
                let tag = match m.get(tag_key) {
                    Some(Value::Str(s)) => s.as_str(),
                    _ => {
                        return Err(DeError(format!(
                            "enum dict missing '{tag_key}' string field"
                        )));
                    }
                };
                visitor.visit_enum(EnumDeserializer::Struct { tag, fields: m })
            }
            other => Err(DeError(format!(
                "expected str or dict for enum, got {}",
                other.type_name()
            ))),
        }
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::None => visitor.visit_none(),
            // Legacy: template `None` variant string or empty string → Rust None.
            Value::Str(s) if s.is_empty() || s == crate::consts::OPTION_NONE => {
                visitor.visit_none()
            }
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_unit<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }

    fn deserialize_tuple<V: de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_identifier<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }

    // Forward numeric widths to i64/f64
    serde::forward_to_deserialize_any! { i8 i16 i32 u8 u16 u32 u64 f32 char bytes byte_buf }
}

// -- Sequence deserializer --

struct SeqDeserializer<'de> {
    iter: core::slice::Iter<'de, Value>,
}

impl<'de> SeqDeserializer<'de> {
    fn new(v: &'de [Value]) -> Self {
        Self { iter: v.iter() }
    }
}

impl<'de> de::SeqAccess<'de> for SeqDeserializer<'de> {
    type Error = DeError;
    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, DeError> {
        match self.iter.next() {
            Some(val) => seed.deserialize(ValueDeserializer(val)).map(Some),
            None => Ok(None),
        }
    }
}

// -- Map deserializer --

struct MapDeserializer<'de> {
    iter: crate::compat::hash_map::Iter<'de, String, Value>,
    current_value: Option<&'de Value>,
}

impl<'de> MapDeserializer<'de> {
    fn new(m: &'de HashMap<String, Value>) -> Self {
        Self {
            iter: m.iter(),
            current_value: None,
        }
    }
}

impl<'de> de::MapAccess<'de> for MapDeserializer<'de> {
    type Error = DeError;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, DeError> {
        match self.iter.next() {
            Some((key, val)) => {
                self.current_value = Some(val);
                seed.deserialize(de::value::BorrowedStrDeserializer::new(key.as_str()))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, DeError> {
        let val = self
            .current_value
            .take()
            .ok_or_else(|| DeError("map value without key".into()))?;
        seed.deserialize(ValueDeserializer(val))
    }
}

// -- Enum deserializer --

enum EnumDeserializer<'de> {
    Unit(&'de str),
    Struct {
        tag: &'de str,
        fields: &'de HashMap<String, Value>,
    },
}

impl<'de> de::EnumAccess<'de> for EnumDeserializer<'de> {
    type Error = DeError;
    type Variant = VariantDeserializer<'de>;

    fn variant_seed<V: de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), DeError> {
        match self {
            Self::Unit(tag) => {
                let variant = seed.deserialize(de::value::BorrowedStrDeserializer::new(tag))?;
                Ok((variant, VariantDeserializer::Unit))
            }
            Self::Struct { tag, fields } => {
                let variant = seed.deserialize(de::value::BorrowedStrDeserializer::new(tag))?;
                Ok((variant, VariantDeserializer::Struct(fields)))
            }
        }
    }
}

enum VariantDeserializer<'de> {
    Unit,
    Struct(&'de HashMap<String, Value>),
}

impl<'de> de::VariantAccess<'de> for VariantDeserializer<'de> {
    type Error = DeError;

    fn unit_variant(self) -> Result<(), DeError> {
        Ok(())
    }

    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(
        self,
        _seed: T,
    ) -> Result<T::Value, DeError> {
        Err(DeError(
            "newtype variants not supported in from_value".into(),
        ))
    }

    fn tuple_variant<V: de::Visitor<'de>>(
        self,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, DeError> {
        Err(DeError("tuple variants not supported in from_value".into()))
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        match self {
            Self::Struct(fields) => {
                // Use a filtering iterator that skips the ENUM_TAG_KEY key
                visitor.visit_map(FilteredMapDeserializer::new(fields))
            }
            Self::Unit => Err(DeError("expected struct variant, got unit".into())),
        }
    }
}

// -- Filtered map deserializer (skips ENUM_TAG_KEY for struct variants) --

struct FilteredMapDeserializer<'de> {
    iter: crate::compat::hash_map::Iter<'de, String, Value>,
    current_value: Option<&'de Value>,
}

impl<'de> FilteredMapDeserializer<'de> {
    fn new(m: &'de HashMap<String, Value>) -> Self {
        Self {
            iter: m.iter(),
            current_value: None,
        }
    }
}

impl<'de> de::MapAccess<'de> for FilteredMapDeserializer<'de> {
    type Error = DeError;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, DeError> {
        loop {
            match self.iter.next() {
                Some((key, val)) => {
                    if key == crate::consts::ENUM_TAG_KEY {
                        continue; // skip "tag"
                    }
                    self.current_value = Some(val);
                    return seed
                        .deserialize(de::value::BorrowedStrDeserializer::new(key.as_str()))
                        .map(Some);
                }
                None => return Ok(None),
            }
        }
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, DeError> {
        let val = self
            .current_value
            .take()
            .ok_or_else(|| DeError("map value without key".into()))?;
        seed.deserialize(ValueDeserializer(val))
    }
}

// ---------------------------------------------------------------------------
// Deserializer implementation: D → Value
// ---------------------------------------------------------------------------

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueVisitor;
        impl<'de> serde::de::Visitor<'de> for ValueVisitor {
            type Value = Value;
            fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                formatter.write_str("any valid template value")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(Value::Int(v))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(Value::Int(v.try_into().map_err(serde::de::Error::custom)?))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(Value::Float(v))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Value::Str(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(Value::Str(v))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(elem) = seq.next_element()? {
                    vec.push(elem);
                }
                Ok(Value::List(Arc::new(vec)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut hashmap =
                    crate::compat::HashMap::with_capacity(map.size_hint().unwrap_or(0));
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    hashmap.insert(key, value);
                }
                Ok(Value::Struct(Arc::new(hashmap)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                // null / unit → template `Value::None` for `option(T)`.
                Ok(Value::None)
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                // serde `None` → template `Value::None` for `option(T)`.
                Ok(Value::None)
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}
