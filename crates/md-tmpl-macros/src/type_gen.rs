use quote::{format_ident, quote};

use crate::{
    crate_path,
    struct_gen::{SetterFn, builder_field_attrs, struct_derive_attrs, var_type_to_rust},
};

/// Check whether any field in `decls` (recursively) contains a `tmpl()` type.
///
/// Used to decide whether serde derives should be suppressed on generated
/// sub-structs — `Arc<Template>` does not implement `Serialize` / `Deserialize`.
fn contains_tmpl_field(decls: &[md_tmpl_core::VarDecl]) -> bool {
    decls.iter().any(|d| var_type_has_tmpl(&d.var_type))
}

/// Recursively check whether a [`VarType`] contains a `Tmpl` variant.
fn var_type_has_tmpl(vt: &md_tmpl_core::VarType) -> bool {
    match vt {
        md_tmpl_core::VarType::Tmpl(_) => true,
        md_tmpl_core::VarType::List(fields) | md_tmpl_core::VarType::Struct(fields) => {
            contains_tmpl_field(fields)
        }
        md_tmpl_core::VarType::Enum(variants) => {
            variants.iter().any(|v| contains_tmpl_field(&v.fields))
        }
        _ => false,
    }
}

/// Generate a sub-struct and setter for a typed list (`list(field = type, ...)`).
pub(crate) fn typed_list_codegen(
    inner_fields: &[md_tmpl_core::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = md_tmpl_core::to_pascal_case(field_name);
    let item_struct_name = format_ident!("{parent_struct}{capitalized}Item");

    let mut item_fields = Vec::new();
    let mut item_set_stmts = Vec::new();

    for inner_decl in inner_fields {
        let inner_field = crate::make_ident(&inner_decl.name);
        let inner_name_str = &inner_decl.name;
        let (inner_type, inner_set) = var_type_to_rust(
            &inner_decl.var_type,
            &format!("{parent_struct}{capitalized}Item"),
            &inner_decl.name,
            sub_structs,
        );
        let inner_builder_attrs = builder_field_attrs(&inner_decl.var_type);
        let inner_rename_attr = crate::serde_rename_attr(&inner_decl.name);
        item_fields
            .push(quote! { #inner_rename_attr #inner_builder_attrs pub #inner_field: #inner_type });
        item_set_stmts.push(inner_set(quote! { item.#inner_field }, inner_name_str));
    }

    let derive_attrs = struct_derive_attrs(contains_tmpl_field(inner_fields));

    sub_structs.push(quote! {
        /// Auto-generated sub-struct for list items.
        #derive_attrs
        pub struct #item_struct_name {
            #(#item_fields),*
        }
    });

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

/// Generate a sub-struct and setter for a typed struct (`struct(field = type, ...)`).
pub(crate) fn typed_dict_codegen(
    inner_fields: &[md_tmpl_core::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = md_tmpl_core::to_pascal_case(field_name);
    let dict_struct_name = format_ident!("{parent_struct}{capitalized}");

    let mut dict_fields = Vec::new();
    let mut dict_set_stmts = Vec::new();

    for inner_decl in inner_fields {
        let inner_field = crate::make_ident(&inner_decl.name);
        let inner_name_str = &inner_decl.name;
        let (inner_type, inner_set) = var_type_to_rust(
            &inner_decl.var_type,
            &format!("{parent_struct}{capitalized}"),
            &inner_decl.name,
            sub_structs,
        );
        let inner_builder_attrs = builder_field_attrs(&inner_decl.var_type);
        let inner_rename_attr = crate::serde_rename_attr(&inner_decl.name);
        dict_fields
            .push(quote! { #inner_rename_attr #inner_builder_attrs pub #inner_field: #inner_type });
        dict_set_stmts.push(inner_set(quote! { val.#inner_field }, inner_name_str));
    }

    let derive_attrs = struct_derive_attrs(contains_tmpl_field(inner_fields));

    sub_structs.push(quote! {
        /// Auto-generated sub-struct for dict fields.
        #derive_attrs
        pub struct #dict_struct_name {
            #(#dict_fields),*
        }
    });

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
                    let mut inner_ctx = #cp::Context::new();
                    #(#stmts)*
                    ctx.set(#name_lit, #cp::Value::Struct(#cp::__private::Arc::new(inner_ctx.into_inner())));
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

/// Convert a list of variant names into unique identifiers, appending numeric
/// suffixes when multiple names map to the same identifier (e.g. `"!!!"` and
/// `"???"` both map to `"Variant"` → `"Variant"` and `"Variant2"`).
pub(crate) fn deduplicate_variant_idents(names: &[String]) -> Vec<(syn::Ident, Option<String>)> {
    let raw: Vec<(syn::Ident, Option<String>)> =
        names.iter().map(|n| string_to_variant_ident(n)).collect();

    // Count occurrences of each ident string.
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (ident, _) in &raw {
        *counts.entry(ident.to_string()).or_insert(0) += 1;
    }

    // For idents that appear more than once, assign sequential suffixes.
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
                // Always need serde rename when we've modified the ident or it
                // differs from the original.
                (new_ident, Some(original.clone()))
            } else {
                (ident, rename)
            }
        })
        .collect()
}

/// Generate a sub-enum and setter for enum variables (`enum[Variant1(fields...), Variant2]`).
pub(crate) fn typed_enum_codegen(
    variants: &[md_tmpl_core::VariantDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = md_tmpl_core::to_pascal_case(field_name);
    let enum_name = quote::format_ident!("{parent_struct}{capitalized}");

    // Classify up front to suppress `#[serde(rename)]` for mixed enums.
    let kind = classify_enum(variants);

    let mut variant_tokens = Vec::new();
    let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&variant_names);

    for (var, (var_ident, rename)) in variants.iter().zip(deduped) {
        let serde_rename = if cfg!(feature = "serde") && kind != EnumKind::Mixed {
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
                    &format!("{parent_struct}{capitalized}{var_ident}"),
                    &inner_decl.name,
                    sub_structs,
                );
                let inner_rename = crate::serde_rename_attr(&inner_decl.name);
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

    let (serde_derives, custom_serde) = enum_derive_attrs(&enum_name, variants, kind);

    sub_structs.push(quote! {
        /// Auto-generated enum for parameter constraints.
        #serde_derives
        pub enum #enum_name {
            #(#variant_tokens),*
        }

        #custom_serde
    });

    let enum_type = enum_name.clone();
    (quote! { #enum_type }, enum_value_setter())
}

/// Setter that serializes an enum field into a `Value` via `to_value`.
///
/// Shared by locally-generated enums ([`typed_enum_codegen`]) and imported enum
/// fields (whose type is aliased to another template's generated enum). Because
/// all generated enums share the same serde representation, the same setter
/// works for both.
pub(crate) fn enum_value_setter() -> crate::struct_gen::SetterFn {
    let cp = crate_path();
    Box::new(move |val, name| {
        let name_lit = name.to_string();
        let cp = &cp;
        quote! {
            ctx.set(
                #name_lit,
                match #cp::to_value(&#val) {
                    ::core::result::Result::Ok(v) => v,
                    ::core::result::Result::Err(e) => {
                        unreachable!("generated enum type should always be serializable: {}", e)
                    }
                },
            );
        }
    })
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
) -> (proc_macro2::TokenStream, SetterFn) {
    let inner_vt = var_type
        .option_inner_type()
        .expect("typed_option_codegen called on non-option type");

    let (inner_type, inner_set) =
        var_type_to_rust(inner_vt, parent_struct, field_name, sub_structs);

    let cp = crate_path();
    (
        quote! { ::core::option::Option<#inner_type> },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            // The inner setter generates `ctx.set(key, value)` statements.
            // We shadow `ctx` with a temporary context to capture the inner value,
            // then extract it for the transparent option representation.
            let inner_stmt = inner_set(quote! { __option_inner_val }, "__option_val__");
            let cp = &cp;
            quote! {
                ctx.set(#name_lit, {
                    let __option_ref = &#val;
                    if let ::core::option::Option::Some(__option_inner_ref) = __option_ref {
                        let __option_inner_val = ::core::clone::Clone::clone(__option_inner_ref);
                        // Use a temporary context to serialize the inner value via
                        // the generated setter.
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

// ---------------------------------------------------------------------------
// Top-level type alias codegen (for `include_template!` and `template!`)
// ---------------------------------------------------------------------------

/// Generate Rust type definitions from frontmatter `types:` declarations.
///
/// Unlike `typed_enum_codegen` (which prefixes names with a parent struct),
/// this function emits types using their **original alias name** from the
/// template.  For example, `BuildPhase = enum(Compile, Link, Test)`
/// generates `pub enum BuildPhase { Compile, Link, Test }`.
///
/// For **unit-variant-only enums**, this additionally generates:
/// - `Display` impl (variant name → string)
/// - `FromStr` impl (case-insensitive variant name → enum)
/// - `VARIANTS` constant (list of all variants)
/// - `all()` convenience method
pub(crate) fn generate_type_alias_tokens(
    type_aliases: &hashbrown::HashMap<String, md_tmpl_core::VarType>,
) -> Vec<proc_macro2::TokenStream> {
    let mut tokens = Vec::new();

    // Sort by name for deterministic output.
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
    variants: &[md_tmpl_core::VariantDecl],
) -> proc_macro2::TokenStream {
    let enum_ident = format_ident!("{}", name);

    // Classify up front so we can suppress `#[serde(rename)]` for mixed enums
    // (which use a custom Serialize/Deserialize impl that doesn't read derive
    // attributes).
    let kind = classify_enum(variants);

    let mut sub_types = Vec::new();
    let mut variant_tokens = Vec::new();
    let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&variant_names);

    for (var, (var_ident, rename)) in variants.iter().zip(deduped) {
        // Only emit `#[serde(rename)]` for non-mixed enums.  Mixed enums have
        // custom Serialize/Deserialize impls that use the raw variant names
        // directly (via `v.name`), so serde derive attributes are orphaned.
        let serde_rename = if cfg!(feature = "serde") && kind != EnumKind::Mixed {
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
                    &format!("{name}{}", var.name),
                    &inner_decl.name,
                    &mut sub_types,
                );
                let inner_rename = crate::serde_rename_attr(&inner_decl.name);
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

    let (serde_derives, custom_serde) = enum_derive_attrs(&enum_ident, variants, kind);

    let doc = format!("Type alias `{name}` from template `types:` block.");

    let extra_impls = match kind {
        EnumKind::UnitOnly => generate_unit_enum_impls(&enum_ident, variants),
        EnumKind::Mixed => generate_mixed_enum_impls(&enum_ident, variants),
        EnumKind::DataOnly => quote! {},
    };

    quote! {
        #(#sub_types)*

        #[doc = #doc]
        #serde_derives
        pub enum #enum_ident {
            #(#variant_tokens),*
        }

        #custom_serde
        #extra_impls
    }
}

/// Map a [`VarType`] to the Rust type token stream used in serde deserialization.
///
/// This is a simplified version of `var_type_to_rust` that only produces the
/// type (no setter closure) and is used inside generated `Deserialize` impls
/// to call `map.next_value::<T>()`.
fn var_type_to_serde_type(var_type: &md_tmpl_core::VarType) -> proc_macro2::TokenStream {
    use md_tmpl_core::VarType;
    match var_type {
        VarType::Int => quote! { i64 },
        VarType::Float => quote! { f64 },
        VarType::Bool => quote! { bool },
        // Str and all complex types fall back to String.  Truly complex nested
        // types in enum variant fields are rare and would need custom handling.
        _ => quote! { ::std::string::String },
    }
}

// ---------------------------------------------------------------------------
// Enum kind classification and shared derive/serde helpers
// ---------------------------------------------------------------------------

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
///
/// Returns `(derive_attrs, custom_serde_tokens)` — the custom serde tokens are
/// empty when derive-based serde is used, and contain manual `Serialize` /
/// `Deserialize` impls for mixed enums.
fn enum_derive_attrs(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
    kind: EnumKind,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    match kind {
        EnumKind::UnitOnly => {
            let derives = if cfg!(feature = "serde") {
                quote! {
                    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
                }
            } else {
                quote! { #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] }
            };
            (derives, quote! {})
        }
        EnumKind::DataOnly => {
            let derives = if cfg!(feature = "serde") {
                quote! {
                    #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
                    #[serde(tag = "__kind__")]
                }
            } else {
                quote! { #[derive(Debug, Clone, PartialEq)] }
            };
            (derives, quote! {})
        }
        EnumKind::Mixed => {
            // Mixed enums never derive Serialize/Deserialize (custom impl is
            // generated separately).  We only derive PartialEq (not Eq/Hash)
            // because data variants may contain f64 or other non-Eq types.
            let derives = quote! { #[derive(Debug, Clone, PartialEq)] };
            let custom_serde = if cfg!(feature = "serde") {
                generate_mixed_enum_serde(enum_ident, variants)
            } else {
                quote! {}
            };
            (derives, custom_serde)
        }
    }
}

/// Generate custom `Serialize` and `Deserialize` impls for a mixed enum.
///
/// Unit variants serialize as plain strings (`"VariantName"`), while data
/// variants serialize as maps with a `__kind__` discriminator field.
fn generate_mixed_enum_serde(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
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

    let data_kind_arms = mixed_deserialize_kind_arms(enum_ident, variants, &deduped);

    let enum_name_str = enum_ident.to_string();
    let expecting_msg =
        format!("a string (unit variant) or map with __kind__ (data variant) for {enum_name_str}");

    quote! {
        impl ::serde::Serialize for #enum_ident {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error> {
                match self {
                    #(#ser_arms),*
                }
            }
        }

        impl<'de> ::serde::Deserialize<'de> for #enum_ident {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> ::core::result::Result<Self, D::Error> {
                struct __MixedEnumVisitor;

                impl<'de> ::serde::de::Visitor<'de> for __MixedEnumVisitor {
                    type Value = #enum_ident;

                    fn expecting(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        f.write_str(#expecting_msg)
                    }

                    fn visit_str<E: ::serde::de::Error>(self, v: &str) -> ::core::result::Result<Self::Value, E> {
                        match v {
                            #(#unit_str_arms,)*
                            other => ::core::result::Result::Err(::serde::de::Error::unknown_variant(
                                other,
                                &[#(#all_unit_names),*],
                            )),
                        }
                    }

                    fn visit_map<A: ::serde::de::MapAccess<'de>>(self, mut map: A) -> ::core::result::Result<Self::Value, A::Error> {
                        let mut __kind: ::core::option::Option<::std::string::String> = ::core::option::Option::None;
                        if let ::core::option::Option::Some(first_key) = map.next_key::<::std::string::String>()? {
                            if first_key == "__kind__" {
                                __kind = ::core::option::Option::Some(map.next_value()?);
                            } else {
                                let _: ::serde::de::IgnoredAny = map.next_value()?;
                                while let ::core::option::Option::Some(key) = map.next_key::<::std::string::String>()? {
                                    if key == "__kind__" {
                                        __kind = ::core::option::Option::Some(map.next_value()?);
                                        break;
                                    }
                                    let _: ::serde::de::IgnoredAny = map.next_value()?;
                                }
                            }
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
///
/// Unit variants become `serialize_str`; data variants become `serialize_map`
/// with a `__kind__` discriminator entry.
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
                let field_count = v.fields.len() + 1; // +1 for __kind__
                let field_idents: Vec<_> = v
                    .fields
                    .iter()
                    .map(|f| crate::make_ident(&f.name))
                    .collect();
                let field_names: Vec<_> = v.fields.iter().map(|f| f.name.as_str()).collect();
                quote! {
                    Self::#ident { #(#field_idents),* } => {
                        use ::serde::ser::SerializeMap;
                        let mut map = serializer.serialize_map(::core::option::Option::Some(#field_count))?;
                        map.serialize_entry("__kind__", #name_str)?;
                        #(map.serialize_entry(#field_names, #field_idents)?;)*
                        map.end()
                    }
                }
            }
        })
        .collect()
}

/// Generate per-variant Deserialize match arms for `visit_map`.
///
/// Each arm declares typed `Option<T>` variables, reads remaining map entries,
/// and constructs the variant.
fn mixed_deserialize_kind_arms(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
    deduped: &[(syn::Ident, Option<String>)],
) -> Vec<proc_macro2::TokenStream> {
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
                .map(|f| var_type_to_serde_type(&f.var_type))
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

            let field_unwraps: Vec<_> = field_idents
                .iter()
                .zip(field_names.iter())
                .zip(field_result_idents.iter())
                .map(|((fi, fn_str), fr)| {
                    quote! {
                        let #fr = #fi.ok_or_else(|| ::serde::de::Error::missing_field(#fn_str))?;
                    }
                })
                .collect();

            quote! {
                #name_str => {
                    #(let mut #field_idents: ::core::option::Option<#field_types> = ::core::option::Option::None;)*
                    while let ::core::option::Option::Some(key) = map.next_key::<::std::string::String>()? {
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

/// Generate `Display`, `FromStr`, `VARIANT_NAMES`, `ALL`, and `as_str` for mixed enums.
///
/// Mixed enums have both unit and struct variants.  The impls handle data
/// variants by matching with `{ .. }` wildcard patterns where needed.
fn generate_mixed_enum_impls(
    enum_ident: &syn::Ident,
    variants: &[md_tmpl_core::VariantDecl],
) -> proc_macro2::TokenStream {
    let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&names);

    let total_variant_count = variants.len();
    let unit_variant_count = variants.iter().filter(|v| v.fields.is_empty()).count();

    // Display: all variants display their declared name.
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

    // as_str: all variants return their declared name. NOT const fn because
    // of data variant patterns.
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

    // FromStr: only unit variants can be constructed from a string.
    let from_str_arms: Vec<_> = variants
        .iter()
        .zip(deduped.iter())
        .filter(|(v, _)| v.fields.is_empty())
        .map(|(v, (ident, _))| {
            let lower = v.name.to_lowercase();
            quote! { #lower => ::core::result::Result::Ok(Self::#ident) }
        })
        .collect();

    // VARIANT_NAMES: all variant names (unit and data).
    let variant_name_strs: Vec<_> = variants
        .iter()
        .map(|v| {
            let name_str = &v.name;
            quote! { #name_str }
        })
        .collect();

    // ALL: only unit variants (can't construct data variants at const time).
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
            pub fn as_str(&self) -> &'static str {
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

/// Generate `Display`, `FromStr`, `VARIANTS`, and `all()` for unit-variant-only enums.
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

/// Generate a top-level struct for a `list(field = type, ...)` type alias.
///
/// The generated struct represents a single item in the list.
pub(crate) fn generate_toplevel_list_item(
    name: &str,
    fields: &[md_tmpl_core::VarDecl],
) -> proc_macro2::TokenStream {
    if fields.is_empty() || (fields.len() == 1 && fields[0].name.is_empty()) {
        return quote! {};
    }
    // For typed lists, generate an item struct.
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

    let derive_attrs = struct_derive_attrs(contains_tmpl_field(fields));
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

    let derive_attrs = struct_derive_attrs(contains_tmpl_field(fields));
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
///
/// Instead of emitting a full enum, this generates `pub type Name = Option<InnerType>`.
/// If the inner type is complex (struct, list), sub-types are generated first.
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
        // All three map to "Variant" originally, so they get suffixed.
        assert_eq!(result[0].0.to_string(), "Variant");
        assert_eq!(result[1].0.to_string(), "Variant2");
        assert_eq!(result[2].0.to_string(), "Variant3");
        // All should have serde renames pointing to the original string.
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
        assert!(result[0].1.is_none()); // no rename needed
        // "---" and "___" both map to "Variant"
        assert_eq!(result[1].0.to_string(), "Variant");
        assert_eq!(result[1].1.as_deref(), Some("---"));
        assert_eq!(result[2].0.to_string(), "Variant2");
        assert_eq!(result[2].1.as_deref(), Some("___"));
        assert_eq!(result[3].0.to_string(), "Fine");
        assert!(result[3].1.is_none());
    }
}
