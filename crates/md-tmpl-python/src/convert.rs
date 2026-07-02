//! Bidirectional conversion between Python objects and [`Value`].

use std::sync::Arc;

use hashbrown::{HashMap, HashSet};
use md_tmpl::{Frontmatter, Value};
use pyo3::{
    Py,
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString},
};

/// Tag key used for internally-tagged enum variants in dicts.
/// Must match [`md_tmpl::consts::ENUM_TAG_KEY`].
const ENUM_TAG_KEY: &str = md_tmpl::consts::ENUM_TAG_KEY;

/// Convert a Python object into a template [`Value`].
pub(crate) fn py_to_value(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    let mut visited = HashSet::new();
    py_to_value_inner(obj, &mut visited, 0)
}

fn py_to_value_inner(
    obj: &Bound<'_, PyAny>,
    visited: &mut HashSet<usize>,
    depth: usize,
) -> PyResult<Value> {
    if depth > 256 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "maximum recursion depth exceeded in template parameter",
        ));
    }

    if obj.is_none() {
        return Ok(Value::None);
    }

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
    if obj.is_instance_of::<crate::template::PyTemplate>() {
        let py_template: PyRef<'_, crate::template::PyTemplate> = obj.extract()?;
        return Ok(Value::Tmpl(Arc::new(py_template.inner().clone())));
    }

    let ptr = obj.as_ptr() as usize;
    if !visited.insert(ptr) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "cyclic object detected in template parameter",
        ));
    }

    let res = (|| -> PyResult<Value> {
        if obj.is_instance_of::<PyList>() {
            let list = obj.cast::<PyList>()?;
            let mut items = Vec::with_capacity(list.len());
            for item in list.iter() {
                items.push(py_to_value_inner(&item, visited, depth + 1)?);
            }
            return Ok(Value::List(Arc::new(items)));
        }
        if obj.is_instance_of::<PyDict>() {
            return py_dict_to_value(obj.cast::<PyDict>()?, visited, depth + 1);
        }

        if let Some(value) = try_convert_tagged_variant(obj, visited, depth + 1)? {
            return Ok(value);
        }

        if let Some(value) = try_convert_enum_member(obj, visited, depth + 1)? {
            return Ok(value);
        }

        if let Some(value) = try_convert_dict_object(obj, visited, depth + 1)? {
            return Ok(value);
        }

        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "cannot convert Python type '{}' to template Value",
            obj.get_type().qualname()?
        )))
    })();
    visited.remove(&ptr);
    res
}

fn try_convert_tagged_variant(
    obj: &Bound<'_, PyAny>,
    visited: &mut HashSet<usize>,
    depth: usize,
) -> PyResult<Option<Value>> {
    let Ok(tag_attr) = obj.getattr("_md_tmpl_tag") else {
        return Ok(None);
    };

    let tag_str: String = tag_attr
        .extract()
        .map_err(|_| pyo3::exceptions::PyTypeError::new_err("_md_tmpl_tag must be a string"))?;

    let mut map = HashMap::new();
    map.insert(ENUM_TAG_KEY.to_string(), Value::Str(tag_str));

    let fields_attr = obj.getattr("_md_tmpl_fields").map_err(|_| {
        pyo3::exceptions::PyAttributeError::new_err(
            "object has _md_tmpl_tag but missing _md_tmpl_fields",
        )
    })?;

    let fields_dict = fields_attr.cast::<PyDict>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("_md_tmpl_fields must return a dict")
    })?;

    for (k, v) in fields_dict.iter() {
        let key: String = k.extract()?;
        map.insert(key, py_to_value_inner(&v, visited, depth)?);
    }

    Ok(Some(Value::Struct(Arc::new(map))))
}

fn try_convert_enum_member(
    obj: &Bound<'_, PyAny>,
    visited: &mut HashSet<usize>,
    depth: usize,
) -> PyResult<Option<Value>> {
    let Ok(name_attr) = obj.getattr("_name_") else {
        return Ok(None);
    };

    let name: String = name_attr
        .extract()
        .map_err(|_| pyo3::exceptions::PyTypeError::new_err("enum _name_ must be a string"))?;

    let Ok(value_attr) = obj.getattr("_value_") else {
        return Ok(Some(Value::Str(name)));
    };

    match value_attr.cast::<PyDict>() {
        Ok(dict) => {
            let mut map = HashMap::with_capacity(dict.len() + 1);
            map.insert(ENUM_TAG_KEY.to_string(), Value::Str(name));
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                map.insert(key, py_to_value_inner(&v, visited, depth)?);
            }
            Ok(Some(Value::Struct(Arc::new(map))))
        }
        Err(_) => Ok(Some(Value::Str(name))),
    }
}

fn try_convert_dict_object(
    obj: &Bound<'_, PyAny>,
    visited: &mut HashSet<usize>,
    depth: usize,
) -> PyResult<Option<Value>> {
    let Ok(dict_attr) = obj.getattr("__dict__") else {
        return Ok(None);
    };

    match dict_attr.cast::<PyDict>() {
        Ok(dict) => py_dict_to_value(dict, visited, depth).map(Some),
        Err(_) => Ok(None),
    }
}

fn py_dict_to_value(
    dict: &Bound<'_, PyDict>,
    visited: &mut HashSet<usize>,
    depth: usize,
) -> PyResult<Value> {
    let mut map = HashMap::with_capacity(dict.len());
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        map.insert(key, py_to_value_inner(&v, visited, depth)?);
    }
    Ok(Value::Struct(Arc::new(map)))
}

/// Convert a template [`Value`] back to a Python object.
///
/// # Errors
///
/// Returns a `PyErr` if allocation fails.
pub(crate) fn value_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Str(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        Value::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any().unbind()),
        Value::Int(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        Value::Float(f) => Ok(f.into_pyobject(py)?.into_any().unbind()),
        Value::List(items) => {
            let list = PyList::empty(py);
            for item in items.iter() {
                list.append(value_to_py(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        Value::Struct(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map.iter() {
                dict.set_item(k, value_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
        Value::Tmpl(tmpl) => {
            let inner = (**tmpl).clone();
            let frontmatter = Frontmatter {
                declarations: inner.declarations().to_vec(),
                ..Default::default()
            };
            let py_tmpl = crate::template::PyTemplate::from_inner(inner, frontmatter);
            Ok(Py::new(py, py_tmpl)?.into_any())
        }
        Value::None => Ok(py.None().into_pyobject(py)?.into_any().unbind()),
    }
}
