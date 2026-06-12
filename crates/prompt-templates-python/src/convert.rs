//! Bidirectional conversion between Python objects and [`Value`].

use std::collections::HashMap;

use prompt_templates::Value;
use pyo3::{
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString},
};

/// Tag key used for internally-tagged enum variants in dicts.
/// Must match [`prompt_templates::consts::ENUM_TAG_KEY`].
const ENUM_TAG_KEY: &str = prompt_templates::consts::ENUM_TAG_KEY;

/// Convert a Python object into a template [`Value`].
///
/// Handles (in order):
/// - `bool` → `Value::Bool`  (checked before int — `bool` subclasses `int`)
/// - `int`  → `Value::Int`
/// - `float` → `Value::Float`
/// - `str`  → `Value::Str`
/// - `list` → `Value::List`  (recursive)
/// - `dict` → `Value::Dict`  (recursive)
/// - objects with `_prompt_template_tag` → enum variant dicts
/// - `enum.Enum` members (have `_name_`) → `Value::Str` or tagged dict
/// - objects with `__dict__` → `Value::Dict` (dataclass-like)
///
/// # Errors
///
/// Returns a `PyErr` if the value type is not supported or conversion fails.
pub(crate) fn py_to_value(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    // Bool must be checked before int because bool is a subclass of int.
    if obj.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(obj.extract::<bool>()?));
    }
    if obj.is_instance_of::<PyInt>() {
        return Ok(Value::Int(obj.extract::<i64>()?));
    }
    if obj.is_instance_of::<PyFloat>() {
        return Ok(Value::Float(obj.extract::<f64>()?));
    }
    if obj.is_instance_of::<PyString>() {
        return Ok(Value::Str(obj.extract::<String>()?));
    }
    if obj.is_instance_of::<PyList>() {
        let list = obj.downcast::<PyList>()?;
        let mut items = Vec::with_capacity(list.len());
        for item in list.iter() {
            items.push(py_to_value(&item)?);
        }
        return Ok(Value::List(items));
    }
    if obj.is_instance_of::<PyDict>() {
        return py_dict_to_value(obj.downcast::<PyDict>()?);
    }

    // Check for generated enum variant instances (have `_prompt_template_tag`).
    if let Some(value) = try_convert_tagged_variant(obj)? {
        return Ok(value);
    }

    // Check for Python enum.Enum members (have `_name_`).
    if let Some(value) = try_convert_enum_member(obj)? {
        return Ok(value);
    }

    // Fallback: dataclass-like objects with __dict__.
    if let Some(value) = try_convert_dict_object(obj)? {
        return Ok(value);
    }

    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "cannot convert Python type '{}' to template Value",
        obj.get_type().qualname()?
    )))
}

/// Try to convert an object with `_prompt_template_tag` + `_prompt_template_fields`.
///
/// Returns `Ok(None)` if the object doesn't have the tag attribute,
/// `Ok(Some(value))` on success, or `Err` on conversion failure.
fn try_convert_tagged_variant(obj: &Bound<'_, PyAny>) -> PyResult<Option<Value>> {
    // Check if the object has the tag — either as instance attr or class attr.
    let Ok(tag_attr) = obj.getattr("_prompt_template_tag") else {
        return Ok(None);
    };

    let tag_str: String = tag_attr.extract().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("_prompt_template_tag must be a string")
    })?;

    let mut map = HashMap::new();
    map.insert(ENUM_TAG_KEY.to_string(), Value::Str(tag_str));

    // Extract payload fields — this MUST succeed if tag exists.
    let fields_attr = obj.getattr("_prompt_template_fields").map_err(|_| {
        pyo3::exceptions::PyAttributeError::new_err(
            "object has _prompt_template_tag but missing _prompt_template_fields",
        )
    })?;

    // _prompt_template_fields may be a property returning a dict or a stored dict.
    let fields_dict = fields_attr.downcast::<PyDict>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("_prompt_template_fields must return a dict")
    })?;

    for (k, v) in fields_dict.iter() {
        let key: String = k.extract()?;
        map.insert(key, py_to_value(&v)?);
    }

    Ok(Some(Value::Dict(map)))
}

/// Try to convert a `enum.Enum` member (has `_name_` attribute).
///
/// Returns `Ok(None)` if the object isn't an enum member,
/// `Ok(Some(value))` on success, or `Err` on conversion failure.
fn try_convert_enum_member(obj: &Bound<'_, PyAny>) -> PyResult<Option<Value>> {
    let Ok(name_attr) = obj.getattr("_name_") else {
        return Ok(None);
    };

    let name: String = name_attr
        .extract()
        .map_err(|_| pyo3::exceptions::PyTypeError::new_err("enum _name_ must be a string"))?;

    // If the enum member's value is a dict, treat as struct variant.
    let Ok(value_attr) = obj.getattr("_value_") else {
        return Ok(Some(Value::Str(name)));
    };

    match value_attr.downcast::<PyDict>() {
        Ok(dict) => {
            let mut map = HashMap::with_capacity(dict.len() + 1);
            map.insert(ENUM_TAG_KEY.to_string(), Value::Str(name));
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                map.insert(key, py_to_value(&v)?);
            }
            Ok(Some(Value::Dict(map)))
        }
        Err(_) => Ok(Some(Value::Str(name))),
    }
}

/// Try to convert an object with `__dict__` (dataclass-like).
///
/// Returns `Ok(None)` if the object doesn't have `__dict__` or it's not a dict.
fn try_convert_dict_object(obj: &Bound<'_, PyAny>) -> PyResult<Option<Value>> {
    let Ok(dict_attr) = obj.getattr("__dict__") else {
        return Ok(None);
    };

    match dict_attr.downcast::<PyDict>() {
        Ok(dict) => py_dict_to_value(dict).map(Some),
        Err(_) => Ok(None),
    }
}

/// Convert a Python dict to `Value::Dict`.
fn py_dict_to_value(dict: &Bound<'_, PyDict>) -> PyResult<Value> {
    let mut map = HashMap::with_capacity(dict.len());
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        map.insert(key, py_to_value(&v)?);
    }
    Ok(Value::Dict(map))
}

/// Convert a template [`Value`] back to a Python object.
///
/// # Errors
///
/// Returns a `PyErr` if allocation fails.
pub(crate) fn value_to_py(py: Python<'_>, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Str(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        Value::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any().unbind()),
        Value::Int(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        Value::Float(f) => Ok(f.into_pyobject(py)?.into_any().unbind()),
        Value::List(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(value_to_py(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        Value::Dict(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, value_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
        Value::Tmpl(_) => Err(pyo3::exceptions::PyTypeError::new_err(
            "cannot convert template value to Python object",
        )),
    }
}
