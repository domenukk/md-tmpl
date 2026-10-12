//! Enum code generation for frontmatter `types:` and `params:` declarations.
//!
//! Handles unit-only, data-only, and mixed enums, including `Eq + Hash`
//! derivation, custom `Serialize`/`Deserialize` for mixed enums, `Display`,
//! `FromStr`, `as_str()`, `VARIANT_NAMES`, `ALL`, and direct `Context` setters.

use quote::{format_ident, quote};

use super::{contains_tmpl_field, variants_are_eq_and_hash};
use crate::{
    crate_path,
    struct_gen::{SetterFn, var_type_to_rust},
};

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

/// Convert a list of variant names into unique identifiers, appending numeric
/// suffixes when multiple names map to the same identifier (e.g. `"!!!"` and
/// `"???"` both map to `"Variant"` → `"Variant"` and `"Variant2"`).
pub(crate) fn deduplicate_variant_idents(names: &[String]) -> Vec<(syn::Ident, Option<String>)> {
    let raw: Vec<(syn::Ident, Option<String>)> =
        names.iter().map(|n| string_to_variant_ident(n)).collect();

    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (ident, _) in &raw {
        *counts.entry(ident.to_string()).or_insert(0) += 1;
    }

    let mut next_suffix: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    raw.into_iter()
        .zip(names.iter())
        .map(|((ident, rename), original)| {
            let key = ident.to_string();
            if counts[&key] > 1 {
                let suffix = next_suffix.entry(key.clone()).or_insert(1);
                let new_name = if *suffix == 1 {
                    key.clone()
                } else {
                    format!("{key}{suffix}")
                };
                *suffix += 1;
                let new_ident = quote::format_ident!("{}", new_name);
                (new_ident, Some(original.clone()))
            } else {
                (ident, rename)
            }
        })
        .collect()
}

/// Generate a sub-enum (or type alias) and setter for enum variables.
pub(crate) fn typed_enum_codegen(
    variants: &[md_tmpl_core::VariantDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
    alias_override: Option<&proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = md_tmpl_core::to_pascal_case(field_name);
    let enum_name = quote::format_ident!("{parent_struct}{capitalized}");
    let parent_prefix = format!("{parent_struct}{capitalized}");

    if let Some(target_ty) = alias_override {
        sub_structs.push(quote! {
            /// Alias to a shared enum type.
            pub type #enum_name = #target_ty;
        });
    } else {
        let doc = "Auto-generated enum for parameter constraints.";
        let def = emit_enum_definition(&enum_name, doc, variants, &parent_prefix, sub_structs);
        sub_structs.push(def);
    }

    let setter = build_direct_enum_setter(&enum_name, variants, &parent_prefix);
    (quote! { #enum_name }, setter)
}

/// Build a direct `match`-based setter that converts an enum value into a `Value`
/// without requiring `serde` (`to_value`).
fn build_direct_enum_setter(
    enum_name: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
    parent_prefix: &str,
) -> SetterFn {
    let cp = crate_path();
    let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&variant_names);

    let mut match_arms = Vec::with_capacity(variants.len());
    for (var, (var_ident, _)) in variants.iter().zip(deduped.iter()) {
        let name_str = &var.name;
        if var.fields.is_empty() {
            match_arms.push(quote! {
                #enum_name::#var_ident => #cp::__private::make_str_value(#name_str)
            });
        } else {
            let mut pat_bindings = Vec::with_capacity(var.fields.len());
            let mut field_stmts = Vec::with_capacity(var.fields.len());
            let mut dummy_subs = Vec::new();
            for inner_decl in &var.fields {
                let inner_field = crate::make_ident(&inner_decl.name);
                let bound_ident = format_ident!("__md_tmpl_enum_field_{}", inner_decl.name);
                let (_, inner_set) = var_type_to_rust(
                    &inner_decl.var_type,
                    &format!("{parent_prefix}{var_ident}"),
                    &inner_decl.name,
                    &mut dummy_subs,
                );
                pat_bindings.push(quote! { #inner_field: #bound_ident });
                field_stmts.push(inner_set(quote! { (*#bound_ident) }, &inner_decl.name));
            }
            match_arms.push(quote! {
                #enum_name::#var_ident { #(#pat_bindings),* } => {
                    let mut ctx = #cp::Context::new();
                    #(#field_stmts)*
                    let mut __enum_map = ctx.into_inner();
                    __enum_map.insert(
                        #cp::__private::String::from("__kind__"),
                        #cp::__private::make_str_value(#name_str),
                    );
                    #cp::Value::Struct(#cp::__private::Arc::new(__enum_map))
                }
            });
        }
    }

    Box::new(move |val, name| {
        let name_lit = name.to_string();
        let arms = &match_arms;
        quote! {
            {
                let __enum_val = match &#val {
                    #(#arms),*
                };
                ctx.set(#name_lit, __enum_val);
            }
        }
    })
}

/// Generate a top-level enum from a `types:` alias.
pub(crate) fn generate_toplevel_enum(
    name: &str,
    variants: &[md_tmpl_core::VariantDecl],
) -> proc_macro2::TokenStream {
    let enum_ident = format_ident!("{}", name);
    let doc = format!("Type alias `{name}` from template `types:` block.");
    let mut sub_types = Vec::new();
    let def = emit_enum_definition(&enum_ident, &doc, variants, name, &mut sub_types);
    quote! {
        #(#sub_types)*
        #def
    }
}

/// Shared enum definition emitter used by both top-level `types:` enums and
/// inline `params:` enums.
fn emit_enum_definition(
    enum_ident: &syn::Ident,
    doc: &str,
    variants: &[md_tmpl_core::VariantDecl],
    parent_prefix: &str,
    sub_types: &mut Vec<proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    let kind = classify_enum(variants);
    let has_tmpl = variants.iter().any(|v| contains_tmpl_field(&v.fields));
    let use_serde = cfg!(feature = "serde") && !has_tmpl;

    let mut variant_tokens = Vec::new();
    let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&variant_names);

    for (var, (var_ident, rename)) in variants.iter().zip(deduped) {
        let serde_rename = if use_serde && kind != EnumKind::Mixed {
            rename.map(|r| quote! { #[serde(rename = #r)] })
        } else {
            None
        };

        if var.fields.is_empty() {
            variant_tokens.push(quote! {
                #serde_rename
                #var_ident
            });
        } else {
            let mut struct_fields = Vec::new();
            for inner_decl in &var.fields {
                let inner_field = crate::make_ident(&inner_decl.name);
                let (inner_type, _) = var_type_to_rust(
                    &inner_decl.var_type,
                    &format!("{parent_prefix}{var_ident}"),
                    &inner_decl.name,
                    sub_types,
                );
                let inner_rename = if use_serde && kind != EnumKind::Mixed {
                    crate::serde_rename_attr(&inner_decl.name)
                } else {
                    quote! {}
                };
                struct_fields.push(quote! { #inner_rename #inner_field: #inner_type });
            }
            variant_tokens.push(quote! {
                #serde_rename
                #var_ident {
                    #(#struct_fields),*
                }
            });
        }
    }

    let (serde_derives, custom_serde) =
        enum_derive_attrs(enum_ident, variants, kind, parent_prefix);

    let extra_impls = match kind {
        EnumKind::UnitOnly => generate_unit_enum_impls(enum_ident, variants),
        EnumKind::Mixed => generate_mixed_enum_impls(enum_ident, variants),
        EnumKind::DataOnly => generate_data_enum_impls(enum_ident, variants),
    };

    quote! {
        #[doc = #doc]
        #serde_derives
        pub enum #enum_ident {
            #(#variant_tokens),*
        }

        #custom_serde
        #extra_impls
    }
}

/// Classification of an enum's variant composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnumKind {
    /// All variants are unit variants (no fields).
    UnitOnly,
    /// All variants carry data (struct fields).
    DataOnly,
    /// Mix of unit variants and data variants.
    Mixed,
}

/// Classify an enum as unit-only, data-only, or mixed.
fn classify_enum(variants: &[md_tmpl_core::VariantDecl]) -> EnumKind {
    let has_unit = variants.iter().any(|v| v.fields.is_empty());
    let has_data = variants.iter().any(|v| !v.fields.is_empty());
    match (has_unit, has_data) {
        (true, false) => EnumKind::UnitOnly,
        (false, true) => EnumKind::DataOnly,
        _ => EnumKind::Mixed,
    }
}

/// Generate `#[derive(...)]` attributes and optional custom serde impls for an enum.
fn enum_derive_attrs(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
    kind: EnumKind,
    parent_prefix: &str,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let has_tmpl = variants.iter().any(|v| contains_tmpl_field(&v.fields));
    let use_serde = cfg!(feature = "serde") && !has_tmpl;
    let is_eq_hash = variants_are_eq_and_hash(variants);

    match kind {
        EnumKind::UnitOnly => {
            let derives = if use_serde {
                quote! {
                    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
                }
            } else {
                quote! { #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] }
            };
            (derives, quote! {})
        }
        EnumKind::DataOnly => {
            let derives = match (use_serde, is_eq_hash) {
                (true, true) => quote! {
                    #[derive(Debug, Clone, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
                    #[serde(tag = "__kind__")]
                },
                (true, false) => quote! {
                    #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
                    #[serde(tag = "__kind__")]
                },
                (false, true) => quote! {
                    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
                },
                (false, false) => quote! {
                    #[derive(Debug, Clone, PartialEq)]
                },
            };
            (derives, quote! {})
        }
        EnumKind::Mixed => {
            let derives = if is_eq_hash {
                quote! { #[derive(Debug, Clone, PartialEq, Eq, Hash)] }
            } else {
                quote! { #[derive(Debug, Clone, PartialEq)] }
            };
            let custom_serde = if use_serde {
                generate_mixed_enum_serde(enum_ident, variants, parent_prefix)
            } else {
                quote! {}
            };
            (derives, custom_serde)
        }
    }
}

/// Generate custom `Serialize` and `Deserialize` impls for a mixed enum.
fn generate_mixed_enum_serde(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
    parent_prefix: &str,
) -> proc_macro2::TokenStream {
    let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&names);

    let ser_arms = mixed_serialize_arms(variants, &deduped);

    let unit_str_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .filter(|(v, _)| v.fields.is_empty())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            quote! { #name_str => ::core::result::Result::Ok(#enum_ident::#ident) }
        })
        .collect();

    let all_unit_names: Vec<_> = variants
        .iter()
        .filter(|v| v.fields.is_empty())
        .map(|v| v.name.as_str())
        .collect();
    let all_data_names: Vec<_> = variants
        .iter()
        .filter(|v| !v.fields.is_empty())
        .map(|v| v.name.as_str())
        .collect();

    let data_kind_arms = mixed_deserialize_kind_arms(enum_ident, variants, &deduped, parent_prefix);

    let enum_name_str = enum_ident.to_string();
    let expecting_msg =
        format!("a string (unit variant) or map with __kind__ (data variant) for {enum_name_str}");
    let cp = crate_path();

    quote! {
        impl ::serde::Serialize for #enum_ident {
            fn serialize<__MdTmplSerializer: ::serde::Serializer>(&self, serializer: __MdTmplSerializer) -> ::core::result::Result<__MdTmplSerializer::Ok, __MdTmplSerializer::Error> {
                match self {
                    #(#ser_arms),*
                }
            }
        }

        impl<'de> ::serde::Deserialize<'de> for #enum_ident {
            fn deserialize<__MdTmplDeserializer: ::serde::Deserializer<'de>>(deserializer: __MdTmplDeserializer) -> ::core::result::Result<Self, __MdTmplDeserializer::Error> {
                struct __MixedEnumVisitor;

                impl<'de> ::serde::de::Visitor<'de> for __MixedEnumVisitor {
                    type Value = #enum_ident;

                    fn expecting(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        f.write_str(#expecting_msg)
                    }

                    fn visit_str<__MdTmplError: ::serde::de::Error>(self, v: &str) -> ::core::result::Result<Self::Value, __MdTmplError> {
                        match v {
                            #(#unit_str_arms,)*
                            other => ::core::result::Result::Err(::serde::de::Error::unknown_variant(
                                other,
                                &[#(#all_unit_names),*],
                            )),
                        }
                    }

                    fn visit_map<__MdTmplMapAccess: ::serde::de::MapAccess<'de>>(self, mut map: __MdTmplMapAccess) -> ::core::result::Result<Self::Value, __MdTmplMapAccess::Error> {
                        let mut __kind: ::core::option::Option<#cp::__private::String> = ::core::option::Option::None;
                        let mut __buffered: #cp::__private::Vec<(#cp::__private::String, #cp::Value)> =
                            #cp::__private::Vec::new();
                        while let ::core::option::Option::Some(key) = map.next_key::<#cp::__private::String>()? {
                            if key == "__kind__" {
                                __kind = ::core::option::Option::Some(map.next_value()?);
                                break;
                            }
                            let val: #cp::Value = map.next_value()?;
                            __buffered.push((key, val));
                        }

                        let __kind_val = __kind.ok_or_else(|| ::serde::de::Error::missing_field("__kind__"))?;
                        match __kind_val.as_str() {
                            #(#data_kind_arms,)*
                            other => ::core::result::Result::Err(::serde::de::Error::unknown_variant(
                                other,
                                &[#(#all_data_names),*],
                            )),
                        }
                    }
                }

                deserializer.deserialize_any(__MixedEnumVisitor)
            }
        }
    }
}

/// Generate Serialize match arms for a mixed enum.
fn mixed_serialize_arms(
    variants: &[md_tmpl_core::VariantDecl],
    deduped: &[(syn::Ident, Option<String>)],
) -> Vec<proc_macro2::TokenStream> {
    variants
        .iter()
        .zip(deduped.iter())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            if v.fields.is_empty() {
                quote! { Self::#ident => serializer.serialize_str(#name_str) }
            } else {
                let field_count = v.fields.len() + 1;
                let field_idents: Vec<_> = v
                    .fields
                    .iter()
                    .map(|f| crate::make_ident(&f.name))
                    .collect();
                let bound_idents: Vec<_> = v
                    .fields
                    .iter()
                    .map(|f| format_ident!("__md_tmpl_field_{}", f.name))
                    .collect();
                let field_names: Vec<_> = v.fields.iter().map(|f| f.name.as_str()).collect();
                quote! {
                    Self::#ident { #(#field_idents: #bound_idents),* } => {
                        use ::serde::ser::SerializeMap;
                        let mut map = serializer.serialize_map(::core::option::Option::Some(#field_count))?;
                        map.serialize_entry("__kind__", #name_str)?;
                        #(map.serialize_entry(#field_names, #bound_idents)?;)*
                        map.end()
                    }
                }
            }
        })
        .collect()
}

/// Generate per-variant Deserialize match arms for `visit_map`.
fn mixed_deserialize_kind_arms(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
    deduped: &[(syn::Ident, Option<String>)],
    parent_prefix: &str,
) -> Vec<proc_macro2::TokenStream> {
    let cp = crate_path();
    variants
        .iter()
        .zip(deduped.iter())
        .filter(|(v, _)| !v.fields.is_empty())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            let field_idents: Vec<_> = v
                .fields
                .iter()
                .map(|f| format_ident!("__field_{}", f.name))
                .collect();
            let field_names: Vec<_> = v.fields.iter().map(|f| f.name.as_str()).collect();
            let field_result_idents: Vec<_> = v
                .fields
                .iter()
                .map(|f| crate::make_ident(&f.name))
                .collect();
            let field_types: Vec<_> = v
                .fields
                .iter()
                .map(|f| {
                    let mut dummy_subs = Vec::new();
                    let (ft, _) = var_type_to_rust(
                        &f.var_type,
                        &format!("{parent_prefix}{ident}"),
                        &f.name,
                        &mut dummy_subs,
                    );
                    ft
                })
                .collect();

            let buffered_field_match_arms: Vec<_> = field_idents
                .iter()
                .zip(field_names.iter())
                .zip(field_types.iter())
                .map(|((fi, fn_str), ft)| {
                    quote! {
                        #fn_str => {
                            #fi = ::core::option::Option::Some(
                                #cp::from_value::<#ft>(__bval).map_err(::serde::de::Error::custom)?
                            );
                        }
                    }
                })
                .collect();

            let field_match_arms: Vec<_> = field_idents
                .iter()
                .zip(field_names.iter())
                .zip(field_types.iter())
                .map(|((fi, fn_str), ft)| {
                    quote! {
                        #fn_str => {
                            #fi = ::core::option::Option::Some(map.next_value::<#ft>()?);
                        }
                    }
                })
                .collect();

            let field_unwraps: Vec<_> = v
                .fields
                .iter()
                .zip(field_idents.iter())
                .zip(field_names.iter())
                .zip(field_result_idents.iter())
                .map(|(((f_decl, fi), fn_str), fr)| {
                    let is_opt = matches!(f_decl.var_type, md_tmpl_core::VarType::Option(_))
                        || f_decl.var_type.is_option();
                    if is_opt {
                        quote! {
                            let #fr = #fi.unwrap_or(::core::option::Option::None);
                        }
                    } else {
                        quote! {
                            let #fr = #fi.ok_or_else(|| ::serde::de::Error::missing_field(#fn_str))?;
                        }
                    }
                })
                .collect();

            quote! {
                #name_str => {
                    #(let mut #field_idents: ::core::option::Option<#field_types> = ::core::option::Option::None;)*
                    for (__bkey, __bval) in &__buffered {
                        match __bkey.as_str() {
                            #(#buffered_field_match_arms)*
                            _ => {}
                        }
                    }
                    while let ::core::option::Option::Some(key) = map.next_key::<#cp::__private::String>()? {
                        match key.as_str() {
                            #(#field_match_arms)*
                            _ => { let _: ::serde::de::IgnoredAny = map.next_value()?; }
                        }
                    }
                    #(#field_unwraps)*
                    ::core::result::Result::Ok(#enum_ident::#ident { #(#field_result_idents),* })
                }
            }
        })
        .collect()
}

/// Generate `Display`, `VARIANT_NAMES`, and `as_str` for data-only enums.
fn generate_data_enum_impls(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
) -> proc_macro2::TokenStream {
    let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&names);
    let total_variant_count = variants.len();

    let display_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            quote! { Self::#ident { .. } => f.write_str(#name_str) }
        })
        .collect();

    let as_str_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            quote! { Self::#ident { .. } => #name_str }
        })
        .collect();

    let variant_name_strs: Vec<_> = variants
        .iter()
        .map(|v| {
            let name_str = &v.name;
            quote! { #name_str }
        })
        .collect();

    quote! {
        impl ::core::fmt::Display for #enum_ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms),*
                }
            }
        }

        impl #enum_ident {
            /// Return the variant name as a static string slice.
            #[must_use]
            pub const fn as_str(&self) -> &'static str {
                match self {
                    #(#as_str_arms),*
                }
            }

            /// All variant names as strings (in declaration order).
            pub const VARIANT_NAMES: [&'static str; #total_variant_count] = [#(#variant_name_strs),*];
        }
    }
}

/// Generate `Display`, `FromStr`, `VARIANT_NAMES`, `ALL`, and `as_str` for mixed enums.
fn generate_mixed_enum_impls(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
) -> proc_macro2::TokenStream {
    let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&names);

    let total_variant_count = variants.len();
    let unit_variant_count = variants.iter().filter(|v| v.fields.is_empty()).count();

    let display_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            if v.fields.is_empty() {
                quote! { Self::#ident => f.write_str(#name_str) }
            } else {
                quote! { Self::#ident { .. } => f.write_str(#name_str) }
            }
        })
        .collect();

    let as_str_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            if v.fields.is_empty() {
                quote! { Self::#ident => #name_str }
            } else {
                quote! { Self::#ident { .. } => #name_str }
            }
        })
        .collect();

    let from_str_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .filter(|(v, _)| v.fields.is_empty())
        .map(|(v, (ident, _))| {
            let lower = v.name.to_lowercase();
            quote! { #lower => ::core::result::Result::Ok(Self::#ident) }
        })
        .collect();

    let variant_name_strs: Vec<_> = variants
        .iter()
        .map(|v| {
            let name_str = &v.name;
            quote! { #name_str }
        })
        .collect();

    let unit_variant_idents: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .filter(|(v, _)| v.fields.is_empty())
        .map(|(_, (ident, _))| {
            quote! { Self::#ident }
        })
        .collect();

    let enum_name_str = enum_ident.to_string();
    let cp = crate_path();

    quote! {
        impl ::core::fmt::Display for #enum_ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms),*
                }
            }
        }

        impl ::core::str::FromStr for #enum_ident {
            type Err = #cp::__private::String;

            fn from_str(s: &str) -> ::core::result::Result<Self, <Self as ::core::str::FromStr>::Err> {
                match s.to_lowercase().as_str() {
                    #(#from_str_arms,)*
                    other => ::core::result::Result::Err(#cp::__private::format!(
                        "unknown {} variant {:?}: expected one of [{}]",
                        #enum_name_str,
                        other,
                        Self::VARIANT_NAMES.join(", "),
                    )),
                }
            }
        }

        impl #enum_ident {
            /// Return the variant name as a static string slice.
            #[must_use]
            pub const fn as_str(&self) -> &'static str {
                match self {
                    #(#as_str_arms),*
                }
            }

            /// All variant names as strings (in declaration order).
            pub const VARIANT_NAMES: [&'static str; #total_variant_count] = [#(#variant_name_strs),*];

            /// All unit variants (in declaration order).
            pub const ALL: [Self; #unit_variant_count] = [#(#unit_variant_idents),*];
        }
    }
}

/// Generate `Display`, `FromStr`, `VARIANT_NAMES`, `ALL`, and `as_str` for unit-variant-only enums.
pub(crate) fn generate_unit_enum_impls(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
) -> proc_macro2::TokenStream {
    let variant_count = variants.len();
    let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&names);

    let display_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            quote! { Self::#ident => f.write_str(#name_str) }
        })
        .collect();

    let as_str_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .map(|(v, (ident, _))| {
            let name_str = &v.name;
            quote! { Self::#ident => #name_str }
        })
        .collect();

    let from_str_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .map(|(v, (ident, _))| {
            let lower = v.name.to_lowercase();
            quote! { #lower => ::core::result::Result::Ok(Self::#ident) }
        })
        .collect();

    let variant_names: Vec<_> = variants
        .iter()
        .map(|v| {
            let name_str = &v.name;
            quote! { #name_str }
        })
        .collect();

    let variant_idents: Vec<_> = deduped
        .iter()
        .map(|(ident, _)| {
            quote! { Self::#ident }
        })
        .collect();

    let enum_name_str = enum_ident.to_string();
    let cp = crate_path();

    quote! {
        impl ::core::fmt::Display for #enum_ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms),*
                }
            }
        }

        impl ::core::str::FromStr for #enum_ident {
            type Err = #cp::__private::String;

            fn from_str(s: &str) -> ::core::result::Result<Self, <Self as ::core::str::FromStr>::Err> {
                match s.to_lowercase().as_str() {
                    #(#from_str_arms,)*
                    other => ::core::result::Result::Err(#cp::__private::format!(
                        "unknown {} variant {:?}: expected one of [{}]",
                        #enum_name_str,
                        other,
                        Self::VARIANT_NAMES.join(", "),
                    )),
                }
            }
        }

        impl #enum_ident {
            /// Return the variant name as a static string slice.
            #[must_use]
            pub const fn as_str(&self) -> &'static str {
                match self {
                    #(#as_str_arms),*
                }
            }

            /// All variant names as strings (in declaration order).
            pub const VARIANT_NAMES: [&'static str; #variant_count] = [#(#variant_names),*];

            /// All variants (in declaration order).
            pub const ALL: [Self; #variant_count] = [#(#variant_idents),*];
        }
    }
}
