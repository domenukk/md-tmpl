//! Template value types.

use std::{collections::HashMap, fmt};

/// A value that can be inserted into a template.
#[derive(Debug, Clone)]
pub enum Value {
    /// A plain string.
    Str(String),
    /// A boolean.
    Bool(bool),
    /// A 64-bit integer.
    Int(i64),
    /// A 64-bit float.
    Float(f64),
    /// An ordered list of values.
    List(Vec<Value>),
    /// A string-keyed map of values.
    Dict(HashMap<String, Value>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::List(a), Self::List(b)) => a == b,
            (Self::Dict(a), Self::Dict(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str(s) => f.write_str(s),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::List(items) => write!(f, "[<list of {}>]", items.len()),
            Self::Dict(map) => write!(f, "{{<dict of {}>}}", map.len()),
        }
    }
}

impl Value {
    /// Returns `true` if the value is considered "truthy".
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Str(s) => !s.is_empty(),
            Self::Bool(b) => *b,
            Self::Int(i) => *i != 0,
            Self::Float(f) => *f != 0.0,
            Self::List(v) => !v.is_empty(),
            Self::Dict(m) => !m.is_empty(),
        }
    }

    /// Returns the type name as a static string.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Str(_) => "str",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::List(_) => "list",
            Self::Dict(_) => "dict",
        }
    }

    /// Access a field on a Dict value.
    #[must_use]
    pub fn get_field(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Dict(m) => m.get(key),
            _ => None,
        }
    }

    /// Returns `true` if this is a `Str` variant.
    #[must_use]
    pub fn is_str(&self) -> bool {
        matches!(self, Self::Str(_))
    }

    /// Returns `true` if this is an `Int` variant.
    #[must_use]
    pub fn is_int(&self) -> bool {
        matches!(self, Self::Int(_))
    }

    /// Returns `true` if this is a `Float` variant.
    #[must_use]
    pub fn is_float(&self) -> bool {
        matches!(self, Self::Float(_))
    }

    /// Returns `true` if this is a `Bool` variant.
    #[must_use]
    pub fn is_bool(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    /// Returns `true` if this is a `List` variant.
    #[must_use]
    pub fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }

    /// Returns `true` if this is a `Dict` variant.
    #[must_use]
    pub fn is_dict(&self) -> bool {
        matches!(self, Self::Dict(_))
    }

    /// Returns the inner `&str` if this is a `Str` variant.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the inner `i64` if this is an `Int` variant.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the inner `f64` if this is a `Float` variant.
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns the inner `bool` if this is a `Bool` variant.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns a slice of the inner list if this is a `List` variant.
    #[must_use]
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(v) => Some(v),
            _ => None,
        }
    }

    /// Returns a reference to the inner map if this is a `Dict` variant.
    #[must_use]
    pub fn as_dict(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Self::Dict(m) => Some(m),
            _ => None,
        }
    }

    /// Create a `Dict` from an iterator of key-value pairs.
    ///
    /// Accepts arrays, slices, vecs — anything iterable.
    ///
    /// # Examples
    ///
    /// ```
    /// use prompt_templates::Value;
    ///
    /// let v = Value::dict([("name", "Alice"), ("role", "admin")]);
    /// assert_eq!(v.get_field("name").unwrap().to_string(), "Alice");
    /// ```
    #[must_use]
    pub fn dict<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<Value>,
    {
        Self::Dict(
            pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }

    /// Create a `List` from an iterator of values.
    ///
    /// Accepts arrays, slices, vecs — anything iterable.
    ///
    /// # Examples
    ///
    /// ```
    /// use prompt_templates::Value;
    ///
    /// let v = Value::list([
    ///     Value::dict([("label", "alpha")]),
    ///     Value::dict([("label", "beta")]),
    /// ]);
    /// assert_eq!(v.type_name(), "list");
    /// ```
    #[must_use]
    pub fn list<I, V>(items: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: Into<Value>,
    {
        Self::List(items.into_iter().map(Into::into).collect())
    }
}

#[cfg(feature = "serde")]
impl Value {
    /// Create a `Value` from any `Serialize` type.
    ///
    /// This is the same as [`to_value`](crate::to_value) but available as a
    /// method on `Value` for convenience.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use prompt_templates::Value;
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct Agent {
    ///     name: String,
    /// }
    ///
    /// let val = Value::from_serialize(&Agent {
    ///     name: "Alice".into(),
    /// })
    /// .unwrap();
    /// assert_eq!(val.get_field("name").unwrap().as_str(), Some("Alice"));
    /// ```
    pub fn from_serialize<T: serde::Serialize>(
        value: &T,
    ) -> Result<Self, crate::serde_support::SerError> {
        crate::serde_support::to_value(value)
    }

    /// Deserialize this `Value` into a Rust type.
    ///
    /// This is the same as [`from_value`](crate::from_value) but available as
    /// a method on `Value` for convenience.
    ///
    /// # Errors
    ///
    /// Returns an error if the value shape doesn't match `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use prompt_templates::Value;
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize, Debug, PartialEq)]
    /// struct Agent {
    ///     name: String,
    /// }
    ///
    /// let val = Value::dict([("name", Value::Str("Alice".into()))]);
    /// let agent: Agent = val.deserialize_into().unwrap();
    /// assert_eq!(
    ///     agent,
    ///     Agent {
    ///         name: "Alice".into()
    ///     }
    /// );
    /// ```
    pub fn deserialize_into<'de, T: serde::Deserialize<'de>>(
        &'de self,
    ) -> Result<T, crate::serde_support::DeError> {
        crate::serde_support::from_value(self)
    }
}

// ---------------------------------------------------------------------------
// From conversions
// ---------------------------------------------------------------------------

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Str(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Self::Int(i64::from(i))
    }
}

impl From<u32> for Value {
    fn from(i: u32) -> Self {
        Self::Int(i64::from(i))
    }
}

impl TryFrom<u64> for Value {
    type Error = std::num::TryFromIntError;
    fn try_from(i: u64) -> Result<Self, Self::Error> {
        Ok(Self::Int(i64::try_from(i)?))
    }
}

impl TryFrom<usize> for Value {
    type Error = std::num::TryFromIntError;
    fn try_from(i: usize) -> Result<Self, Self::Error> {
        Ok(Self::Int(i64::try_from(i)?))
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<f32> for Value {
    fn from(f: f32) -> Self {
        Self::Float(f64::from(f))
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Self::List(v)
    }
}

impl From<HashMap<String, Value>> for Value {
    fn from(m: HashMap<String, Value>) -> Self {
        Self::Dict(m)
    }
}

// ---------------------------------------------------------------------------
// TryFrom conversions (consuming)
// ---------------------------------------------------------------------------

/// Error returned when a [`Value`] is the wrong variant for a conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueTypeError {
    /// The expected type name.
    pub expected: &'static str,
    /// The actual type name of the value.
    pub actual: &'static str,
}

impl fmt::Display for ValueTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}, got {}", self.expected, self.actual)
    }
}

impl std::error::Error for ValueTypeError {}

impl TryFrom<Value> for String {
    type Error = ValueTypeError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Str(s) => Ok(s),
            other => Err(ValueTypeError {
                expected: "str",
                actual: other.type_name(),
            }),
        }
    }
}

impl TryFrom<Value> for i64 {
    type Error = ValueTypeError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Int(i) => Ok(i),
            other => Err(ValueTypeError {
                expected: "int",
                actual: other.type_name(),
            }),
        }
    }
}

impl TryFrom<Value> for f64 {
    type Error = ValueTypeError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Float(f) => Ok(f),
            other => Err(ValueTypeError {
                expected: "float",
                actual: other.type_name(),
            }),
        }
    }
}

impl TryFrom<Value> for bool {
    type Error = ValueTypeError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Bool(b) => Ok(b),
            other => Err(ValueTypeError {
                expected: "bool",
                actual: other.type_name(),
            }),
        }
    }
}

impl TryFrom<Value> for Vec<Value> {
    type Error = ValueTypeError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::List(l) => Ok(l),
            other => Err(ValueTypeError {
                expected: "list",
                actual: other.type_name(),
            }),
        }
    }
}

impl<S: std::hash::BuildHasher + Default> TryFrom<Value> for HashMap<String, Value, S> {
    type Error = ValueTypeError;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Dict(m) => Ok(m.into_iter().collect()),
            other => Err(ValueTypeError {
                expected: "dict",
                actual: other.type_name(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Display --

    #[test]
    fn display_str() {
        assert_eq!(Value::Str("hello".into()).to_string(), "hello");
    }

    #[test]
    fn display_bool() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Bool(false).to_string(), "false");
    }

    #[test]
    fn display_int() {
        assert_eq!(Value::Int(42).to_string(), "42");
        assert_eq!(Value::Int(-7).to_string(), "-7");
    }

    #[test]
    fn display_float() {
        assert_eq!(Value::Float(3.25).to_string(), "3.25");
    }

    #[test]
    fn display_list() {
        let list = Value::List(vec![Value::Int(1)]);
        assert_eq!(list.to_string(), "[<list of 1>]");
        assert_eq!(Value::List(vec![]).to_string(), "[<list of 0>]");
    }

    #[test]
    fn display_dict() {
        let dict = Value::Dict(HashMap::from([("k".into(), Value::Int(1))]));
        assert_eq!(dict.to_string(), "{<dict of 1>}");
        assert_eq!(Value::Dict(HashMap::new()).to_string(), "{<dict of 0>}");
    }

    // -- is_truthy --

    #[test]
    fn truthy_str() {
        assert!(Value::Str("hello".into()).is_truthy());
        assert!(!Value::Str(String::new()).is_truthy());
    }

    #[test]
    fn truthy_bool() {
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Bool(false).is_truthy());
    }

    #[test]
    fn truthy_int() {
        assert!(Value::Int(1).is_truthy());
        assert!(Value::Int(-1).is_truthy());
        assert!(!Value::Int(0).is_truthy());
    }

    #[test]
    fn truthy_float() {
        assert!(Value::Float(0.1).is_truthy());
        assert!(!Value::Float(0.0).is_truthy());
    }

    #[test]
    fn truthy_list() {
        assert!(Value::List(vec![Value::Int(1)]).is_truthy());
        assert!(!Value::List(vec![]).is_truthy());
    }

    #[test]
    fn truthy_dict() {
        let populated = Value::Dict(HashMap::from([("k".into(), Value::Int(1))]));
        assert!(populated.is_truthy());
        assert!(!Value::Dict(HashMap::new()).is_truthy());
    }

    // -- type_name --

    #[test]
    fn type_names() {
        assert_eq!(Value::Str("x".into()).type_name(), "str");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Int(0).type_name(), "int");
        assert_eq!(Value::Float(0.0).type_name(), "float");
        assert_eq!(Value::List(vec![]).type_name(), "list");
        assert_eq!(Value::Dict(HashMap::new()).type_name(), "dict");
    }

    // -- get_field --

    #[test]
    fn get_field_on_dict() {
        let dict = Value::Dict(HashMap::from([
            ("name".into(), Value::Str("Alice".into())),
            ("score".into(), Value::Int(95)),
        ]));
        assert_eq!(dict.get_field("name"), Some(&Value::Str("Alice".into())));
        assert_eq!(dict.get_field("score"), Some(&Value::Int(95)));
        assert_eq!(dict.get_field("missing"), None);
    }

    #[test]
    fn get_field_on_non_dict_returns_none() {
        assert_eq!(Value::Str("x".into()).get_field("any"), None);
        assert_eq!(Value::Int(1).get_field("any"), None);
        assert_eq!(Value::List(vec![]).get_field("any"), None);
    }

    // -- From conversions --

    #[test]
    fn from_str_ref() {
        let v: Value = "hello".into();
        assert_eq!(v, Value::Str("hello".into()));
    }

    #[test]
    fn from_string() {
        let v: Value = String::from("world").into();
        assert_eq!(v, Value::Str("world".into()));
    }

    #[test]
    fn from_bool() {
        let v: Value = true.into();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn from_i64() {
        let v: Value = 42_i64.into();
        assert_eq!(v, Value::Int(42));
    }

    #[test]
    fn from_i32() {
        let v: Value = 7_i32.into();
        assert_eq!(v, Value::Int(7));
    }

    #[test]
    fn from_u32() {
        let v: Value = 100_u32.into();
        assert_eq!(v, Value::Int(100));
    }

    #[test]
    fn try_from_u64() {
        let v = Value::try_from(999_u64).unwrap();
        assert_eq!(v, Value::Int(999));
    }

    #[test]
    fn try_from_u64_overflow() {
        let result = Value::try_from(u64::MAX);
        assert!(result.is_err(), "u64::MAX should not fit in i64");
    }

    #[test]
    fn try_from_usize() {
        let v = Value::try_from(5_usize).unwrap();
        assert_eq!(v, Value::Int(5));
    }

    #[test]
    fn from_f64() {
        let v: Value = 2.5_f64.into();
        assert_eq!(v, Value::Float(2.5));
    }

    #[test]
    fn from_f32() {
        let v: Value = 1.5_f32.into();
        // f32 → f64 conversion
        assert!(matches!(v, Value::Float(f) if (f - 1.5).abs() < f64::EPSILON));
    }

    #[test]
    fn from_vec_value() {
        let items = vec![Value::Int(1), Value::Str("two".into())];
        let v: Value = items.into();
        assert!(matches!(v, Value::List(ref l) if l.len() == 2));
    }

    #[test]
    fn from_hashmap_value() {
        let map = HashMap::from([("k".into(), Value::Bool(true))]);
        let v: Value = map.into();
        assert_eq!(v.get_field("k"), Some(&Value::Bool(true)));
    }

    // -- as_* accessors --

    #[test]
    fn as_str_returns_some_for_str() {
        assert_eq!(Value::Str("hello".into()).as_str(), Some("hello"));
    }

    #[test]
    fn as_str_returns_none_for_non_str() {
        assert_eq!(Value::Int(42).as_str(), None);
    }

    #[test]
    fn as_int_returns_some_for_int() {
        assert_eq!(Value::Int(42).as_int(), Some(42));
    }

    #[test]
    fn as_int_returns_none_for_non_int() {
        assert_eq!(Value::Str("42".into()).as_int(), None);
    }

    #[test]
    fn as_float_returns_some_for_float() {
        assert_eq!(Value::Float(3.25).as_float(), Some(3.25));
    }

    #[test]
    fn as_float_returns_none_for_non_float() {
        assert_eq!(Value::Int(3).as_float(), None);
    }

    #[test]
    fn as_bool_returns_some_for_bool() {
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
    }

    #[test]
    fn as_bool_returns_none_for_non_bool() {
        assert_eq!(Value::Str("true".into()).as_bool(), None);
    }

    #[test]
    fn as_list_returns_some_for_list() {
        let items = vec![Value::Int(1), Value::Int(2)];
        let v = Value::List(items.clone());
        assert_eq!(v.as_list(), Some(items.as_slice()));
    }

    #[test]
    fn as_list_returns_none_for_non_list() {
        assert_eq!(Value::Int(1).as_list(), None);
    }

    #[test]
    fn as_dict_returns_some_for_dict() {
        let map = HashMap::from([("k".into(), Value::Int(1))]);
        let v = Value::Dict(map.clone());
        assert_eq!(v.as_dict(), Some(&map));
    }

    #[test]
    fn as_dict_returns_none_for_non_dict() {
        assert_eq!(Value::Int(1).as_dict(), None);
    }

    // -- TryFrom conversions --

    #[test]
    fn try_from_str_success() {
        let v = Value::Str("hello".into());
        assert_eq!(String::try_from(v).unwrap(), "hello");
    }

    #[test]
    fn try_from_str_failure_has_message() {
        let v = Value::Int(42);
        let err = String::try_from(v).unwrap_err();
        assert_eq!(err.expected, "str");
        assert_eq!(err.actual, "int");
        assert_eq!(err.to_string(), "expected str, got int");
    }

    #[test]
    fn try_from_i64_success() {
        let v = Value::Int(99);
        assert_eq!(i64::try_from(v).unwrap(), 99);
    }

    #[test]
    fn try_from_i64_failure() {
        let v = Value::Str("99".into());
        let err = i64::try_from(v).expect_err("Str should not convert to i64");
        assert_eq!(err.expected, "int");
        assert_eq!(err.actual, "str");
    }

    #[test]
    fn try_from_f64_success() {
        let v = Value::Float(2.5);
        assert!((f64::try_from(v).unwrap() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn try_from_bool_success() {
        let v = Value::Bool(false);
        assert!(!bool::try_from(v).unwrap());
    }

    #[test]
    fn try_from_vec_success() {
        let v = Value::List(vec![Value::Int(1)]);
        let list = Vec::<Value>::try_from(v).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn try_from_hashmap_success() {
        let v = Value::Dict(HashMap::from([("k".into(), Value::Int(1))]));
        let map = HashMap::<String, Value>::try_from(v).unwrap();
        assert_eq!(map.len(), 1);
    }
}
