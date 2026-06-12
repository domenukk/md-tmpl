use quote::{format_ident, quote};

use crate::{
    compile::to_pascal_case,
    struct_gen::{SetterFn, builder_field_attrs, struct_derive_attrs, var_type_to_rust},
};

/// Generate a sub-struct and setter for a typed list (`list<field = type, ...>`).
pub(crate) fn typed_list_codegen(
    inner_fields: &[prompt_templates::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = to_pascal_case(field_name);
    let item_struct_name = format_ident!("{parent_struct}{capitalized}Item");

    let mut item_fields = Vec::new();
    let mut item_set_stmts = Vec::new();

    for inner_decl in inner_fields {
        let inner_field = format_ident!("{}", inner_decl.name);
        let inner_name_str = &inner_decl.name;
        let (inner_type, inner_set) = var_type_to_rust(
            &inner_decl.var_type,
            &format!("{parent_struct}{capitalized}Item"),
            &inner_decl.name,
            sub_structs,
        );
        let inner_builder_attrs = builder_field_attrs(&inner_decl.var_type);
        item_fields.push(quote! { #inner_builder_attrs pub #inner_field: #inner_type });
        item_set_stmts.push(inner_set(quote! { item.#inner_field }, inner_name_str));
    }

    let derive_attrs = struct_derive_attrs(false);

    sub_structs.push(quote! {
        /// Auto-generated sub-struct for list items.
        #derive_attrs
        pub struct #item_struct_name {
            #(#item_fields),*
        }
    });

    let item_struct = item_struct_name.clone();
    (
        quote! { Vec<#item_struct> },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            let stmts = &item_set_stmts;
            quote! {
                ctx.set(#name_lit, ::prompt_templates::Value::List(
                    #val.iter().map(|item| {
                        let mut ctx = ::prompt_templates::Context::new();
                        #(#stmts)*
                        ::prompt_templates::Value::Dict(ctx.into_inner())
                    }).collect()
                ));
            }
        }),
    )
}

/// Generate a sub-struct and setter for a typed dict (`dict<field = type, ...>`).
pub(crate) fn typed_dict_codegen(
    inner_fields: &[prompt_templates::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = to_pascal_case(field_name);
    let dict_struct_name = format_ident!("{parent_struct}{capitalized}");

    let mut dict_fields = Vec::new();
    let mut dict_set_stmts = Vec::new();

    for inner_decl in inner_fields {
        let inner_field = format_ident!("{}", inner_decl.name);
        let inner_name_str = &inner_decl.name;
        let (inner_type, inner_set) = var_type_to_rust(
            &inner_decl.var_type,
            &format!("{parent_struct}{capitalized}"),
            &inner_decl.name,
            sub_structs,
        );
        let inner_builder_attrs = builder_field_attrs(&inner_decl.var_type);
        dict_fields.push(quote! { #inner_builder_attrs pub #inner_field: #inner_type });
        dict_set_stmts.push(inner_set(quote! { val.#inner_field }, inner_name_str));
    }

    let derive_attrs = struct_derive_attrs(false);

    sub_structs.push(quote! {
        /// Auto-generated sub-struct for dict fields.
        #derive_attrs
        pub struct #dict_struct_name {
            #(#dict_fields),*
        }
    });

    let dict_struct = dict_struct_name.clone();
    (
        quote! { #dict_struct },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            let stmts = &dict_set_stmts;
            quote! {
                {
                    let val = &#val;
                    let mut inner_ctx = ::prompt_templates::Context::new();
                    #(#stmts)*
                    ctx.set(#name_lit, ::prompt_templates::Value::Dict(inner_ctx.into_inner()));
                }
            }
        }),
    )
}

/// Generate variant name identifier and its raw string if they differ (for serde rename).
pub(crate) fn string_to_variant_ident(s: &str) -> (syn::Ident, Option<String>) {
    let mut clean = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if capitalize_next {
                clean.extend(c.to_uppercase());
                capitalize_next = false;
            } else {
                clean.push(c);
            }
        } else {
            capitalize_next = true;
        }
    }
    if clean.is_empty() {
        clean = "Variant".to_string();
    }
    let ident = quote::format_ident!("{}", clean);
    if clean == s {
        (ident, None)
    } else {
        (ident, Some(s.to_string()))
    }
}

/// Generate a sub-enum and setter for enum variables (`enum[Variant1(fields...), Variant2]`).
pub(crate) fn typed_enum_codegen(
    variants: &[prompt_templates::VariantDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = to_pascal_case(field_name);
    let enum_name = quote::format_ident!("{parent_struct}{capitalized}");

    let mut variant_tokens = Vec::new();

    for var in variants {
        let (var_ident, rename) = string_to_variant_ident(&var.name);
        let serde_rename = rename.map(|r| quote! { #[serde(rename = #r)] });

        if var.fields.is_empty() {
            variant_tokens.push(quote! {
                #serde_rename
                #var_ident
            });
        } else {
            let mut struct_fields = Vec::new();
            for inner_decl in &var.fields {
                let inner_field = quote::format_ident!("{}", inner_decl.name);
                let (inner_type, _) = var_type_to_rust(
                    &inner_decl.var_type,
                    &format!("{parent_struct}{capitalized}{var_ident}"),
                    &inner_decl.name,
                    sub_structs,
                );
                struct_fields.push(quote! { #inner_field: #inner_type });
            }

            variant_tokens.push(quote! {
                #serde_rename
                #var_ident {
                    #(#struct_fields),*
                }
            });
        }
    }

    // Only use `#[serde(tag)]` for enums that contain struct variants.
    // Unit-variant-only enums serialize as plain strings, which is needed
    // for template display via `{{ value }}`.
    let has_data_variants = variants.iter().any(|v| !v.fields.is_empty());
    let serde_derives = if cfg!(feature = "serde") {
        if has_data_variants {
            quote! {
                #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
                #[serde(tag = "__kind__")]
            }
        } else {
            // Unit-variant-only enums: no heap data, so Copy is safe and
            // Hash enables use as HashMap keys.
            quote! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
            }
        }
    } else if has_data_variants {
        quote! { #[derive(Debug, Clone, PartialEq)] }
    } else {
        quote! { #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] }
    };

    sub_structs.push(quote! {
        /// Auto-generated enum for parameter constraints.
        #serde_derives
        pub enum #enum_name {
            #(#variant_tokens),*
        }
    });

    let enum_type = enum_name.clone();
    (
        quote! { #enum_type },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            quote! {
                ctx.set(
                    #name_lit,
                    ::prompt_templates::to_value(&#val)
                        .expect("infallible: generated enum type is always serializable"),
                );
            }
        }),
    )
}

// ---------------------------------------------------------------------------
// Top-level type alias codegen (for `include_types!`)
// ---------------------------------------------------------------------------

/// Generate Rust type definitions from frontmatter `types:` declarations.
///
/// Unlike `typed_enum_codegen` (which prefixes names with a parent struct),
/// this function emits types using their **original alias name** from the
/// template.  For example, `ResearchPhase = enum<Explore, Triage, Exploit>`
/// generates `pub enum ResearchPhase { Explore, Triage, Exploit }`.
///
/// For **unit-variant-only enums**, this additionally generates:
/// - `Display` impl (variant name → string)
/// - `FromStr` impl (case-insensitive variant name → enum)
/// - `VARIANTS` constant (list of all variants)
/// - `all()` convenience method
pub(crate) fn generate_type_alias_tokens(
    type_aliases: &std::collections::HashMap<String, prompt_templates::VarType>,
) -> Vec<proc_macro2::TokenStream> {
    let mut tokens = Vec::new();

    // Sort by name for deterministic output.
    let mut aliases: Vec<_> = type_aliases.iter().collect();
    aliases.sort_by_key(|(name, _)| (*name).clone());

    for (name, var_type) in aliases {
        match var_type {
            prompt_templates::VarType::Enum(variants) => {
                tokens.push(generate_toplevel_enum(name, variants));
            }
            prompt_templates::VarType::List(fields) => {
                tokens.push(generate_toplevel_list_item(name, fields));
            }
            prompt_templates::VarType::Dict(fields) => {
                tokens.push(generate_toplevel_dict(name, fields));
            }
            // Scalar aliases (str, int, float, bool) don't generate new types —
            // they map directly to Rust primitives.
            _ => {}
        }
    }

    tokens
}

/// Generate a top-level enum from a `types:` alias.
pub(crate) fn generate_toplevel_enum(
    name: &str,
    variants: &[prompt_templates::VariantDecl],
) -> proc_macro2::TokenStream {
    let enum_ident = format_ident!("{}", name);
    let has_data_variants = variants.iter().any(|v| !v.fields.is_empty());

    let mut sub_types = Vec::new();
    let mut variant_tokens = Vec::new();

    for var in variants {
        let (var_ident, rename) = string_to_variant_ident(&var.name);
        let serde_rename = rename.map(|r| quote! { #[serde(rename = #r)] });

        if var.fields.is_empty() {
            variant_tokens.push(quote! {
                #serde_rename
                #var_ident
            });
        } else {
            let mut struct_fields = Vec::new();
            for inner_decl in &var.fields {
                let inner_field = format_ident!("{}", inner_decl.name);
                let (inner_type, _) = var_type_to_rust(
                    &inner_decl.var_type,
                    &format!("{name}{}", var.name),
                    &inner_decl.name,
                    &mut sub_types,
                );
                struct_fields.push(quote! { #inner_field: #inner_type });
            }
            variant_tokens.push(quote! {
                #serde_rename
                #var_ident {
                    #(#struct_fields),*
                }
            });
        }
    }

    let serde_derives = if cfg!(feature = "serde") {
        if has_data_variants {
            quote! {
                #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
                #[serde(tag = "__kind__")]
            }
        } else {
            quote! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
            }
        }
    } else if has_data_variants {
        quote! { #[derive(Debug, Clone, PartialEq)] }
    } else {
        quote! { #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] }
    };

    let doc = format!("Type alias `{name}` from template `types:` block.");

    // For unit-variant-only enums, generate Display, FromStr, VARIANTS, all().
    let extra_impls = if has_data_variants {
        quote! {}
    } else {
        generate_unit_enum_impls(&enum_ident, variants)
    };

    quote! {
        #(#sub_types)*

        #[doc = #doc]
        #serde_derives
        pub enum #enum_ident {
            #(#variant_tokens),*
        }

        #extra_impls
    }
}

/// Generate `Display`, `FromStr`, `VARIANTS`, and `all()` for unit-variant-only enums.
pub(crate) fn generate_unit_enum_impls(
    enum_ident: &syn::Ident,
    variants: &[prompt_templates::VariantDecl],
) -> proc_macro2::TokenStream {
    let variant_count = variants.len();

    let display_arms: Vec<_> = variants
        .iter()
        .map(|v| {
            let (ident, _) = string_to_variant_ident(&v.name);
            let name_str = &v.name;
            quote! { Self::#ident => f.write_str(#name_str) }
        })
        .collect();

    let from_str_arms: Vec<_> = variants
        .iter()
        .map(|v| {
            let (ident, _) = string_to_variant_ident(&v.name);
            let lower = v.name.to_lowercase();
            quote! { #lower => ::std::result::Result::Ok(Self::#ident) }
        })
        .collect();

    let variant_names: Vec<_> = variants
        .iter()
        .map(|v| {
            let name_str = &v.name;
            quote! { #name_str }
        })
        .collect();

    let variant_idents: Vec<_> = variants
        .iter()
        .map(|v| {
            let (ident, _) = string_to_variant_ident(&v.name);
            quote! { Self::#ident }
        })
        .collect();

    let enum_name_str = enum_ident.to_string();

    quote! {
        impl ::std::fmt::Display for #enum_ident {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                match self {
                    #(#display_arms),*
                }
            }
        }

        impl ::std::str::FromStr for #enum_ident {
            type Err = String;

            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    #(#from_str_arms,)*
                    other => ::std::result::Result::Err(::std::format!(
                        "unknown {} variant {:?}: expected one of [{}]",
                        #enum_name_str,
                        other,
                        Self::VARIANT_NAMES.join(", "),
                    )),
                }
            }
        }

        impl #enum_ident {
            /// All variant names as strings (in declaration order).
            pub const VARIANT_NAMES: [&'static str; #variant_count] = [#(#variant_names),*];

            /// All variants (in declaration order).
            pub const ALL: [Self; #variant_count] = [#(#variant_idents),*];
        }
    }
}

/// Generate a top-level struct for a `list<field = type, ...>` type alias.
///
/// The generated struct represents a single item in the list.
pub(crate) fn generate_toplevel_list_item(
    name: &str,
    fields: &[prompt_templates::VarDecl],
) -> proc_macro2::TokenStream {
    if fields.is_empty() {
        return quote! {};
    }
    // For typed lists, generate an item struct.
    let item_ident = format_ident!("{}Item", name);
    let mut sub_types = Vec::new();
    let mut item_fields = Vec::new();

    for decl in fields {
        let field_ident = format_ident!("{}", decl.name);
        let (field_type, _) = var_type_to_rust(
            &decl.var_type,
            &format!("{name}Item"),
            &decl.name,
            &mut sub_types,
        );
        let builder_attrs = builder_field_attrs(&decl.var_type);
        item_fields.push(quote! { #builder_attrs pub #field_ident: #field_type });
    }

    let derive_attrs = struct_derive_attrs(false);
    let doc = format!("Item type for list alias `{name}` from template `types:` block.");

    quote! {
        #(#sub_types)*

        #[doc = #doc]
        #derive_attrs
        pub struct #item_ident {
            #(#item_fields),*
        }
    }
}

/// Generate a top-level struct for a `dict<field = type, ...>` type alias.
pub(crate) fn generate_toplevel_dict(
    name: &str,
    fields: &[prompt_templates::VarDecl],
) -> proc_macro2::TokenStream {
    if fields.is_empty() {
        return quote! {};
    }
    let dict_ident = format_ident!("{}", name);
    let mut sub_types = Vec::new();
    let mut dict_fields = Vec::new();

    for decl in fields {
        let field_ident = format_ident!("{}", decl.name);
        let (field_type, _) = var_type_to_rust(&decl.var_type, name, &decl.name, &mut sub_types);
        let builder_attrs = builder_field_attrs(&decl.var_type);
        dict_fields.push(quote! { #builder_attrs pub #field_ident: #field_type });
    }

    let derive_attrs = struct_derive_attrs(false);
    let doc = format!("Dict alias `{name}` from template `types:` block.");

    quote! {
        #(#sub_types)*

        #[doc = #doc]
        #derive_attrs
        pub struct #dict_ident {
            #(#dict_fields),*
        }
    }
}
