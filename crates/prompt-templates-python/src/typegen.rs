//! Dynamic Python type generation from template frontmatter declarations.
//!
//! Reads [`VarDecl`] / [`VarType`] / [`VariantDecl`] from a parsed template
//! and generates Python classes at runtime using the strongly-typed
//! [`PyClassDef`](crate::pyclass_builder::PyClassDef) builder.

use std::fmt::Write;

use prompt_templates::{VarDecl, VarType, VariantDecl, to_pascal_case};
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
                Field::new("_prompt_template_tag", "str"),
                Field::new("_prompt_template_fields", "dict"),
            ])
            .method(PyMethodDef {
                name: "__init__".into(),
                params: vec![tag_field],
                return_annotation: None,
                doc: None,
                body: vec![
                    "self._prompt_template_tag = tag".into(),
                    "self._prompt_template_fields = {}".into(),
                ],
            })
            .method(PyMethodDef {
                name: "__repr__".into(),
                params: Vec::new(),
                return_annotation: Some("str".into()),
                doc: None,
                body: vec!["return self._prompt_template_tag".into()],
            })
            .method(PyMethodDef {
                name: "__eq__".into(),
                params: vec![Field::new("other", "")],
                return_annotation: Some("bool".into()),
                doc: None,
                body: vec![
                    "if not isinstance(other, type(self)):".into(),
                    "    return NotImplemented".into(),
                    "return self._prompt_template_tag == other._prompt_template_tag".into(),
                ],
            })
            .method(PyMethodDef {
                name: "__hash__".into(),
                params: Vec::new(),
                return_annotation: Some("int".into()),
                doc: None,
                body: vec!["return hash(self._prompt_template_tag)".into()],
            });

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
        .attr(ClassAttr::new("_prompt_template_tag", format!("'{name}'")))
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
        "from prompt_templates._prompt_templates import Template as _NativeTemplate".into(),
        "if template is None:".into(),
        format!("    template = _NativeTemplate.from_file('{template_path}')"),
        format!("_kwargs = {{{}}}", kwarg_items.join(", ")),
        "_kwargs = {k: v for k, v in _kwargs.items() if v is not None}".into(),
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
        .method(PyMethodDef {
            name: "__init__".into(),
            params: init_params,
            return_annotation: None,
            doc: None,
            body: init_body,
        })
        .method(PyMethodDef {
            name: "__repr__".into(),
            params: Vec::new(),
            return_annotation: Some("str".into()),
            doc: None,
            body: vec![format!("return f'{name}({})'", repr_parts.join(", "))],
        })
        .method(PyMethodDef {
            name: "render".into(),
            params: vec![Field::new("template", "")],
            return_annotation: Some("str".into()),
            doc: Some("Render this params object into a template.".into()),
            body: render_body,
        })
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
fn vartype_to_python_annotation(vt: &VarType, parent_name: &str, field_name: &str) -> String {
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
        generate_types_for_decl(py, &class_name, &synthetic_decl, out)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Source code generation (for static type checkers)
// ---------------------------------------------------------------------------

/// Generate Python source code with typed classes for a template file.
///
/// Returns source code that can be written to a `.py` file for mypy/pyright.
/// Uses `@dataclass` for model classes and `Variants` subclasses for enums.
#[pyfunction]
pub(crate) fn generate_python_source_for_template(path: &str) -> PyResult<String> {
    let tmpl = crate::template::load_template(path)?;
    let decls = tmpl.inner().declarations();

    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Template");
    let base_name = stem.strip_suffix(".tmpl").unwrap_or(stem);
    let params_class_name = to_pascal_case(base_name);

    let mut out = String::new();
    writeln!(out, "from __future__ import annotations").expect("write to String");
    writeln!(out).expect("write to String");
    writeln!(out, "from dataclasses import dataclass").expect("write to String");
    writeln!(out, "from typing import Any").expect("write to String");
    writeln!(out).expect("write to String");
    writeln!(out, "from prompt_templates import Variants").expect("write to String");
    writeln!(out).expect("write to String");

    // Collect nested type definitions first.
    let mut nested_defs: Vec<String> = Vec::new();
    for decl in decls {
        source_gen_types_for_decl(&params_class_name, decl, &mut nested_defs);
    }

    // Write nested types before the params class (forward reference order).
    for def in &nested_defs {
        writeln!(out, "{def}").expect("write to String");
    }

    // Write the params class.
    source_gen_params_class(&mut out, &params_class_name, decls);

    Ok(out)
}

/// Recursively generate Python source definitions for a single declaration.
fn source_gen_types_for_decl(parent_name: &str, decl: &VarDecl, out: &mut Vec<String>) {
    match &decl.var_type {
        VarType::Enum(variants) => {
            let enum_name = to_pascal_case(&decl.name);
            out.push(source_gen_enum_class(&enum_name, variants));
        }
        VarType::List(fields) if !fields.is_empty() => {
            let item_name = format!("{parent_name}{}Item", to_pascal_case(&decl.name));
            // Recurse for nested types within list items first.
            for field in fields {
                source_gen_types_for_decl(&item_name, field, out);
            }
            out.push(source_gen_model_class(&item_name, fields));
        }
        VarType::Struct(fields) if !fields.is_empty() => {
            let struct_name = to_pascal_case(&decl.name);
            for field in fields {
                source_gen_types_for_decl(&struct_name, field, out);
            }
            out.push(source_gen_model_class(&struct_name, fields));
        }
        _ => {}
    }
}

/// Generate Python source for a `Variants` enum subclass.
fn source_gen_enum_class(name: &str, variants: &[VariantDecl]) -> String {
    let mut s = String::new();
    writeln!(s, "class {name}(Variants):").expect("write to String");
    if variants.is_empty() {
        writeln!(s, "    pass").expect("write to String");
        return s;
    }
    for var in variants {
        if var.fields.is_empty() {
            writeln!(s, "    {} = ()", var.name).expect("write to String");
        } else {
            let fields: Vec<String> = var
                .fields
                .iter()
                .map(|f| {
                    format!(
                        "\"{}\": {}",
                        f.name,
                        vartype_to_python_source_annotation(&f.var_type, name, &f.name)
                    )
                })
                .collect();
            writeln!(s, "    {} = {{{}}}", var.name, fields.join(", ")).expect("write to String");
        }
    }
    s
}

/// Generate Python source for a `@dataclass` model class.
fn source_gen_model_class(name: &str, fields: &[VarDecl]) -> String {
    let mut s = String::new();
    writeln!(s, "@dataclass").expect("write to String");
    writeln!(s, "class {name}:").expect("write to String");
    if fields.is_empty() {
        writeln!(s, "    pass").expect("write to String");
        return s;
    }
    for f in fields {
        writeln!(
            s,
            "    {}: {}",
            f.name,
            vartype_to_python_source_annotation(&f.var_type, name, &f.name)
        )
        .expect("write to String");
    }
    s
}

/// Generate the params `@dataclass` with a `render()` method.
fn source_gen_params_class(out: &mut String, name: &str, decls: &[VarDecl]) {
    writeln!(out, "@dataclass").expect("write to String");
    writeln!(out, "class {name}:").expect("write to String");
    if decls.is_empty() {
        writeln!(out, "    pass").expect("write to String");
        return;
    }
    for d in decls {
        writeln!(
            out,
            "    {}: {}",
            d.name,
            vartype_to_python_source_annotation(&d.var_type, name, &d.name)
        )
        .expect("write to String");
    }
}

/// Python annotation for source-code generation (same logic as runtime annotations).
fn vartype_to_python_source_annotation(
    vt: &VarType,
    parent_name: &str,
    field_name: &str,
) -> String {
    // Reuse the same logic as runtime annotations.
    vartype_to_python_annotation(vt, parent_name, field_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_via_core_crate() {
        assert_eq!(to_pascal_case("code_review"), "CodeReview");
        assert_eq!(to_pascal_case("simple_greeting"), "SimpleGreeting");
        assert_eq!(to_pascal_case("task-report"), "TaskReport");
        assert_eq!(to_pascal_case("single"), "Single");
        assert_eq!(to_pascal_case(""), "");
        assert_eq!(to_pascal_case("already_PascalCase"), "AlreadyPascalCase");
    }

    #[test]
    fn vartype_annotations() {
        // Scalar types ignore parent/field context.
        assert_eq!(
            vartype_to_python_annotation(&VarType::Str, "Parent", "field"),
            "str"
        );
        assert_eq!(
            vartype_to_python_annotation(&VarType::Int, "Parent", "field"),
            "int"
        );
        assert_eq!(
            vartype_to_python_annotation(&VarType::Float, "Parent", "field"),
            "float"
        );
        assert_eq!(
            vartype_to_python_annotation(&VarType::Bool, "Parent", "field"),
            "bool"
        );

        // Empty compound types.
        assert_eq!(
            vartype_to_python_annotation(&VarType::List(vec![]), "Parent", "items"),
            "list"
        );
        assert_eq!(
            vartype_to_python_annotation(&VarType::Struct(vec![]), "Parent", "config"),
            "dict"
        );

        // Non-empty list → list[ParentItemsItem].
        assert_eq!(
            vartype_to_python_annotation(
                &VarType::List(vec![VarDecl {
                    name: "x".into(),
                    var_type: VarType::Str,
                    default_value: None,
                }]),
                "Params",
                "items"
            ),
            "list[ParamsItemsItem]"
        );

        // Non-empty struct → PascalCase(field_name).
        assert_eq!(
            vartype_to_python_annotation(
                &VarType::Struct(vec![VarDecl {
                    name: "x".into(),
                    var_type: VarType::Str,
                    default_value: None,
                }]),
                "Params",
                "config"
            ),
            "Config"
        );

        // Enum → PascalCase(field_name).
        assert_eq!(
            vartype_to_python_annotation(&VarType::Enum(vec![]), "Params", "status"),
            "Status"
        );

        // Tmpl stays opaque.
        assert_eq!(
            vartype_to_python_annotation(&VarType::Tmpl(vec![]), "Params", "body"),
            "object"
        );
    }

    #[test]
    fn source_gen_enum() {
        let source = source_gen_enum_class(
            "Status",
            &[
                VariantDecl {
                    name: "Approved".into(),
                    fields: vec![],
                },
                VariantDecl {
                    name: "Rejected".into(),
                    fields: vec![],
                },
                VariantDecl {
                    name: "NeedsChanges".into(),
                    fields: vec![VarDecl {
                        name: "reason".into(),
                        var_type: VarType::Str,
                        default_value: None,
                    }],
                },
            ],
        );
        assert!(source.contains("class Status(Variants):"));
        assert!(source.contains("Approved = ()"));
        assert!(source.contains("Rejected = ()"));
        assert!(source.contains("NeedsChanges = {\"reason\": str}"));
    }

    #[test]
    fn source_gen_model() {
        let source = source_gen_model_class(
            "ReviewItem",
            &[
                VarDecl {
                    name: "name".into(),
                    var_type: VarType::Str,
                    default_value: None,
                },
                VarDecl {
                    name: "score".into(),
                    var_type: VarType::Int,
                    default_value: None,
                },
            ],
        );
        assert!(source.contains("@dataclass"));
        assert!(source.contains("class ReviewItem:"));
        assert!(source.contains("    name: str"));
        assert!(source.contains("    score: int"));
    }

    #[test]
    fn source_gen_params() {
        let mut out = String::new();
        source_gen_params_class(
            &mut out,
            "ReviewParams",
            &[
                VarDecl {
                    name: "reviewer".into(),
                    var_type: VarType::Str,
                    default_value: None,
                },
                VarDecl {
                    name: "score".into(),
                    var_type: VarType::Float,
                    default_value: None,
                },
            ],
        );
        assert!(out.contains("@dataclass"));
        assert!(out.contains("class ReviewParams:"));
        assert!(out.contains("    reviewer: str"));
        assert!(out.contains("    score: float"));
    }
}
