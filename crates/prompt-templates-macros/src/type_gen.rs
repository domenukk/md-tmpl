use quote::{format_ident, quote};

use crate::{
    crate_path,
    struct_gen::{SetterFn, builder_field_attrs, struct_derive_attrs, var_type_to_rust},
};

/// Check whether any field in `decls` (recursively) contains a `tmpl<>` type.
///
/// Used to decide whether serde derives should be suppressed on generated
/// sub-structs — `Arc<Template>` does not implement `Serialize` / `Deserialize`.
fn contains_tmpl_field(decls: &[prompt_templates::VarDecl]) -> bool {
    decls.iter().any(|d| var_type_has_tmpl(&d.var_type))
}

/// Recursively check whether a [`VarType`] contains a `Tmpl` variant.
fn var_type_has_tmpl(vt: &prompt_templates::VarType) -> bool {
    match vt {
        prompt_templates::VarType::Tmpl(_) => true,
        prompt_templates::VarType::List(fields) | prompt_templates::VarType::Struct(fields) => {
            contains_tmpl_field(fields)
        }
        prompt_templates::VarType::Enum(variants) => {
            variants.iter().any(|v| contains_tmpl_field(&v.fields))
        }
        _ => false,
    }
}

/// Generate a sub-struct and setter for a typed list (`list<field = type, ...>`).
pub(crate) fn typed_list_codegen(
    inner_fields: &[prompt_templates::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = prompt_templates::to_pascal_case(field_name);
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

/// Generate a sub-struct and setter for a typed struct (`struct<field = type, ...>`).
pub(crate) fn typed_dict_codegen(
    inner_fields: &[prompt_templates::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = prompt_templates::to_pascal_case(field_name);
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
    variants: &[prompt_templates::VariantDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = prompt_templates::to_pascal_case(field_name);
    let enum_name = quote::format_ident!("{parent_struct}{capitalized}");

    let mut variant_tokens = Vec::new();
    let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&variant_names);

    for (var, (var_ident, rename)) in variants.iter().zip(deduped) {
        let serde_rename = if cfg!(feature = "serde") {
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
    let cp = crate_path();
    (
        quote! { #enum_type },
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
        }),
    )
}

/// Generate `Option<T>` and a setter for option-typed variables (`option<T>`).
///
/// Transparent option representation:
/// - `None` → `Value::None`
/// - `Some(v)` → the inner value directly (e.g. `Value::Int(42)`)
pub(crate) fn typed_option_codegen(
    var_type: &prompt_templates::VarType,
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
/// template.  For example, `BuildPhase = enum<Compile, Link, Test>`
/// generates `pub enum BuildPhase { Compile, Link, Test }`.
///
/// For **unit-variant-only enums**, this additionally generates:
/// - `Display` impl (variant name → string)
/// - `FromStr` impl (case-insensitive variant name → enum)
/// - `VARIANTS` constant (list of all variants)
/// - `all()` convenience method
pub(crate) fn generate_type_alias_tokens(
    type_aliases: &hashbrown::HashMap<String, prompt_templates::VarType>,
) -> Vec<proc_macro2::TokenStream> {
    let mut tokens = Vec::new();

    // Sort by name for deterministic output.
    let mut aliases: Vec<_> = type_aliases.iter().collect();
    aliases.sort_by_key(|(name, _)| (*name).clone());

    for (name, var_type) in aliases {
        match var_type {
            prompt_templates::VarType::Option(_) => {
                tokens.push(generate_toplevel_option_alias(name, var_type));
            }
            prompt_templates::VarType::Enum(_) if var_type.is_option() => {
                tokens.push(generate_toplevel_option_alias(name, var_type));
            }
            prompt_templates::VarType::Enum(variants) => {
                tokens.push(generate_toplevel_enum(name, variants));
            }
            prompt_templates::VarType::List(fields) => {
                tokens.push(generate_toplevel_list_item(name, fields));
            }
            prompt_templates::VarType::Struct(fields) => {
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
    let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&variant_names);

    for (var, (var_ident, rename)) in variants.iter().zip(deduped) {
        let serde_rename = if cfg!(feature = "serde") {
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

            fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
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

/// Generate a top-level struct for a `struct<field = type, ...>` type alias.
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

/// Generate a top-level type alias for an `option<T>` type alias.
///
/// Instead of emitting a full enum, this generates `pub type Name = Option<InnerType>`.
/// If the inner type is complex (struct, list), sub-types are generated first.
pub(crate) fn generate_toplevel_option_alias(
    name: &str,
    var_type: &prompt_templates::VarType,
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
