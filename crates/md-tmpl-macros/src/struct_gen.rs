use quote::{format_ident, quote};

use crate::{
    codegen::{codegen_value_as_rust_literal, is_scalar},
    crate_path,
    type_gen::{
        enum_value_setter, typed_dict_codegen, typed_enum_codegen, typed_list_codegen,
        typed_option_codegen,
    },
};

/// A closure that generates a context-setter statement from a value expression.
pub(crate) type SetterFn = Box<dyn Fn(proc_macro2::TokenStream, &str) -> proc_macro2::TokenStream>;

/// Where the template source came from — determines doc strings and the shape
/// of the generated `render()` method.
pub(crate) enum StructGenSource<'a> {
    /// Struct lives inside a module that has a `template()` accessor.
    ///
    /// `render()` takes no template argument and calls `super::template()`
    /// internally. A separate `render_reloaded(tmpl)` method is emitted for
    /// hot-reload use cases.
    ///
    /// Dep tracking is handled by the enclosing module — `generate_struct_tokens`
    /// does NOT emit `include_str!` for this variant.
    Module {
        /// Path shown in doc comments (may be a file path or `<inline>`).
        doc_path: &'a str,
    },
}

/// Generate the `render` / `render_reloaded` method tokens for the impl block.
fn render_method_tokens() -> proc_macro2::TokenStream {
    let cp = crate_path();
    quote! {
        /// Render using the embedded compile-time template.
        ///
        /// This calls the sibling [`template()`] function internally.
        /// For hot-reload scenarios where you load a template from disk
        /// at runtime, use [`render_reloaded()`](Self::render_reloaded)
        /// instead.
        ///
        /// # Errors
        ///
        /// Returns [`TemplateError`] if rendering fails.
        pub fn render(&self) -> ::core::result::Result<#cp::__private::String, #cp::TemplateError> {
            self.render_reloaded(template())
        }

        /// Validate a reloaded template and render with this struct's fields.
        ///
        /// Use this when hot-reloading a template from disk to ensure the
        /// reloaded version is still compatible with this compiled struct.
        ///
        /// # Errors
        ///
        /// Returns [`TemplateError`] if validation or rendering fails.
        pub fn render_reloaded(&self, tmpl: &#cp::Template) -> ::core::result::Result<#cp::__private::String, #cp::TemplateError> {
            Self::validate_template(tmpl)?;
            let ctx = self.to_context();
            tmpl.render_ctx(&ctx)
        }
    }
}

/// Generate the struct definition and impl block from frontmatter declarations.
///
/// `imported_type_paths` maps a param name to the fully-qualified Rust path of
/// an imported enum type (see [`build_imported_type_paths`]). Such fields
/// reference the imported type directly (via a `pub type` alias for backward
/// compatibility) instead of emitting a duplicate per-template enum.
///
/// [`build_imported_type_paths`]: crate::build_imported_type_paths
pub(crate) fn generate_struct_tokens(
    frontmatter: &md_tmpl_core::Frontmatter,
    struct_name: &syn::Ident,
    source: &StructGenSource<'_>,
    imported_type_paths: &std::collections::HashMap<String, proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    let struct_name_str = struct_name.to_string();
    let mut sub_structs = Vec::new();
    let mut fields = Vec::new();
    let mut set_stmts = Vec::new();
    let mut expected_vars = Vec::new();

    for decl in &frontmatter.declarations {
        let field_name = crate::make_ident(&decl.name);
        let var_name_str = &decl.name;
        let (field_type, field_set) = if let Some(path) = imported_type_paths.get(&decl.name) {
            // Reference the imported enum directly. Emit a `pub type` alias named
            // like the enum this template would otherwise have generated, so any
            // external references (and generated docs) keep working.
            let alias_ident = format_ident!(
                "{struct_name_str}{}",
                md_tmpl_core::to_pascal_case(&decl.name)
            );
            sub_structs.push(quote! {
                /// Alias to an imported enum type (shared across templates).
                pub type #alias_ident = #path;
            });
            (quote! { #alias_ident }, enum_value_setter())
        } else {
            var_type_to_rust(
                &decl.var_type,
                &struct_name_str,
                &decl.name,
                &mut sub_structs,
            )
        };

        let builder_attrs = builder_field_attrs(&decl.var_type);
        let rename_attr = crate::serde_rename_attr(&decl.name);
        fields.push(quote! { #rename_attr #builder_attrs pub #field_name: #field_type });
        set_stmts.push(field_set(quote! { self.#field_name }, var_name_str));
        expected_vars.push(var_name_str.clone());
    }

    let const_decls =
        generate_const_decl_tokens(&frontmatter.consts, &struct_name_str, &mut sub_structs);

    let expected_var_lits: Vec<_> = expected_vars.iter().map(|s| quote! { #s }).collect();
    let expected_count = expected_vars.len();

    let StructGenSource::Module { doc_path } = source;
    let path_for_docs = (*doc_path).to_string();
    let doc_attrs = build_struct_docs(frontmatter, &path_for_docs);

    let has_tmpl_fields = frontmatter
        .declarations
        .iter()
        .any(|d| matches!(d.var_type, md_tmpl_core::VarType::Tmpl(_)));
    let derive_attrs = struct_derive_attrs(has_tmpl_fields);

    let render_methods = render_method_tokens();
    let cp = crate_path();

    quote! {

        #(#sub_structs)*

        #(#const_decls)*

        #(#doc_attrs)*
        #derive_attrs
        // NOLINT: emitted into generated structs; fields mangled from un-escapable keywords (self/Self/super) become pub `__self` etc. and must stay pub.
        #[allow(clippy::pub_underscore_fields)]
        pub struct #struct_name {
            #(#fields),*
        }

        impl #struct_name {
            /// Expected variable names this struct was generated from.
            const EXPECTED_VARS: [&'static str; #expected_count] = [#(#expected_var_lits),*];

            /// Validate that a template's variable declarations match this struct.
            ///
            /// Use this when hot-reloading a template from disk to ensure the
            /// reloaded version is still compatible with this compiled struct.
            ///
            /// The template body can be edited freely, but the `params:`
            /// block in the frontmatter **must not be changed** — it is part
            /// of the compile-time contract.
            ///
            /// # Errors
            ///
            /// Returns [`TemplateError::DeclarationsMutated`] if the
            /// template's declared variable names don't match the expected
            /// set.
            pub fn validate_template(tmpl: &#cp::Template) -> ::core::result::Result<(), #cp::TemplateError> {
                let mut decl_names: #cp::__private::Vec<&str> = tmpl
                    .declarations()
                    .iter()
                    .map(|d| d.name.as_str())
                    .collect();
                decl_names.sort_unstable();
                let mut expected: #cp::__private::Vec<&str> = Self::EXPECTED_VARS
                    .iter()
                    .copied()
                    .collect();
                expected.sort_unstable();

                if decl_names != expected {
                    let missing: #cp::__private::Vec<&&str> = expected.iter().filter(|v| !decl_names.contains(v)).collect();
                    let extra: #cp::__private::Vec<&&str> = decl_names.iter().filter(|v| !expected.contains(v)).collect();
                    return ::core::result::Result::Err(#cp::TemplateError::DeclarationsMutated {
                        details: #cp::__private::format!(
                            "removed {:?}, added {:?}. \
                             You may edit the template body, but the \
                             frontmatter `params:` block must stay \
                             unchanged",
                            missing, extra
                        ),
                    });
                }
                ::core::result::Result::Ok(())
            }

            /// Convert this struct into a [`Context`](::md_tmpl::Context).
            #[must_use]
            pub fn to_context(&self) -> #cp::Context {
                let mut ctx = #cp::Context::new();
                #(#set_stmts)*
                ctx
            }

            #render_methods
        }
    }
}

/// Generate `const` / `static` declarations for frontmatter `consts:` entries.
pub(crate) fn generate_const_decl_tokens(
    consts: &[md_tmpl_core::VarDecl],
    struct_name_str: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> Vec<proc_macro2::TokenStream> {
    let cp = crate_path();
    let mut const_decls = Vec::new();
    for decl in consts {
        let const_name = decl.name.to_uppercase();
        let const_ident = crate::make_ident(&const_name);
        let (rust_type, _) =
            var_type_to_rust(&decl.var_type, struct_name_str, &decl.name, sub_structs);

        if let Some(v) = &decl.default_value {
            if is_scalar(&decl.var_type) {
                let (final_type, val_tokens) =
                    if let (md_tmpl_core::Value::Str(s), md_tmpl_core::VarType::Str) =
                        (v, &decl.var_type)
                    {
                        (quote! { &'static str }, quote! { #s })
                    } else {
                        let vt = codegen_value_as_rust_literal(
                            v,
                            &decl.var_type,
                            struct_name_str,
                            &decl.name,
                        );
                        (rust_type, vt)
                    };
                const_decls.push(quote! {
                    pub const #const_ident: #final_type = #val_tokens;
                });
            } else {
                let val_tokens =
                    codegen_value_as_rust_literal(v, &decl.var_type, struct_name_str, &decl.name);
                const_decls.push(quote! {
                    pub static #const_ident: #cp::__private::LazyLock<#rust_type> = #cp::__private::LazyLock::new(|| #val_tokens);
                });
            }
        }
    }
    const_decls
}

/// Build doc-comment attributes from frontmatter metadata.
pub(crate) fn build_struct_docs(
    frontmatter: &md_tmpl_core::Frontmatter,
    path_raw: &str,
) -> Vec<proc_macro2::TokenStream> {
    let tmpl_name = match frontmatter.name.as_deref() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => path_raw.to_string(),
    };
    let description = match frontmatter.description.as_deref() {
        Some(desc) if !desc.is_empty() => desc.to_string(),
        _ => String::from("(no description)"),
    };

    let mut doc_lines = vec![
        format!("Parameters for the **{tmpl_name}** template."),
        String::new(),
        description,
        String::new(),
        format!("Source: `{path_raw}`"),
        String::new(),
    ];

    if !frontmatter.declarations.is_empty() {
        doc_lines.push("# Variables".to_string());
        doc_lines.push(String::new());
        doc_lines.push("| Name | Type |".to_string());
        doc_lines.push("|------|------|".to_string());
        for decl in &frontmatter.declarations {
            doc_lines.push(format!("| `{}` | `{}` |", decl.name, decl.var_type));
        }
        doc_lines.push(String::new());
    }

    doc_lines.push("Fill in the fields and call [`render()`](Self::render) to produce".to_string());
    doc_lines.push("the rendered template output. Supports hot-reload via".to_string());
    doc_lines.push("[`validate_template()`](Self::validate_template).".to_string());

    doc_lines
        .iter()
        .map(|line| quote! { #[doc = #line] })
        .collect()
}

/// Choose derive attributes for generated structs.
///
/// `has_tmpl_fields` — when `true`, serde derives are omitted because
/// `Arc<Template>` does not support `Serialize` / `Deserialize`.
///
/// The `TypedBuilder` derive is **always** emitted, routed through the crate's
/// `__private` re-export (`#cp::__private::TypedBuilder`) so downstream crates
/// get a builder without needing their own `typed-builder` dependency.
///
/// **Feature flag design:** `cfg!(feature = "serde")` checks the *proc-macro
/// crate's own* Cargo feature, **not** the downstream user's. It is
/// intentionally empty in `Cargo.toml` — it carries no dependency of its own.
/// Instead, it acts as a code-generation toggle: when a user enables `serde`
/// on `md-tmpl-macros`, the proc-macro emits `#[derive(Serialize, Deserialize)]`
/// into the generated code, relying on the user's own `serde` dependency to
/// resolve the derive.
pub(crate) fn struct_derive_attrs(has_tmpl_fields: bool) -> proc_macro2::TokenStream {
    let cp = crate_path();
    let use_serde = cfg!(feature = "serde") && !has_tmpl_fields;
    if use_serde {
        quote! {
            #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize, #cp::__private::TypedBuilder)]
            #[builder(crate_module_path = #cp::__private::typed_builder)]
        }
    } else {
        quote! {
            #[derive(Debug, Clone, PartialEq, #cp::__private::TypedBuilder)]
            #[builder(crate_module_path = #cp::__private::typed_builder)]
        }
    }
}

/// Generate field-level builder attributes based on the variable type.
///
/// - `String` fields get `#[builder(setter(into))]` for ergonomic `"str".into()`.
/// - `Vec<…>` fields get `#[builder(default)]` so they default to empty.
/// - All other fields have no special builder attributes.
pub(crate) fn builder_field_attrs(var_type: &md_tmpl_core::VarType) -> proc_macro2::TokenStream {
    use md_tmpl_core::VarType;
    match var_type {
        VarType::Str => quote! { #[builder(setter(into))] },
        VarType::List(_) => quote! { #[builder(default, setter(into))] },
        VarType::Enum(_) if var_type.is_option() => quote! { #[builder(default)] },
        _ => quote! {},
    }
}

/// Map a `VarType` to a Rust type token and a setter closure.
pub(crate) fn var_type_to_rust(
    var_type: &md_tmpl_core::VarType,
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    use md_tmpl_core::VarType;
    let cp = crate_path();

    match var_type {
        VarType::Str => (quote! { String }, simple_setter("as_str()")),
        VarType::Int => (quote! { i64 }, simple_setter("")),
        VarType::Float => (quote! { f64 }, simple_setter("")),
        VarType::Bool => (quote! { bool }, simple_setter("")),
        VarType::List(fields) if !fields.is_empty() => {
            if fields.len() == 1 && fields[0].name.is_empty() {
                let (inner_type, inner_set) =
                    var_type_to_rust(&fields[0].var_type, parent_struct, field_name, sub_structs);
                let cp2 = cp.clone();
                let is_copy_scalar = matches!(
                    fields[0].var_type,
                    VarType::Int | VarType::Float | VarType::Bool
                );
                let is_string = matches!(fields[0].var_type, VarType::Str);
                (
                    quote! { #cp::__private::Vec<#inner_type> },
                    Box::new(move |val, name| {
                        let name_lit = name.to_string();
                        let cp = &cp2;
                        // For scalar Copy types, dereference the iterator item.
                        // For String, clone it. For complex types, use as-is.
                        let item_binding = if is_copy_scalar {
                            quote! { let item = *item_ref; }
                        } else if is_string {
                            quote! { let item = item_ref.clone(); }
                        } else {
                            quote! { let item = item_ref; }
                        };
                        // IMPORTANT: inner_set generates `ctx.set(...)` but inside
                        // the .map() closure we must use `inner_ctx` to avoid borrowing
                        // the outer `ctx` mutably. We use a local binding to shadow `ctx`.
                        let stmts = inner_set(quote! { item }, "item");
                        quote! {
                            ctx.set(#name_lit, #cp::Value::List(#cp::__private::Arc::new(
                                #val.iter().map(|item_ref| {
                                    #item_binding
                                    // Shadow `ctx` with `inner_ctx` so that the inner
                                    // setter's `ctx.set(...)` actually calls `inner_ctx.set(...)`.
                                    let mut ctx = #cp::Context::new();
                                    #stmts
                                    ctx.get("item").cloned().unwrap_or(#cp::Value::Str(#cp::__private::String::new()))
                                }).collect()
                            )));
                        }
                    }),
                )
            } else {
                typed_list_codegen(fields, parent_struct, field_name, sub_structs)
            }
        }
        VarType::List(_) => (quote! { Vec<#cp::Value> }, {
            let cp2 = cp.clone();
            Box::new(move |val, name| {
                let name_lit = name.to_string();
                let cp = &cp2;
                quote! { ctx.set(#name_lit, #cp::Value::List(#cp::__private::Arc::new(#val.clone()))); }
            })
        }),
        VarType::Struct(fields) if !fields.is_empty() => {
            typed_dict_codegen(fields, parent_struct, field_name, sub_structs)
        }
        VarType::Struct(_) => (quote! { #cp::Value }, {
            let cp2 = cp.clone();
            Box::new(move |val, name| {
                let name_lit = name.to_string();
                let _cp = &cp2;
                quote! { ctx.set(#name_lit, #val.clone()); }
            })
        }),
        VarType::Enum(_) if var_type.is_option() => {
            typed_option_codegen(var_type, parent_struct, field_name, sub_structs)
        }
        VarType::Enum(variants) => {
            typed_enum_codegen(variants, parent_struct, field_name, sub_structs)
        }
        VarType::Tmpl(_) => (quote! { #cp::__private::Arc<#cp::Template> }, {
            let cp2 = cp.clone();
            Box::new(move |val, name| {
                let name_lit = name.to_string();
                let cp = &cp2;
                quote! { ctx.set(#name_lit, #cp::Value::Tmpl(#val.clone())); }
            })
        }),
        VarType::Option(inner) => {
            // Desugar to enum-style option codegen
            typed_option_codegen(
                &VarType::Option(inner.clone()),
                parent_struct,
                field_name,
                sub_structs,
            )
        }
    }
}

/// Create a setter that calls `ctx.set(name, val)` or `ctx.set(name, val.as_str())`.
pub(crate) fn simple_setter(suffix: &str) -> SetterFn {
    let suffix = suffix.to_string();
    Box::new(move |val, name| {
        let name_lit = name.to_string();
        if suffix.is_empty() {
            quote! { ctx.set(#name_lit, #val); }
        } else {
            // Only case is .as_str() for String fields.
            quote! { ctx.set(#name_lit, #val.as_str()); }
        }
    })
}
