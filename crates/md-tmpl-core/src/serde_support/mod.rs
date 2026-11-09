//! Serde integration for converting `Serialize` types into [`Value`].
//!
//! Enabled by the `serde` feature flag.

use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::fmt;

use serde::ser::{self, Serialize};

use crate::{compat::HashMap, context::Context, value::Value};

/// Convert any `Serialize` type into a [`Value`].
///
/// Structs become `Struct`, vectors become `List`, strings/numbers/bools map
/// to their corresponding `Value` variants.
///
/// # Errors
///
/// Returns an error if the type contains unsupported serde data (e.g. bytes).
///
/// # Examples
///
/// ```
/// use md_tmpl_core::Value;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Agent {
///     name: String,
///     score: i64,
/// }
///
/// let agent = Agent {
///     name: "Alice".into(),
///     score: 95,
/// };
/// let val = md_tmpl_core::to_value(&agent).unwrap();
/// assert_eq!(val.get_field("name").unwrap().to_string(), "Alice");
/// assert_eq!(val.get_field("score").unwrap().to_string(), "95");
/// ```
pub fn to_value<T: Serialize>(value: &T) -> Result<Value, SerError> {
    value.serialize(ValueSerializer)
}

/// Serialize a type directly into a [`Context`].
///
/// More efficient than `to_value` followed by `from_value`, as it avoids
/// allocating and unwrapping an `Arc` for the root struct/map.
///
/// # Errors
///
/// Returns [`SerError`] if serialization fails or if the root value is not
/// a struct or map.
pub fn to_context<T: Serialize>(value: &T) -> Result<Context, SerError> {
    value.serialize(ContextSerializer)
}

/// Error type for serde-to-Value conversion.
#[derive(Debug)]
pub struct SerError(String);

impl fmt::Display for SerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl core::error::Error for SerError {}

impl ser::Error for SerError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

// ---------------------------------------------------------------------------
// Serializer implementation
// ---------------------------------------------------------------------------

struct ValueSerializer;

impl ser::Serializer for ValueSerializer {
    type Ok = Value;
    type Error = SerError;
    type SerializeSeq = SeqBuilder;
    type SerializeTuple = SeqBuilder;
    type SerializeTupleStruct = SeqBuilder;
    type SerializeTupleVariant = SeqBuilder;
    type SerializeMap = MapBuilder;
    type SerializeStruct = StructBuilder;
    type SerializeStructVariant = StructVariantBuilder;

    fn serialize_bool(self, v: bool) -> Result<Value, SerError> {
        Ok(Value::Bool(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }
    fn serialize_i16(self, v: i16) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }
    fn serialize_i32(self, v: i32) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }
    fn serialize_i64(self, v: i64) -> Result<Value, SerError> {
        Ok(Value::Int(v))
    }
    fn serialize_u8(self, v: u8) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }
    fn serialize_u16(self, v: u16) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }
    fn serialize_u32(self, v: u32) -> Result<Value, SerError> {
        Ok(Value::Int(i64::from(v)))
    }
    fn serialize_u64(self, v: u64) -> Result<Value, SerError> {
        let i = i64::try_from(v).map_err(|_| {
            <SerError as ser::Error>::custom(format!("u64 value {v} exceeds i64::MAX"))
        })?;
        Ok(Value::Int(i))
    }
    fn serialize_f32(self, v: f32) -> Result<Value, SerError> {
        Ok(Value::Float(f64::from(v)))
    }
    fn serialize_f64(self, v: f64) -> Result<Value, SerError> {
        Ok(Value::Float(v))
    }
    fn serialize_char(self, v: char) -> Result<Value, SerError> {
        Ok(Value::Str(v.to_string()))
    }
    fn serialize_str(self, v: &str) -> Result<Value, SerError> {
        Ok(Value::Str(v.to_string()))
    }
    fn serialize_bytes(self, _v: &[u8]) -> Result<Value, SerError> {
        Err(SerError("byte arrays are not supported".into()))
    }

    fn serialize_none(self) -> Result<Value, SerError> {
        // Map to the template engine's `Value::None` for `option(T)`.
        Ok(Value::None)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Value, SerError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<Value, SerError> {
        Ok(Value::Str(String::new()))
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<Value, SerError> {
        Ok(Value::Str(String::new()))
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _idx: u32,
        variant: &'static str,
    ) -> Result<Value, SerError> {
        Ok(Value::Str(variant.to_string()))
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Value, SerError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _idx: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value, SerError> {
        let inner = value.serialize(ValueSerializer)?;
        Ok(Value::Struct(Arc::new(HashMap::from([(
            variant.to_string(),
            inner,
        )]))))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SeqBuilder, SerError> {
        Ok(SeqBuilder(Vec::with_capacity(len.unwrap_or(0))))
    }
    fn serialize_tuple(self, len: usize) -> Result<SeqBuilder, SerError> {
        Ok(SeqBuilder(Vec::with_capacity(len)))
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<SeqBuilder, SerError> {
        Ok(SeqBuilder(Vec::with_capacity(len)))
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<SeqBuilder, SerError> {
        Ok(SeqBuilder(Vec::with_capacity(len)))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<MapBuilder, SerError> {
        Ok(MapBuilder {
            map: HashMap::with_capacity(len.unwrap_or(0)),
            pending_key: None,
        })
    }
    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<StructBuilder, SerError> {
        Ok(StructBuilder {
            map: HashMap::with_capacity(len),
        })
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _idx: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<StructVariantBuilder, SerError> {
        let mut map = HashMap::with_capacity(len + 1); // +1 for the tag key
        map.insert(
            crate::consts::ENUM_TAG_KEY.to_owned(),
            Value::Str(variant.to_owned()),
        );
        Ok(StructVariantBuilder { map })
    }
}

// ---------------------------------------------------------------------------
// Sequence builder
// ---------------------------------------------------------------------------

struct SeqBuilder(Vec<Value>);

impl ser::SerializeSeq for SeqBuilder {
    type Ok = Value;
    type Error = SerError;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        self.0.push(value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Value, SerError> {
        Ok(Value::List(Arc::new(self.0)))
    }
}

impl ser::SerializeTuple for SeqBuilder {
    type Ok = Value;
    type Error = SerError;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<Value, SerError> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for SeqBuilder {
    type Ok = Value;
    type Error = SerError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<Value, SerError> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleVariant for SeqBuilder {
    type Ok = Value;
    type Error = SerError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<Value, SerError> {
        ser::SerializeSeq::end(self)
    }
}

// ---------------------------------------------------------------------------
// Map/struct builder
// ---------------------------------------------------------------------------

struct MapBuilder {
    map: HashMap<String, Value>,
    pending_key: Option<String>,
}

impl ser::SerializeMap for MapBuilder {
    type Ok = Value;
    type Error = SerError;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), SerError> {
        let key_val = key.serialize(ValueSerializer)?;
        match key_val {
            Value::Str(s) => {
                self.pending_key = Some(s);
                Ok(())
            }
            Value::Int(i) => {
                self.pending_key = Some(i.to_string());
                Ok(())
            }
            other => Err(SerError(format!(
                "map keys must be strings or integers, got {}",
                other.type_name()
            ))),
        }
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        let key = self
            .pending_key
            .take()
            .ok_or_else(|| SerError("serialize_value called without serialize_key".into()))?;
        self.map.insert(key, value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Value, SerError> {
        Ok(Value::Struct(Arc::new(self.map)))
    }
}

struct StructBuilder {
    map: HashMap<String, Value>,
}

impl ser::SerializeStruct for StructBuilder {
    type Ok = Value;
    type Error = SerError;
    #[inline]
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), SerError> {
        self.map
            .insert(key.to_owned(), value.serialize(ValueSerializer)?);
        Ok(())
    }
    #[inline]
    fn end(self) -> Result<Value, SerError> {
        Ok(Value::Struct(Arc::new(self.map)))
    }
}

struct StructVariantBuilder {
    map: HashMap<String, Value>,
}

impl ser::SerializeStructVariant for StructVariantBuilder {
    type Ok = Value;
    type Error = SerError;
    #[inline]
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), SerError> {
        self.map
            .insert(key.to_owned(), value.serialize(ValueSerializer)?);
        Ok(())
    }
    #[inline]
    fn end(self) -> Result<Value, SerError> {
        Ok(Value::Struct(Arc::new(self.map)))
    }
}

// ---------------------------------------------------------------------------
// ContextSerializer: Direct serialization into Context without root Arc
// ---------------------------------------------------------------------------

struct ContextSerializer;

macro_rules! reject_scalar_context {
    ($($fn:ident: $ty:ty => $msg:literal),* $(,)?) => {
        $(
            #[inline]
            fn $fn(self, _v: $ty) -> Result<Context, SerError> {
                Err(SerError(concat!("expected struct or map at root of Context, got ", $msg).into()))
            }
        )*
    };
}

impl ser::Serializer for ContextSerializer {
    type Ok = Context;
    type Error = SerError;
    type SerializeSeq = ser::Impossible<Context, SerError>;
    type SerializeTuple = ser::Impossible<Context, SerError>;
    type SerializeTupleStruct = ser::Impossible<Context, SerError>;
    type SerializeTupleVariant = ser::Impossible<Context, SerError>;
    type SerializeMap = ContextMapBuilder;
    type SerializeStruct = ContextStructBuilder;
    type SerializeStructVariant = ContextStructVariantBuilder;

    reject_scalar_context! {
        serialize_bool: bool => "bool",
        serialize_i8: i8 => "integer",
        serialize_i16: i16 => "integer",
        serialize_i32: i32 => "integer",
        serialize_i64: i64 => "integer",
        serialize_u8: u8 => "integer",
        serialize_u16: u16 => "integer",
        serialize_u32: u32 => "integer",
        serialize_u64: u64 => "integer",
        serialize_f32: f32 => "float",
        serialize_f64: f64 => "float",
        serialize_char: char => "char",
        serialize_str: &str => "string",
        serialize_bytes: &[u8] => "bytes",
    }

    #[inline]
    fn serialize_none(self) -> Result<Context, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got none".into(),
        ))
    }
    #[inline]
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Context, SerError> {
        value.serialize(self)
    }
    #[inline]
    fn serialize_unit(self) -> Result<Context, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got unit".into(),
        ))
    }
    #[inline]
    fn serialize_unit_struct(self, _name: &'static str) -> Result<Context, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got unit struct".into(),
        ))
    }
    #[inline]
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
    ) -> Result<Context, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got unit variant".into(),
        ))
    }
    #[inline]
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Context, SerError> {
        value.serialize(self)
    }
    #[inline]
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Context, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got newtype variant".into(),
        ))
    }
    #[inline]
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got sequence".into(),
        ))
    }
    #[inline]
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got tuple".into(),
        ))
    }
    #[inline]
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got tuple struct".into(),
        ))
    }
    #[inline]
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, SerError> {
        Err(SerError(
            "expected struct or map at root of Context, got tuple variant".into(),
        ))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<ContextMapBuilder, SerError> {
        Ok(ContextMapBuilder {
            map: HashMap::with_capacity(len.unwrap_or(0)),
            pending_key: None,
        })
    }
    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<ContextStructBuilder, SerError> {
        Ok(ContextStructBuilder {
            map: HashMap::with_capacity(len),
        })
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _idx: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<ContextStructVariantBuilder, SerError> {
        let mut map = HashMap::with_capacity(len + 1);
        map.insert(
            crate::consts::ENUM_TAG_KEY.to_owned(),
            Value::Str(variant.to_owned()),
        );
        Ok(ContextStructVariantBuilder { map })
    }
}

struct ContextStructBuilder {
    map: HashMap<String, Value>,
}

impl ser::SerializeStruct for ContextStructBuilder {
    type Ok = Context;
    type Error = SerError;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), SerError> {
        self.map
            .insert(key.to_owned(), value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Context, SerError> {
        Ok(Context {
            values: self.map,
            validated_token: core::sync::atomic::AtomicUsize::new(0),
        })
    }
}

struct ContextStructVariantBuilder {
    map: HashMap<String, Value>,
}

impl ser::SerializeStructVariant for ContextStructVariantBuilder {
    type Ok = Context;
    type Error = SerError;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), SerError> {
        self.map
            .insert(key.to_owned(), value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Context, SerError> {
        Ok(Context {
            values: self.map,
            validated_token: core::sync::atomic::AtomicUsize::new(0),
        })
    }
}

struct ContextMapBuilder {
    map: HashMap<String, Value>,
    pending_key: Option<String>,
}

impl ser::SerializeMap for ContextMapBuilder {
    type Ok = Context;
    type Error = SerError;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), SerError> {
        let key_val = key.serialize(ValueSerializer)?;
        match key_val {
            Value::Str(s) => {
                self.pending_key = Some(s);
                Ok(())
            }
            Value::Int(i) => {
                let mut buf = itoa::Buffer::new();
                self.pending_key = Some(buf.format(i).to_owned());
                Ok(())
            }
            other => Err(SerError(format!(
                "map keys must be strings or integers, got {}",
                other.type_name()
            ))),
        }
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SerError> {
        let key = self
            .pending_key
            .take()
            .ok_or_else(|| SerError("serialize_value called without serialize_key".into()))?;
        self.map.insert(key, value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Context, SerError> {
        Ok(Context {
            values: self.map,
            validated_token: core::sync::atomic::AtomicUsize::new(0),
        })
    }
}

mod de;
pub use de::{DeError, from_value};
