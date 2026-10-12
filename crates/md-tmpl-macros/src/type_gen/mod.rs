//! Type generation for frontmatter `types:` aliases and `params:` sub-types.

pub(crate) mod enum_gen;

pub(crate) use enum_gen::{
    deduplicate_variant_idents, generate_toplevel_enum, string_to_variant_ident, typed_enum_codegen,
};
use quote::{format_ident, quote};

use crate::{
    crate_path,
    struct_gen::{
        SetterFn, builder_field_attrs, struct_derive_attrs, var_type_to_rust,
        var_type_to_rust_with_alias,
    },
};

/// Check whether any field in `decls` (recursively) contains a `tmpl()` type.
///
/// Used to decide whether serde derives should be suppressed on generated
/// sub-structs — `Arc<Template>` does not implement `Serialize` / `Deserialize`.
pub(crate) fn contains_tmpl_field(decls: &[md_tmpl_core::VarDecl]) -> bool {
    decls.iter().any(|d| var_type_has_tmpl(&d.var_type))
}

/// Recursively check whether a [`VarType`] contains a `Tmpl` variant.
fn var_type_has_tmpl(vt: &md_tmpl_core::VarType) -> bool {
    match vt {
        md_tmpl_core::VarType::Tmpl(_) => true,
        md_tmpl_core::VarType::Option(inner) => var_type_has_tmpl(inner),
        md_tmpl_core::VarType::List(fields) | md_tmpl_core::VarType::Struct(fields) => {
            contains_tmpl_field(fields)
        }
        md_tmpl_core::VarType::Enum(variants) => {
            variants.iter().any(|v| contains_tmpl_field(&v.fields))
        }
        _ => false,
    }
}

/// Check whether a [`VarType`] can derive `Eq` and `Hash` in Rust.
///
/// `Float` (`f64`), `Tmpl` (`Arc<Template>`), and untyped `Value` containers
/// do not implement `Eq` or `Hash`. All other scalar and compound types do
/// whenever their nested fields/variants recursively support `Eq + Hash`.
pub(crate) fn var_type_is_eq_and_hash(vt: &md_tmpl_core::VarType) -> bool {
    use md_tmpl_core::VarType;
    match vt {
        VarType::Str | VarType::Int | VarType::Bool => true,
        VarType::Float | VarType::Tmpl(_) => false,
        VarType::Option(inner) => var_type_is_eq_and_hash(inner),
        VarType::Enum(_) if vt.is_option() => {
            vt.option_inner_type().is_some_and(var_type_is_eq_and_hash)
        }
        VarType::List(fields) | VarType::Struct(fields) => {
            !fields.is_empty() && fields_are_eq_and_hash(fields)
        }
        VarType::Enum(variants) => variants_are_eq_and_hash(variants),
    }
}

/// Check whether all declarations in `decls` support `Eq` and `Hash`.
pub(crate) fn fields_are_eq_and_hash(decls: &[md_tmpl_core::VarDecl]) -> bool {
    decls.iter().all(|d| var_type_is_eq_and_hash(&d.var_type))
}

/// Check whether all variants in `variants` support `Eq` and `Hash`.
pub(crate) fn variants_are_eq_and_hash(variants: &[md_tmpl_core::VariantDecl]) -> bool {
    variants.iter().all(|v| fields_are_eq_and_hash(&v.fields))
}

/// Generate a sub-struct (or type alias) and setter for a typed list (`list(field = type, ...)`).
pub(crate) fn typed_list_codegen(
    inner_fields: &[md_tmpl_core::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
    alias_override: Option<&proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = md_tmpl_core::to_pascal_case(field_name);
    let item_struct_name = format_ident!("{parent_struct}{capitalized}Item");

    let mut item_fields = Vec::new();
    let mut item_set_stmts = Vec::new();
    let mut dummy_subs = Vec::new();
    let target_subs = if alias_override.is_some() {
        &mut dummy_subs
    } else {
        &mut *sub_structs
    };

    for inner_decl in inner_fields {
        let inner_field = crate::make_ident(&inner_decl.name);
        let inner_name_str = &inner_decl.name;
        let (inner_type, inner_set) = var_type_to_rust(
            &inner_decl.var_type,
            &format!("{parent_struct}{capitalized}Item"),
            &inner_decl.name,
            target_subs,
        );
        let inner_builder_attrs = builder_field_attrs(&inner_decl.var_type);
        let inner_rename_attr = crate::serde_rename_attr(&inner_decl.name);
        item_fields
            .push(quote! { #inner_rename_attr #inner_builder_attrs pub #inner_field: #inner_type });
        item_set_stmts.push(inner_set(quote! { item.#inner_field }, inner_name_str));
    }

    if let Some(target_ty) = alias_override {
        sub_structs.push(quote! {
            /// Alias to a shared list item struct.
            pub type #item_struct_name = #target_ty;
        });
    } else {
        let derive_attrs = struct_derive_attrs(
            contains_tmpl_field(inner_fields),
            fields_are_eq_and_hash(inner_fields),
        );
        sub_structs.push(quote! {
            /// Auto-generated sub-struct for list items.
            #derive_attrs
            pub struct #item_struct_name {
                #(#item_fields),*
            }
        });
    }

    let item_struct = item_struct_name.clone();
    let cp = crate_path();
    (
        quote! { Vec<#item_struct> },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            let stmts = &item_set_stmts;
            let cp = &cp;
            quote! {
                ctx.set(#name_lit, #cp::Value::List(#cp::__private::Arc::new(
                    #val.iter().map(|item| {
                        let mut ctx = #cp::Context::new();
                        #(#stmts)*
                        #cp::Value::Struct(#cp::__private::Arc::new(ctx.into_inner()))
                    }).collect()
                )));
            }
        }),
    )
}

/// Generate a sub-struct (or type alias) and setter for a typed struct (`struct(field = type, ...)`).
pub(crate) fn typed_dict_codegen(
    inner_fields: &[md_tmpl_core::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
    alias_override: Option<&proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = md_tmpl_core::to_pascal_case(field_name);
    let dict_struct_name = format_ident!("{parent_struct}{capitalized}");

    let mut dict_fields = Vec::new();
    let mut dict_set_stmts = Vec::new();
    let mut dummy_subs = Vec::new();
    let target_subs = if alias_override.is_some() {
        &mut dummy_subs
    } else {
        &mut *sub_structs
    };

    for inner_decl in inner_fields {
        let inner_field = crate::make_ident(&inner_decl.name);
        let inner_name_str = &inner_decl.name;
        let (inner_type, inner_set) = var_type_to_rust(
            &inner_decl.var_type,
            &format!("{parent_struct}{capitalized}"),
            &inner_decl.name,
            target_subs,
        );
        let inner_builder_attrs = builder_field_attrs(&inner_decl.var_type);
        let inner_rename_attr = crate::serde_rename_attr(&inner_decl.name);
        dict_fields
            .push(quote! { #inner_rename_attr #inner_builder_attrs pub #inner_field: #inner_type });
        dict_set_stmts.push(inner_set(quote! { val.#inner_field }, inner_name_str));
    }

    if let Some(target_ty) = alias_override {
        sub_structs.push(quote! {
            /// Alias to a shared struct type.
            pub type #dict_struct_name = #target_ty;
        });
    } else {
        let derive_attrs = struct_derive_attrs(
            contains_tmpl_field(inner_fields),
            fields_are_eq_and_hash(inner_fields),
        );
        sub_structs.push(quote! {
            /// Auto-generated sub-struct for dict fields.
            #derive_attrs
            pub struct #dict_struct_name {
                #(#dict_fields),*
            }
        });
    }

    let dict_struct = dict_struct_name.clone();
    let cp = crate_path();
    (
        quote! { #dict_struct },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            let stmts = &dict_set_stmts;
            let cp = &cp;
            quote! {
                {
                    let val = &#val;
                    let inner_map = {
                        let mut ctx = #cp::Context::new();
                        #(#stmts)*
                        ctx.into_inner()
                    };
                    ctx.set(#name_lit, #cp::Value::Struct(#cp::__private::Arc::new(inner_map)));
                }
            }
        }),
    )
}

/// Generate `Option<T>` and a setter for option-typed variables (`option(T)`).
///
/// Transparent option representation:
/// - `None` → `Value::None`
/// - `Some(v)` → the inner value directly (e.g. `Value::Int(42)`)
pub(crate) fn typed_option_codegen(
    var_type: &md_tmpl_core::VarType,
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
    alias_override: Option<&proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let inner_vt = var_type
        .option_inner_type()
        .expect("typed_option_codegen called on non-option type");

    let (inner_type, inner_set) = var_type_to_rust_with_alias(
        inner_vt,
        parent_struct,
        field_name,
        sub_structs,
        alias_override,
    );

    let cp = crate_path();
    (
        quote! { ::core::option::Option<#inner_type> },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            let inner_stmt = inner_set(quote! { __option_inner_val }, "__option_val__");
            let cp = &cp;
            quote! {
                ctx.set(#name_lit, {
                    let __option_ref = &#val;
                    if let ::core::option::Option::Some(__option_inner_ref) = __option_ref {
                        let __option_inner_val = ::core::clone::Clone::clone(__option_inner_ref);
                        let mut ctx = #cp::Context::new();
                        #inner_stmt
                        ctx.get("__option_val__")
                            .cloned()
                            .unwrap_or(#cp::Value::None)
                    } else {
                        #cp::Value::None
                    }
                });
            }
        }),
    )
}

/// Generate Rust type definitions from frontmatter `types:` declarations.
pub(crate) fn generate_type_alias_tokens(
    type_aliases: &hashbrown::HashMap<String, md_tmpl_core::VarType>,
) -> Vec<proc_macro2::TokenStream> {
    let mut tokens = Vec::new();

    let mut aliases: Vec<_> = type_aliases.iter().collect();
    aliases.sort_by_key(|(name, _)| (*name).clone());

    for (name, var_type) in aliases {
        match var_type {
            md_tmpl_core::VarType::Option(_) => {
                tokens.push(generate_toplevel_option_alias(name, var_type));
            }
            md_tmpl_core::VarType::Enum(_) if var_type.is_option() => {
                tokens.push(generate_toplevel_option_alias(name, var_type));
            }
            md_tmpl_core::VarType::Enum(variants) => {
                tokens.push(generate_toplevel_enum(name, variants));
            }
            md_tmpl_core::VarType::List(fields) => {
                tokens.push(generate_toplevel_list_item(name, fields));
            }
            md_tmpl_core::VarType::Struct(fields) => {
                tokens.push(generate_toplevel_dict(name, fields));
            }
            _ => {}
        }
    }

    tokens
}

/// Generate a top-level struct for a `list(field = type, ...)` type alias.
pub(crate) fn generate_toplevel_list_item(
    name: &str,
    fields: &[md_tmpl_core::VarDecl],
) -> proc_macro2::TokenStream {
    if fields.is_empty() || (fields.len() == 1 && fields[0].name.is_empty()) {
        return quote! {};
    }
    let item_ident = format_ident!("{}Item", name);
    let mut sub_types = Vec::new();
    let mut item_fields = Vec::new();

    for decl in fields {
        let field_ident = crate::make_ident(&decl.name);
        let (field_type, _) = var_type_to_rust(
            &decl.var_type,
            &format!("{name}Item"),
            &decl.name,
            &mut sub_types,
        );
        let builder_attrs = builder_field_attrs(&decl.var_type);
        let rename_attr = crate::serde_rename_attr(&decl.name);
        item_fields.push(quote! { #rename_attr #builder_attrs pub #field_ident: #field_type });
    }

    let derive_attrs =
        struct_derive_attrs(contains_tmpl_field(fields), fields_are_eq_and_hash(fields));
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

/// Generate a top-level struct for a `struct(field = type, ...)` type alias.
pub(crate) fn generate_toplevel_dict(
    name: &str,
    fields: &[md_tmpl_core::VarDecl],
) -> proc_macro2::TokenStream {
    if fields.is_empty() {
        return quote! {};
    }
    let dict_ident = format_ident!("{}", name);
    let mut sub_types = Vec::new();
    let mut dict_fields = Vec::new();

    for decl in fields {
        let field_ident = crate::make_ident(&decl.name);
        let (field_type, _) = var_type_to_rust(&decl.var_type, name, &decl.name, &mut sub_types);
        let builder_attrs = builder_field_attrs(&decl.var_type);
        let rename_attr = crate::serde_rename_attr(&decl.name);
        dict_fields.push(quote! { #rename_attr #builder_attrs pub #field_ident: #field_type });
    }

    let derive_attrs =
        struct_derive_attrs(contains_tmpl_field(fields), fields_are_eq_and_hash(fields));
    let doc = format!("Struct alias `{name}` from template `types:` block.");

    quote! {
        #(#sub_types)*

        #[doc = #doc]
        #derive_attrs
        pub struct #dict_ident {
            #(#dict_fields),*
        }
    }
}

/// Generate a top-level type alias for an `option(T)` type alias.
pub(crate) fn generate_toplevel_option_alias(
    name: &str,
    var_type: &md_tmpl_core::VarType,
) -> proc_macro2::TokenStream {
    let inner_vt = var_type
        .option_inner_type()
        .expect("generate_toplevel_option_alias called on non-option type");

    let alias_ident = format_ident!("{}", name);
    let mut sub_types = Vec::new();
    let (inner_type, _) = var_type_to_rust(inner_vt, name, "inner", &mut sub_types);
    let doc = format!("Type alias `{name}` (option) from template `types:` block.");

    quote! {
        #(#sub_types)*

        #[doc = #doc]
        pub type #alias_ident = ::core::option::Option<#inner_type>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_to_variant_ident_normal_name() {
        let (ident, rename) = string_to_variant_ident("Confirmed");
        assert_eq!(ident.to_string(), "Confirmed");
        assert!(rename.is_none());
    }

    #[test]
    fn string_to_variant_ident_kebab_case() {
        let (ident, rename) = string_to_variant_ident("in-progress");
        assert_eq!(ident.to_string(), "InProgress");
        assert_eq!(rename, Some("in-progress".to_string()));
    }

    #[test]
    fn string_to_variant_ident_all_non_alphanumeric_falls_back() {
        let (ident, rename) = string_to_variant_ident("!!!");
        assert_eq!(ident.to_string(), "Variant");
        assert_eq!(rename, Some("!!!".to_string()));
    }

    #[test]
    fn deduplicate_no_collisions() {
        let names = vec!["Alpha".to_string(), "Beta".to_string()];
        let result = deduplicate_variant_idents(&names);
        assert_eq!(result[0].0.to_string(), "Alpha");
        assert!(result[0].1.is_none());
        assert_eq!(result[1].0.to_string(), "Beta");
        assert!(result[1].1.is_none());
    }

    #[test]
    fn deduplicate_collision_adds_suffix() {
        let names = vec!["!!!".to_string(), "???".to_string(), "###".to_string()];
        let result = deduplicate_variant_idents(&names);
        assert_eq!(result[0].0.to_string(), "Variant");
        assert_eq!(result[1].0.to_string(), "Variant2");
        assert_eq!(result[2].0.to_string(), "Variant3");
        assert_eq!(result[0].1.as_deref(), Some("!!!"));
        assert_eq!(result[1].1.as_deref(), Some("???"));
        assert_eq!(result[2].1.as_deref(), Some("###"));
    }

    #[test]
    fn deduplicate_mixed_collision_and_unique() {
        let names = vec![
            "Good".to_string(),
            "---".to_string(),
            "___".to_string(),
            "Fine".to_string(),
        ];
        let result = deduplicate_variant_idents(&names);
        assert_eq!(result[0].0.to_string(), "Good");
        assert!(result[0].1.is_none());
        assert_eq!(result[1].0.to_string(), "Variant");
        assert_eq!(result[1].1.as_deref(), Some("---"));
        assert_eq!(result[2].0.to_string(), "Variant2");
        assert_eq!(result[2].1.as_deref(), Some("___"));
        assert_eq!(result[3].0.to_string(), "Fine");
        assert!(result[3].1.is_none());
    }
}
