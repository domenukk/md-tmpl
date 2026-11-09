//! Python source-code generation from template frontmatter declarations.

use std::fmt::Write;

use md_tmpl::{VarDecl, VarType, VariantDecl, to_pascal_case};
use pyo3::prelude::*;

use super::vartype_to_python_annotation;

// ---------------------------------------------------------------------------
// Source code generation (for static type checkers)
// ---------------------------------------------------------------------------

/// Generate Python source code with typed classes for a template file.
///
/// Returns source code that can be written to a `.py` file for mypy/pyright.
/// Uses `@dataclass` for model classes and `Variants` subclasses for enums.
///
/// The generated params class has a typed `render()` method, so type checkers
/// catch missing or mistyped arguments at analysis time.
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

    // Check what imports we need.
    let needs_optional = decls_need_optional(decls);
    let needs_field = decls_need_field(decls);

    let mut out = String::new();
    writeln!(out, "\"\"\"Auto-generated typed stubs for {path}.").expect("write to String");
    writeln!(out).expect("write to String");
    writeln!(
        out,
        "Do not edit — regenerate with ``generate_types_source()``."
    )
    .expect("write to String");
    writeln!(out, "\"\"\"").expect("write to String");
    writeln!(out, "from __future__ import annotations").expect("write to String");
    writeln!(out).expect("write to String");

    // Build import line for dataclasses.
    if needs_field {
        writeln!(out, "from dataclasses import dataclass, field").expect("write to String");
    } else {
        writeln!(out, "from dataclasses import dataclass").expect("write to String");
    }

    // Build typing imports.
    let mut typing_imports = vec!["Any"];
    if needs_optional {
        typing_imports.push("Optional");
    }
    writeln!(out, "from typing import {}", typing_imports.join(", ")).expect("write to String");
    writeln!(out).expect("write to String");
    writeln!(out, "from md_tmpl import Template, Variants").expect("write to String");
    writeln!(out).expect("write to String");

    // Collect nested type definitions first.
    let mut nested_defs: Vec<GeneratedTypeDef> = Vec::new();
    let mut enum_aliases: Vec<(Vec<VariantDecl>, String)> = Vec::new();

    for (alias_name, var_type) in tmpl.type_aliases() {
        if let VarType::Enum(variants) = var_type {
            enum_aliases.push((variants.clone(), to_pascal_case(alias_name)));
        }
    }

    for decl in decls {
        source_gen_types_for_decl(
            &params_class_name,
            decl,
            &mut nested_defs,
            &mut enum_aliases,
        );
    }

    // Generate types for explicit type aliases from the `types:` block.
    let existing_names: std::collections::HashSet<String> =
        nested_defs.iter().map(|d| d.class_name.clone()).collect();

    for (alias_name, var_type) in tmpl.type_aliases() {
        if matches!(var_type, VarType::List(_)) {
            continue;
        }
        let class_name = to_pascal_case(alias_name);
        if existing_names.contains(&class_name) {
            continue;
        }
        let synthetic_decl = VarDecl {
            name: alias_name.clone(),
            var_type: var_type.clone(),
            default_value: None,
        };
        source_gen_types_for_decl("", &synthetic_decl, &mut nested_defs, &mut enum_aliases);
    }

    deduplicate_nested_defs(&mut nested_defs);

    // Write nested types before the params class (forward reference order).
    for def in &nested_defs {
        writeln!(out, "{}", def.source).expect("write to String");
    }

    // Write the params class.
    source_gen_params_class(&mut out, &params_class_name, decls, path, &enum_aliases);

    // Write __all__.
    // Also export nested type names.
    let nested_names: Vec<String> = nested_defs.iter().map(|d| d.class_name.clone()).collect();
    let mut all_names: Vec<String> = vec![params_class_name.clone()];
    all_names.extend(nested_names);

    writeln!(out).expect("write to String");
    let quoted: Vec<String> = all_names.iter().map(|n| format!("\"{n}\"")).collect();
    writeln!(out, "__all__ = [{}]", quoted.join(", ")).expect("write to String");

    Ok(out)
}

struct GeneratedTypeDef {
    class_name: String,
    source: String,
}

/// Recursively generate Python source definitions for a single declaration.
fn source_gen_types_for_decl(
    parent_name: &str,
    decl: &VarDecl,
    out: &mut Vec<GeneratedTypeDef>,
    enum_aliases: &mut Vec<(Vec<VariantDecl>, String)>,
) {
    match &decl.var_type {
        VarType::Enum(variants) => {
            let enum_name =
                if let Some((_, existing)) = enum_aliases.iter().find(|(v, _)| v == variants) {
                    existing.clone()
                } else {
                    let name = to_pascal_case(&decl.name);
                    enum_aliases.push((variants.clone(), name.clone()));
                    name
                };
            out.push(GeneratedTypeDef {
                class_name: enum_name.clone(),
                source: source_gen_enum_class(&enum_name, variants, enum_aliases),
            });
        }
        VarType::List(fields) if !fields.is_empty() => {
            let item_name = format!("{parent_name}{}Item", to_pascal_case(&decl.name));
            // Recurse for nested types within list items first.
            for field in fields {
                source_gen_types_for_decl(&item_name, field, out, enum_aliases);
            }
            out.push(GeneratedTypeDef {
                class_name: item_name.clone(),
                source: source_gen_model_class(&item_name, fields, enum_aliases),
            });
        }
        VarType::Struct(fields) if !fields.is_empty() => {
            let struct_name = to_pascal_case(&decl.name);
            for field in fields {
                source_gen_types_for_decl(&struct_name, field, out, enum_aliases);
            }
            out.push(GeneratedTypeDef {
                class_name: struct_name.clone(),
                source: source_gen_model_class(&struct_name, fields, enum_aliases),
            });
        }
        VarType::Option(inner) => {
            let inner_decl = VarDecl {
                name: decl.name.clone(),
                var_type: (**inner).clone(),
                default_value: None,
            };
            source_gen_types_for_decl(parent_name, &inner_decl, out, enum_aliases);
        }
        _ => {}
    }
}

/// Generate Python source for a `Variants` enum subclass.
pub(super) fn source_gen_enum_class(
    name: &str,
    variants: &[VariantDecl],
    enum_aliases: &[(Vec<VariantDecl>, String)],
) -> String {
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
                        vartype_to_python_source_annotation(
                            &f.var_type,
                            name,
                            &f.name,
                            enum_aliases
                        )
                    )
                })
                .collect();
            writeln!(s, "    {} = {{{}}}", var.name, fields.join(", ")).expect("write to String");
        }
    }
    s
}

/// Generate Python source for a `@dataclass` model class.
pub(super) fn source_gen_model_class(
    name: &str,
    fields: &[VarDecl],
    enum_aliases: &[(Vec<VariantDecl>, String)],
) -> String {
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
            vartype_to_python_source_annotation(&f.var_type, name, &f.name, enum_aliases)
        )
        .expect("write to String");
    }
    s
}

/// Generate the params `@dataclass` with a typed `render()` method.
pub(super) fn source_gen_params_class(
    out: &mut String,
    name: &str,
    decls: &[VarDecl],
    template_path: &str,
    enum_aliases: &[(Vec<VariantDecl>, String)],
) {
    writeln!(out, "@dataclass").expect("write to String");
    writeln!(out, "class {name}:").expect("write to String");
    writeln!(
        out,
        "    \"\"\"Typed parameters for template ``{template_path}``.\"\"\""
    )
    .expect("write to String");

    if decls.is_empty() {
        writeln!(out).expect("write to String");
        // Still need render() even with no params.
        source_gen_render_method(out, template_path);
        return;
    }

    writeln!(out).expect("write to String");

    // Fields without defaults first, then fields with defaults (dataclass rule).
    let (required, optional): (Vec<&VarDecl>, Vec<&VarDecl>) =
        decls.iter().partition(|d| d.default_value.is_none());

    for d in &required {
        writeln!(
            out,
            "    {}: {}",
            d.name,
            vartype_to_python_source_annotation(&d.var_type, name, &d.name, enum_aliases)
        )
        .expect("write to String");
    }

    for d in &optional {
        let ann = vartype_to_python_source_annotation(&d.var_type, name, &d.name, enum_aliases);
        let Some(default_val) = d.default_value.as_ref() else {
            continue;
        };
        let default_repr = default_to_python_repr(default_val);
        writeln!(
            out,
            "    {}: {} = field(default={})",
            d.name, ann, default_repr
        )
        .expect("write to String");
    }

    writeln!(out).expect("write to String");
    source_gen_render_method(out, template_path);
}

/// Emit the `render()` method for the params dataclass.
fn source_gen_render_method(out: &mut String, template_path: &str) {
    writeln!(
        out,
        "    def render(self, template: Template | None = None) -> str:"
    )
    .expect("write to String");
    writeln!(
        out,
        "        \"\"\"Render this params object into its template.\"\"\""
    )
    .expect("write to String");
    writeln!(out, "        if template is None:").expect("write to String");
    writeln!(
        out,
        "            template = Template.from_file(\"{template_path}\")"
    )
    .expect("write to String");
    writeln!(out, "        import dataclasses").expect("write to String");
    writeln!(
        out,
        "        return template.render_dict(dataclasses.asdict(self))"
    )
    .expect("write to String");
}

/// Check if any declaration uses `Option<T>`.
pub(super) fn decls_need_optional(decls: &[VarDecl]) -> bool {
    decls
        .iter()
        .any(|d| matches!(&d.var_type, VarType::Option(_)))
}

/// Check if any declaration has a default value (needs `field(default=...)`).
pub(super) fn decls_need_field(decls: &[VarDecl]) -> bool {
    decls.iter().any(|d| d.default_value.is_some())
}

/// Convert a default [`Value`] to its Python literal repr string.
pub(super) fn default_to_python_repr(val: &md_tmpl::Value) -> String {
    use md_tmpl::Value;
    match val {
        Value::Str(s) => format!("{s:?}"),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            let s = f.to_string();
            if s.contains('.') { s } else { format!("{s}.0") }
        }
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::List(items) => {
            if items.is_empty() {
                "[]".into()
            } else {
                let elems: Vec<String> = items.iter().map(default_to_python_repr).collect();
                format!("[{}]", elems.join(", "))
            }
        }
        Value::Struct(map) => {
            if map.is_empty() {
                "{}".into()
            } else {
                let entries: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{k:?}: {}", default_to_python_repr(v)))
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
        }
        // None and Tmpl can't be represented as Python literals.
        Value::None | Value::Tmpl(_) => "None".into(),
    }
}

fn deduplicate_nested_defs(nested_defs: &mut Vec<GeneratedTypeDef>) {
    let mut seen_classes = std::collections::HashSet::new();
    nested_defs.retain(|def| seen_classes.insert(def.class_name.clone()));
}

/// Python annotation for source-code generation (reuses `enum_aliases` when present).
fn vartype_to_python_source_annotation(
    vt: &VarType,
    parent_name: &str,
    field_name: &str,
    enum_aliases: &[(Vec<VariantDecl>, String)],
) -> String {
    match vt {
        VarType::Enum(variants) => match enum_aliases.iter().find(|(v, _)| v == variants) {
            Some((_, alias_name)) => alias_name.clone(),
            None => to_pascal_case(field_name),
        },
        VarType::Option(inner) => format!(
            "{} | None",
            vartype_to_python_source_annotation(inner, parent_name, field_name, enum_aliases)
        ),
        _ => vartype_to_python_annotation(vt, parent_name, field_name),
    }
}
