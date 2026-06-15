use quote::{format_ident, quote};

use crate::{
    codegen::{codegen_value_as_rust_literal, is_scalar},
    type_gen::{typed_dict_codegen, typed_enum_codegen, typed_list_codegen},
};

/// A closure that generates a context-setter statement from a value expression.
pub(crate) type SetterFn = Box<dyn Fn(proc_macro2::TokenStream, &str) -> proc_macro2::TokenStream>;

/// Generate the struct definition and impl block from frontmatter declarations.
pub(crate) fn generate_struct_tokens(
    frontmatter: &prompt_templates::Frontmatter,
    struct_name: &syn::Ident,
    struct_name_str: &str,
    path_raw: &str,
    path_str: &str,
) -> proc_macro2::TokenStream {
    let mut sub_structs = Vec::new();
    let mut fields = Vec::new();
    let mut set_stmts = Vec::new();
    let mut expected_vars = Vec::new();

    for decl in &frontmatter.declarations {
        let field_name = format_ident!("{}", decl.name);
        let var_name_str = &decl.name;
        let (field_type, field_set) = var_type_to_rust(
            &decl.var_type,
            struct_name_str,
            &decl.name,
            &mut sub_structs,
        );

        let builder_attrs = builder_field_attrs(&decl.var_type);
        fields.push(quote! { #builder_attrs pub #field_name: #field_type });
        set_stmts.push(field_set(quote! { self.#field_name }, var_name_str));
        expected_vars.push(var_name_str.clone());
    }

    let const_decls =
        generate_const_decl_tokens(&frontmatter.consts, struct_name_str, &mut sub_structs);

    let expected_var_lits: Vec<_> = expected_vars.iter().map(|s| quote! { #s }).collect();
    let expected_count = expected_vars.len();
    let doc_attrs = build_struct_docs(frontmatter, path_raw);

    let has_tmpl_fields = frontmatter
        .declarations
        .iter()
        .any(|d| matches!(d.var_type, prompt_templates::VarType::Tmpl(_)));
    let derive_attrs = struct_derive_attrs(has_tmpl_fields);

    quote! {
        // Dependency tracking.
        const _: &str = include_str!(#path_str);

        #(#sub_structs)*

        #(#const_decls)*

        #(#doc_attrs)*
        #derive_attrs
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
            pub fn validate_template(tmpl: &::prompt_templates::Template) -> ::std::result::Result<(), ::prompt_templates::TemplateError> {
                let decl_names: ::std::collections::HashSet<&str> = tmpl
                    .declarations()
                    .iter()
                    .map(|d| d.name.as_str())
                    .collect();
                let expected: ::std::collections::HashSet<&str> = Self::EXPECTED_VARS
                    .iter()
                    .copied()
                    .collect();

                if decl_names != expected {
                    let missing: Vec<_> = expected.difference(&decl_names).collect();
                    let extra: Vec<_> = decl_names.difference(&expected).collect();
                    return Err(::prompt_templates::TemplateError::DeclarationsMutated {
                        details: format!(
                            "removed {:?}, added {:?}. \
                             You may edit the template body, but the \
                             frontmatter `params:` block must stay \
                             unchanged",
                            missing, extra
                        ),
                    });
                }
                Ok(())
            }

            /// Convert this struct into a [`Context`](::prompt_templates::Context).
            #[must_use]
            pub fn to_context(&self) -> ::prompt_templates::Context {
                let mut ctx = ::prompt_templates::Context::new();
                #(#set_stmts)*
                ctx
            }

            /// Validate the template and render with this struct's fields.
            ///
            /// # Errors
            ///
            /// Returns [`TemplateError`] if validation or rendering fails.
            pub fn render(&self, tmpl: &::prompt_templates::Template) -> ::std::result::Result<String, ::prompt_templates::TemplateError> {
                Self::validate_template(tmpl)?;
                let ctx = self.to_context();
                tmpl.render(&ctx)
            }
        }
    }
}

/// Generate `const` / `static` declarations for frontmatter `consts:` entries.
pub(crate) fn generate_const_decl_tokens(
    consts: &[prompt_templates::VarDecl],
    struct_name_str: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> Vec<proc_macro2::TokenStream> {
    let mut const_decls = Vec::new();
    for decl in consts {
        let const_name = decl.name.to_uppercase();
        let const_ident = format_ident!("{}", const_name);
        let (rust_type, _) =
            var_type_to_rust(&decl.var_type, struct_name_str, &decl.name, sub_structs);

        if let Some(v) = &decl.default_value {
            if is_scalar(&decl.var_type) {
                let (final_type, val_tokens) =
                    if let (prompt_templates::Value::Str(s), prompt_templates::VarType::Str) =
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
                    pub static #const_ident: ::std::sync::LazyLock<#rust_type> = ::std::sync::LazyLock::new(|| #val_tokens);
                });
            }
        }
    }
    const_decls
}

/// Build doc-comment attributes from frontmatter metadata.
pub(crate) fn build_struct_docs(
    frontmatter: &prompt_templates::Frontmatter,
    path_raw: &str,
) -> Vec<proc_macro2::TokenStream> {
    let tmpl_name = if frontmatter.name.is_empty() {
        path_raw.to_string()
    } else {
        frontmatter.name.clone()
    };
    let description = if frontmatter.description.is_empty() {
        String::from("(no description)")
    } else {
        frontmatter.description.clone()
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

/// Choose derive attributes for generated structs based on active features.
///
/// `has_tmpl_fields` — when `true`, serde derives are omitted because
/// `Arc<Template>` does not support `Serialize` / `Deserialize`.
///
/// **Feature flag design:** `cfg!(feature = "serde")` and
/// `cfg!(feature = "typed-builder")` check the *proc-macro crate's own*
/// Cargo features, **not** the downstream user's.  These features are
/// intentionally empty in `Cargo.toml` — they carry no dependencies of their
/// own.  Instead, they act as code-generation toggles: when a user enables
/// `serde` on `prompt-templates-macros`, the proc-macro emits
/// `#[derive(Serialize, Deserialize)]` into the generated code, relying on
/// the user's own `serde` dependency to resolve the derive.
pub(crate) fn struct_derive_attrs(has_tmpl_fields: bool) -> proc_macro2::TokenStream {
    let use_serde = cfg!(feature = "serde") && !has_tmpl_fields;
    match (use_serde, cfg!(feature = "typed-builder")) {
        (true, true) => quote! {
            #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize, ::typed_builder::TypedBuilder)]
        },
        (true, false) => quote! {
            #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
        },
        (false, true) => quote! {
            #[derive(Debug, Clone, PartialEq, ::typed_builder::TypedBuilder)]
        },
        (false, false) => quote! {
            #[derive(Debug, Clone, PartialEq)]
        },
    }
}

/// Generate field-level builder attributes based on the variable type.
///
/// - `String` fields get `#[builder(setter(into))]` for ergonomic `"str".into()`.
/// - `Vec<…>` fields get `#[builder(default)]` so they default to empty.
/// - All other fields have no special builder attributes.
pub(crate) fn builder_field_attrs(
    var_type: &prompt_templates::VarType,
) -> proc_macro2::TokenStream {
    use prompt_templates::VarType;
    if !cfg!(feature = "typed-builder") {
        return quote! {};
    }
    match var_type {
        VarType::Str => quote! { #[builder(setter(into))] },
        VarType::List(_) => quote! { #[builder(default)] },
        _ => quote! {},
    }
}

/// Map a `VarType` to a Rust type token and a setter closure.
pub(crate) fn var_type_to_rust(
    var_type: &prompt_templates::VarType,
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    use prompt_templates::VarType;

    match var_type {
        VarType::Str => (quote! { String }, simple_setter("as_str()")),
        VarType::Int => (quote! { i64 }, simple_setter("")),
        VarType::Float => (quote! { f64 }, simple_setter("")),
        VarType::Bool => (quote! { bool }, simple_setter("")),
        VarType::List(fields) if !fields.is_empty() => {
            if fields.len() == 1 && fields[0].name.is_empty() {
                let (inner_type, inner_set) =
                    var_type_to_rust(&fields[0].var_type, parent_struct, field_name, sub_structs);
                (
                    quote! { ::std::vec::Vec<#inner_type> },
                    Box::new(move |val, name| {
                        let name_lit = name.to_string();
                        let stmts = inner_set(quote! { item }, "item");
                        quote! {
                            ctx.set(#name_lit, ::prompt_templates::Value::List(
                                #val.iter().map(|item| {
                                    let mut inner_ctx = ::prompt_templates::Context::new();
                                    #stmts
                                    inner_ctx.get("item").cloned().unwrap_or(::prompt_templates::Value::Str(::std::string::String::new()))
                                }).collect()
                            ));
                        }
                    }),
                )
            } else {
                typed_list_codegen(fields, parent_struct, field_name, sub_structs)
            }
        }
        VarType::List(_) => (
            quote! { Vec<::prompt_templates::Value> },
            Box::new(|val, name| {
                let name_lit = name.to_string();
                quote! { ctx.set(#name_lit, ::prompt_templates::Value::List(#val.clone())); }
            }),
        ),
        VarType::Dict(fields) if !fields.is_empty() => {
            typed_dict_codegen(fields, parent_struct, field_name, sub_structs)
        }
        VarType::Dict(_) => (
            quote! { ::prompt_templates::Value },
            Box::new(|val, name| {
                let name_lit = name.to_string();
                quote! { ctx.set(#name_lit, #val.clone()); }
            }),
        ),
        VarType::Enum(variants) => {
            typed_enum_codegen(variants, parent_struct, field_name, sub_structs)
        }
        VarType::Tmpl(_) => (
            quote! { ::std::sync::Arc<::prompt_templates::Template> },
            Box::new(|val, name| {
                let name_lit = name.to_string();
                quote! { ctx.set(#name_lit, ::prompt_templates::Value::Tmpl(#val.clone())); }
            }),
        ),
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
