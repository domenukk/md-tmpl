//! Dynamic Python type generation from template frontmatter declarations.
//!
//! Reads [`VarDecl`] / [`VarType`] / [`VariantDecl`] from a parsed template
//! and generates Python classes at runtime using the strongly-typed
//! [`PyClassDef`](crate::pyclass_builder::PyClassDef) builder.

use std::fmt::Write;

use md_tmpl::{VarDecl, VarType, VariantDecl, to_pascal_case};
use pyo3::{Py, prelude::*, types::PyDict};

use crate::pyclass_builder::{ClassAttr, Field, PyClassDef, PyMethodDef};

/// Generate Python type classes for a template file.
///
/// Returns a dict mapping class names to their generated Python classes.
/// Called from the Python-side import hook and `template()` helper.
#[pyfunction]
pub(crate) fn generate_types_for_template(py: Python<'_>, path: &str) -> PyResult<Py<PyAny>> {
    let tmpl = crate::template::load_template(path)?;
    let decls = tmpl.inner().declarations();
    let result = PyDict::new(py);

    // Derive a base name from the file stem.
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Template");
    let base_name = stem.strip_suffix(".tmpl").unwrap_or(stem);
    let params_class_name = to_pascal_case(base_name);

    // Generate nested types first, then the top-level params class.
    let mut generated_types: Vec<(String, Py<PyAny>)> = Vec::new();
    for decl in decls {
        generate_types_for_decl(py, &params_class_name, decl, &mut generated_types)?;
    }

    // Generate types for explicit type aliases from the `types:` block.
    generate_types_for_aliases(py, &tmpl, &mut generated_types)?;

    // Build the params class — pass generated types so they're in scope for
    // type annotations (e.g. `outcome: Outcome`) in the __init__ signature.
    let params_cls = build_params_class(py, &params_class_name, decls, path, &generated_types)?;
    result.set_item(&params_class_name, &params_cls)?;

    for (name, cls) in &generated_types {
        result.set_item(name, cls)?;
    }

    Ok(result.into_any().unbind())
}

/// Recursively generate Python types for a single declaration.
fn generate_types_for_decl(
    py: Python<'_>,
    parent_name: &str,
    decl: &VarDecl,
    out: &mut Vec<(String, Py<PyAny>)>,
) -> PyResult<()> {
    match &decl.var_type {
        VarType::Enum(variants) => {
            let enum_name = to_pascal_case(&decl.name);
            let cls = build_enum_class(py, &enum_name, variants)?;
            out.push((enum_name, cls));
        }
        VarType::List(fields) if !fields.is_empty() => {
            let item_name = format!("{parent_name}{}Item", to_pascal_case(&decl.name));
            let cls = build_model_class(py, &item_name, fields)?;
            out.push((item_name.clone(), cls));
            for field in fields {
                generate_types_for_decl(py, &item_name, field, out)?;
            }
        }
        VarType::Struct(fields) if !fields.is_empty() => {
            let dict_name = to_pascal_case(&decl.name);
            let cls = build_model_class(py, &dict_name, fields)?;
            out.push((dict_name.clone(), cls));
            for field in fields {
                generate_types_for_decl(py, &dict_name, field, out)?;
            }
        }
        // Scalar types and empty compound types need no Python class generation.
        VarType::Str
        | VarType::Bool
        | VarType::Int
        | VarType::Float
        | VarType::Tmpl(_)
        | VarType::List(_)
        | VarType::Struct(_) => {}
        // Option<T>: generate types for the inner type.
        VarType::Option(inner) => {
            let inner_decl = VarDecl {
                name: decl.name.clone(),
                var_type: (**inner).clone(),
                default_value: None,
            };
            generate_types_for_decl(py, parent_name, &inner_decl, out)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Enum class generation
// ---------------------------------------------------------------------------

/// Build a Python enum class from variant declarations.
///
/// Unit variants become sentinel attributes on a shared `_Variant` inner class.
/// Struct variants become **distinct nested classes** with `__match_args__`
/// for Python 3.10+ pattern matching.
fn build_enum_class(py: Python<'_>, name: &str, variants: &[VariantDecl]) -> PyResult<Py<PyAny>> {
    let has_unit_variants = variants.iter().any(|v| v.fields.is_empty());

    // Build docstring.
    let mut doc = String::from("Enum type generated from template frontmatter.\n\n    Variants:\n");
    for var in variants {
        if var.fields.is_empty() {
            writeln!(
                doc,
                "        {vn}: Unit variant (use as ``{name}.{vn}``).",
                vn = var.name
            )
            .expect("write to String");
        } else {
            let fields: Vec<String> = var
                .fields
                .iter()
                .map(|f| format!("{}={}", f.name, f.var_type))
                .collect();
            writeln!(
                doc,
                "        {}({}): Struct variant.",
                var.name,
                fields.join(", ")
            )
            .expect("write to String");
        }
    }

    let mut cls = PyClassDef::build(name).doc(doc);

    // _Variant inner class for unit sentinels (only if needed).
    if has_unit_variants {
        let tag_field = Field::new("tag", "str");
        let unit_sentinel = PyClassDef::build("_Variant")
            .doc("A unit variant sentinel. Compared by tag value.")
            .slots(vec![
                Field::new("_md_tmpl_tag", "str"),
                Field::new("_md_tmpl_fields", "dict"),
            ])
            .method(
                PyMethodDef::builder()
                    .name("__init__")
                    .params(vec![tag_field])
                    .body(vec![
                        "self._md_tmpl_tag = tag".into(),
                        "self._md_tmpl_fields = {}".into(),
                    ])
                    .build(),
            )
            .method(
                PyMethodDef::builder()
                    .name("__repr__")
                    .return_annotation("str")
                    .body(vec!["return self._md_tmpl_tag".into()])
                    .build(),
            )
            .method(
                PyMethodDef::builder()
                    .name("__eq__")
                    .params(vec![Field::new("other", "")])
                    .return_annotation("bool")
                    .body(vec![
                        "if not isinstance(other, type(self)):".into(),
                        "    return NotImplemented".into(),
                        "return self._md_tmpl_tag == other._md_tmpl_tag".into(),
                    ])
                    .build(),
            )
            .method(
                PyMethodDef::builder()
                    .name("__hash__")
                    .return_annotation("int")
                    .body(vec!["return hash(self._md_tmpl_tag)".into()])
                    .build(),
            );

        cls = cls.inner_class(unit_sentinel);
    }

    // Generate each variant as either a sentinel attribute or a nested class.
    for var in variants {
        if var.fields.is_empty() {
            cls = cls.attr(ClassAttr::new(
                &var.name,
                format!("_Variant('{}')", var.name),
            ));
        } else {
            let struct_cls = build_struct_variant_def(&var.name, &var.fields);
            cls = cls.inner_class(struct_cls);
        }
    }

    cls.exec(py)
}

/// Build a [`PyClassDef`] for a struct variant with `__match_args__`.
fn build_struct_variant_def(name: &str, fields: &[VarDecl]) -> PyClassDef {
    let typed_fields: Vec<Field> = fields
        .iter()
        .map(|f| {
            Field::new(
                &f.name,
                vartype_to_python_annotation(&f.var_type, name, &f.name),
            )
        })
        .collect();

    let field_sig = typed_fields
        .iter()
        .map(|f| format!("{}: {}", f.name, f.annotation))
        .collect::<Vec<_>>()
        .join(", ");

    PyClassDef::build(name)
        .doc(format!("Struct variant: {name}({field_sig})."))
        .match_args(typed_fields.clone())
        .slots(typed_fields.clone())
        .attr(ClassAttr::new("_md_tmpl_tag", format!("'{name}'")))
        .with_init(&typed_fields)
        .with_fields_property(&typed_fields)
        .with_repr(name, &typed_fields)
        .with_eq(&typed_fields)
        .with_hash(name, &typed_fields)
}

// ---------------------------------------------------------------------------
// Model class generation
// ---------------------------------------------------------------------------

/// Build a Python model class (like a dataclass) from field declarations.
fn build_model_class(py: Python<'_>, name: &str, fields: &[VarDecl]) -> PyResult<Py<PyAny>> {
    let typed_fields: Vec<Field> = fields
        .iter()
        .map(|f| {
            Field::new(
                &f.name,
                vartype_to_python_annotation(&f.var_type, name, &f.name),
            )
        })
        .collect();

    let mut doc = String::from("Model type generated from template frontmatter.\n\n    Fields:\n");
    for f in &typed_fields {
        writeln!(doc, "        {}: {}", f.name, f.annotation).expect("write to String");
    }

    PyClassDef::build(name)
        .doc(doc)
        .slots(typed_fields.clone())
        .with_init(&typed_fields)
        .with_repr(name, &typed_fields)
        .with_eq(&typed_fields)
        .with_dict_property(&typed_fields)
        .exec(py)
}

// ---------------------------------------------------------------------------
// Params class generation
// ---------------------------------------------------------------------------

/// Build the top-level params class with a `render()` method.
fn build_params_class(
    py: Python<'_>,
    name: &str,
    decls: &[VarDecl],
    template_path: &str,
    generated_types: &[(String, Py<PyAny>)],
) -> PyResult<Py<PyAny>> {
    let typed_fields: Vec<Field> = decls
        .iter()
        .map(|d| {
            Field::new(
                &d.name,
                vartype_to_python_annotation(&d.var_type, name, &d.name),
            )
        })
        .collect();

    let mut doc = format!("Typed parameters for template '{template_path}'.\n\n    Parameters:\n");
    for f in &typed_fields {
        writeln!(doc, "        {}: {}", f.name, f.annotation).expect("write to String");
    }

    // Build render() method body.
    let kwarg_items: Vec<String> = decls
        .iter()
        .map(|d| format!("'{n}': self.{n}", n = d.name))
        .collect();
    let render_body = vec![
        "from md_tmpl._md_tmpl import Template as _NativeTemplate".into(),
        "if template is None:".into(),
        format!("    template = _NativeTemplate.from_file('{template_path}')"),
        format!("_kwargs = {{{}}}", kwarg_items.join(", ")),
        "return template.render_dict(_kwargs)".into(),
    ];

    // Build init body with default handling.
    let init_body: Vec<String> = decls
        .iter()
        .map(|d| format!("self.{n} = {n}", n = d.name))
        .collect();

    // Custom init with optional defaults.
    let init_params: Vec<Field> = decls
        .iter()
        .map(|d| {
            let annotation = vartype_to_python_annotation(&d.var_type, name, &d.name);
            Field::new(&d.name, annotation)
        })
        .collect();

    // __repr__
    let repr_parts: Vec<String> = decls
        .iter()
        .map(|d| format!("{}={{self.{}!r}}", d.name, d.name))
        .collect();

    let cls = PyClassDef::build(name)
        .doc(doc)
        .attr(ClassAttr::new(
            "_template_path",
            format!("'{template_path}'"),
        ))
        .slots(typed_fields.clone())
        .method(
            PyMethodDef::builder()
                .name("__init__")
                .params(init_params)
                .body(init_body)
                .build(),
        )
        .method(
            PyMethodDef::builder()
                .name("__repr__")
                .return_annotation("str")
                .body(vec![format!("return f'{name}({})'", repr_parts.join(", "))])
                .build(),
        )
        .method(
            PyMethodDef::builder()
                .name("render")
                .params(vec![Field::new("template", "")])
                .return_annotation("str")
                .doc("Render this params object into a template.")
                .body(render_body)
                .build(),
        )
        .with_dict_property(&typed_fields);

    cls.exec_with_locals(py, Some(generated_types))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a `VarType` to a Python type annotation string.
///
/// For compound types (struct, list, enum), the annotation references the
/// actual generated class name derived from `parent_name` and `field_name`.
pub(super) fn vartype_to_python_annotation(
    vt: &VarType,
    parent_name: &str,
    field_name: &str,
) -> String {
    match vt {
        VarType::Str => "str".into(),
        VarType::Bool => "bool".into(),
        VarType::Int => "int".into(),
        VarType::Float => "float".into(),
        VarType::List(fields) if fields.is_empty() => "list".into(),
        VarType::List(_) => format!("list[{}{}Item]", parent_name, to_pascal_case(field_name)),
        VarType::Struct(fields) if fields.is_empty() => "dict".into(),
        VarType::Struct(_) | VarType::Enum(_) => to_pascal_case(field_name),
        VarType::Tmpl(_) => "object".into(),
        VarType::Option(inner) => {
            let inner_ann = vartype_to_python_annotation(inner, parent_name, field_name);
            format!("Optional[{inner_ann}]")
        }
    }
}

/// Generate Python types for explicit `types:` block aliases.
///
/// For each type alias that maps to a compound type (enum, list, dict),
/// generate a corresponding Python class if one hasn't already been
/// generated by the param-based type generation.
fn generate_types_for_aliases(
    py: Python<'_>,
    tmpl: &crate::template::PyTemplate,
    out: &mut Vec<(String, Py<PyAny>)>,
) -> PyResult<()> {
    let existing_names: std::collections::HashSet<String> =
        out.iter().map(|(name, _)| name.clone()).collect();
    for (alias_name, var_type) in tmpl.type_aliases() {
        let class_name = to_pascal_case(alias_name);
        if existing_names.contains(&class_name) {
            continue; // Already generated by param-based codegen.
        }
        let synthetic_decl = VarDecl {
            name: alias_name.clone(),
            var_type: var_type.clone(),
            default_value: None,
        };
        generate_types_for_decl(py, "", &synthetic_decl, out)?;
    }
    Ok(())
}

mod source_gen;
pub(crate) use source_gen::*;
#[cfg(test)]
use source_gen::{
    decls_need_field, decls_need_optional, default_to_python_repr, source_gen_enum_class,
    source_gen_model_class, source_gen_params_class,
};

#[cfg(test)]
mod tests;
