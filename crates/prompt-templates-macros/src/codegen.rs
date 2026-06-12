use quote::{format_ident, quote};

use crate::{compile::to_pascal_case, type_gen::string_to_variant_ident};

pub(crate) fn codegen_segment(
    seg: &prompt_templates::compiled::Segment,
) -> proc_macro2::TokenStream {
    use prompt_templates::compiled::Segment;
    match seg {
        Segment::Static(s) => quote! {
            ::prompt_templates::compiled::Segment::Static(::std::borrow::Cow::Borrowed(#s))
        },
        Segment::Expr { path, filters } => {
            let filters_tokens = filters.iter().map(codegen_parsed_filter);
            quote! {
                ::prompt_templates::compiled::Segment::Expr {
                    path: ::std::borrow::Cow::Borrowed(#path),
                    filters: ::std::vec![#(#filters_tokens),*],
                }
            }
        }
        Segment::ForLoop {
            binding,
            list_path,
            body,
        } => {
            let body_tokens = body.iter().map(codegen_segment);
            quote! {
                ::prompt_templates::compiled::Segment::ForLoop {
                    binding: ::std::borrow::Cow::Borrowed(#binding),
                    list_path: ::std::borrow::Cow::Borrowed(#list_path),
                    body: ::std::vec![#(#body_tokens),*],
                }
            }
        }
        Segment::If {
            branches,
            else_body,
        } => {
            let branch_tokens = branches.iter().map(|(cond, body)| {
                let cond_tokens = codegen_condition(cond);
                let body_tokens = body.iter().map(codegen_segment);
                quote! {
                    (#cond_tokens, ::std::vec![#(#body_tokens),*])
                }
            });
            let else_tokens = else_body.iter().map(codegen_segment);
            quote! {
                ::prompt_templates::compiled::Segment::If {
                    branches: ::std::vec![#(#branch_tokens),*],
                    else_body: ::std::vec![#(#else_tokens),*],
                }
            }
        }
        Segment::Raw(s) => quote! {
            ::prompt_templates::compiled::Segment::Raw(::std::borrow::Cow::Borrowed(#s))
        },
        Segment::Comment(refs) => {
            quote! {
                ::prompt_templates::compiled::Segment::Comment(::std::vec![#(::std::borrow::Cow::Borrowed(#refs)),*])
            }
        }
        Segment::Include(inc) => {
            let path = &inc.path;
            let with_vars = inc.with_vars.iter().map(|(k, v)| {
                quote! { (::std::borrow::Cow::Borrowed(#k), ::std::borrow::Cow::Borrowed(#v)) }
            });
            let for_each = inc.for_each.as_ref().map_or_else(
                || quote! { ::std::option::Option::None },
                |(b, l)| quote! { ::std::option::Option::Some((::std::borrow::Cow::Borrowed(#b), ::std::borrow::Cow::Borrowed(#l))) },
            );
            let inline_compiled = inc.inline_compiled.as_ref().map_or_else(
                || quote! { ::std::option::Option::None },
                |ic| {
                    let ic_tokens = codegen_compiled_inline_template(ic);
                    quote! { ::std::option::Option::Some(#ic_tokens) }
                },
            );
            quote! {
                ::prompt_templates::compiled::Segment::Include(
                    ::prompt_templates::compiled::CompiledInclude {
                        path: ::std::borrow::Cow::Borrowed(#path),
                        with_vars: ::std::vec![#(#with_vars),*],
                        for_each: #for_each,
                        inline_compiled: #inline_compiled,
                    }
                )
            }
        }
        Segment::Match { expr, arms } => {
            let arm_tokens = arms.iter().map(|(variants, body)| {
                let body_tokens = body.iter().map(codegen_segment);
                let variant_tokens = variants.iter().map(|v| {
                    quote! { ::std::borrow::Cow::Borrowed(#v) }
                });
                quote! {
                    (::std::vec![#(#variant_tokens),*], ::std::vec![#(#body_tokens),*])
                }
            });
            quote! {
                ::prompt_templates::compiled::Segment::Match {
                    expr: ::std::borrow::Cow::Borrowed(#expr),
                    arms: ::std::vec![#(#arm_tokens),*],
                }
            }
        }
    }
}

pub(crate) fn codegen_parsed_filter(
    f: &prompt_templates::compiled::ParsedFilter,
) -> proc_macro2::TokenStream {
    let kind = codegen_filter_kind(f.kind);
    let args = f.args.as_ref().map_or_else(
        || quote! { ::std::option::Option::None },
        |a| quote! { ::std::option::Option::Some(::std::borrow::Cow::Borrowed(#a)) },
    );
    quote! {
        ::prompt_templates::compiled::ParsedFilter {
            kind: #kind,
            args: #args,
        }
    }
}

pub(crate) fn codegen_filter_kind(
    k: prompt_templates::compiled::FilterKind,
) -> proc_macro2::TokenStream {
    use prompt_templates::compiled::FilterKind;
    match k {
        FilterKind::Upper => quote! { ::prompt_templates::compiled::FilterKind::Upper },
        FilterKind::Lower => quote! { ::prompt_templates::compiled::FilterKind::Lower },
        FilterKind::Trim => quote! { ::prompt_templates::compiled::FilterKind::Trim },
        FilterKind::Fixed => quote! { ::prompt_templates::compiled::FilterKind::Fixed },
        FilterKind::Join => quote! { ::prompt_templates::compiled::FilterKind::Join },
        FilterKind::Limit => quote! { ::prompt_templates::compiled::FilterKind::Limit },
        FilterKind::Add => quote! { ::prompt_templates::compiled::FilterKind::Add },
        FilterKind::Sub => quote! { ::prompt_templates::compiled::FilterKind::Sub },
    }
}

pub(crate) fn codegen_condition(
    c: &prompt_templates::compiled::Condition,
) -> proc_macro2::TokenStream {
    use prompt_templates::compiled::Condition;
    match c {
        Condition::Truthy(v) => quote! {
            ::prompt_templates::compiled::Condition::Truthy(::std::borrow::Cow::Borrowed(#v))
        },
        Condition::Comparison { left, op, right } => {
            let op_tokens = codegen_comparison_op(*op);
            quote! {
                ::prompt_templates::compiled::Condition::Comparison {
                    left: ::std::borrow::Cow::Borrowed(#left),
                    op: #op_tokens,
                    right: ::std::borrow::Cow::Borrowed(#right),
                }
            }
        }
    }
}

pub(crate) fn codegen_comparison_op(
    op: prompt_templates::compiled::ComparisonOp,
) -> proc_macro2::TokenStream {
    use prompt_templates::compiled::ComparisonOp;
    match op {
        ComparisonOp::Eq => quote! { ::prompt_templates::compiled::ComparisonOp::Eq },
        ComparisonOp::Ne => quote! { ::prompt_templates::compiled::ComparisonOp::Ne },
        ComparisonOp::Le => quote! { ::prompt_templates::compiled::ComparisonOp::Le },
        ComparisonOp::Ge => quote! { ::prompt_templates::compiled::ComparisonOp::Ge },
        ComparisonOp::Lt => quote! { ::prompt_templates::compiled::ComparisonOp::Lt },
        ComparisonOp::Gt => quote! { ::prompt_templates::compiled::ComparisonOp::Gt },
    }
}

pub(crate) fn codegen_compiled_inline_template(
    t: &prompt_templates::compiled::CompiledInlineTemplate,
) -> proc_macro2::TokenStream {
    let segments_tokens = t.segments.iter().map(codegen_segment);
    let decls_tokens = t.declarations.iter().map(codegen_var_decl);
    quote! {
        ::prompt_templates::compiled::CompiledInlineTemplate {
            segments: ::std::sync::Arc::from([#(#segments_tokens),*]),
            declarations: ::std::sync::Arc::from([#(#decls_tokens),*]),
        }
    }
}

pub(crate) fn codegen_var_decl(d: &prompt_templates::VarDecl) -> proc_macro2::TokenStream {
    let name = &d.name;
    let type_tokens = codegen_var_type(&d.var_type);
    let default_tokens = if let Some(v) = &d.default_value {
        let v_tokens = codegen_value(v);
        quote! { ::std::option::Option::Some(#v_tokens) }
    } else {
        quote! { ::std::option::Option::None }
    };
    quote! {
        ::prompt_templates::VarDecl {
            name: ::std::string::String::from(#name),
            var_type: #type_tokens,
            default_value: #default_tokens,
        }
    }
}

pub(crate) fn codegen_value(v: &prompt_templates::Value) -> proc_macro2::TokenStream {
    use prompt_templates::Value;
    match v {
        Value::Str(s) => quote! { ::prompt_templates::Value::Str(::std::string::String::from(#s)) },
        Value::Int(i) => quote! { ::prompt_templates::Value::Int(#i) },
        Value::Float(f) => quote! { ::prompt_templates::Value::Float(#f) },
        Value::Bool(b) => quote! { ::prompt_templates::Value::Bool(#b) },
        Value::List(l) => {
            let items = l.iter().map(codegen_value);
            quote! { ::prompt_templates::Value::List(::std::vec![#(#items),*]) }
        }
        Value::Dict(d) => {
            let entries = d.iter().map(|(k, v)| {
                let v_tokens = codegen_value(v);
                quote! { (::std::string::String::from(#k), #v_tokens) }
            });
            quote! {
                ::prompt_templates::Value::Dict(
                    [#(#entries),*].into_iter().collect()
                )
            }
        }
        Value::Tmpl(_) => {
            quote! {
                compile_error!("Value::Tmpl cannot be used as a compile-time constant literal")
            }
        }
    }
}

pub(crate) fn codegen_var_type(t: &prompt_templates::VarType) -> proc_macro2::TokenStream {
    use prompt_templates::VarType;
    match t {
        VarType::Str => quote! { ::prompt_templates::VarType::Str },
        VarType::Bool => quote! { ::prompt_templates::VarType::Bool },
        VarType::Int => quote! { ::prompt_templates::VarType::Int },
        VarType::Float => quote! { ::prompt_templates::VarType::Float },
        VarType::List(fields) => {
            let fields_tokens = fields.iter().map(codegen_var_decl);
            quote! { ::prompt_templates::VarType::List(::std::vec![#(#fields_tokens),*]) }
        }
        VarType::Dict(fields) => {
            let fields_tokens = fields.iter().map(codegen_var_decl);
            quote! { ::prompt_templates::VarType::Dict(::std::vec![#(#fields_tokens),*]) }
        }
        VarType::Enum(variants) => {
            let variants_tokens = variants.iter().map(codegen_variant_decl);
            quote! { ::prompt_templates::VarType::Enum(::std::vec![#(#variants_tokens),*]) }
        }
        VarType::Tmpl(fields) => {
            let fields_tokens = fields.iter().map(codegen_var_decl);
            quote! { ::prompt_templates::VarType::Tmpl(::std::vec![#(#fields_tokens),*]) }
        }
    }
}

pub(crate) fn codegen_variant_decl(v: &prompt_templates::VariantDecl) -> proc_macro2::TokenStream {
    let name = &v.name;
    let fields_tokens = v.fields.iter().map(codegen_var_decl);
    quote! {
        ::prompt_templates::VariantDecl {
            name: ::std::string::String::from(#name),
            fields: ::std::vec![#(#fields_tokens),*],
        }
    }
}

pub(crate) fn is_scalar(t: &prompt_templates::VarType) -> bool {
    use prompt_templates::VarType;
    matches!(
        t,
        VarType::Str | VarType::Int | VarType::Float | VarType::Bool
    )
}

/// Generate a Rust literal for a `List` value with typed fields.
///
/// Handles both single-anonymous-field lists and named-field struct lists.
pub(crate) fn codegen_list_literal(
    items: &[prompt_templates::Value],
    fields: &[prompt_templates::VarDecl],
    parent_struct: &str,
    field_name: &str,
) -> proc_macro2::TokenStream {
    use prompt_templates::Value;

    if fields.len() == 1 && fields[0].name.is_empty() {
        let item_tokens = items.iter().map(|item| {
            codegen_value_as_rust_literal(item, &fields[0].var_type, parent_struct, field_name)
        });
        quote! { ::std::vec![#(#item_tokens),*] }
    } else {
        let capitalized = to_pascal_case(field_name);
        let item_struct_name = format_ident!("{parent_struct}{capitalized}Item");
        let item_tokens = items.iter().map(|item| {
            if let Value::Dict(d) = item {
                let field_tokens = fields.iter().map(|f_decl| {
                    let f_name = format_ident!("{}", f_decl.name);
                    let f_val = d.get(&f_decl.name).expect("value matches type");
                    let f_tokens = codegen_value_as_rust_literal(
                        f_val,
                        &f_decl.var_type,
                        &format!("{parent_struct}{capitalized}Item"),
                        &f_decl.name,
                    );
                    quote! { #f_name: #f_tokens }
                });
                quote! { #item_struct_name { #(#field_tokens),* } }
            } else {
                panic!("type mismatch in constant: expected Dict for list item, got {item:?}");
            }
        });
        quote! { ::std::vec![#(#item_tokens),*] }
    }
}

pub(crate) fn codegen_value_as_rust_literal(
    v: &prompt_templates::Value,
    t: &prompt_templates::VarType,
    parent_struct: &str,
    field_name: &str,
) -> proc_macro2::TokenStream {
    use prompt_templates::{Value, VarType};

    match (v, t) {
        (Value::Str(s), VarType::Str) => quote! { ::std::string::String::from(#s) },
        (Value::Int(i), VarType::Int) => quote! { #i },
        (Value::Float(f), VarType::Float) => quote! { #f },
        (Value::Bool(b), VarType::Bool) => quote! { #b },
        (Value::List(items), VarType::List(fields)) => {
            codegen_list_literal(items, fields, parent_struct, field_name)
        }
        (Value::Dict(d), VarType::Dict(fields)) => {
            let capitalized = to_pascal_case(field_name);
            let struct_name = format_ident!("{parent_struct}{capitalized}");
            let field_tokens = fields.iter().map(|f_decl| {
                let f_name = format_ident!("{}", f_decl.name);
                let f_val = d.get(&f_decl.name).expect("value matches type");
                let f_tokens = codegen_value_as_rust_literal(
                    f_val,
                    &f_decl.var_type,
                    &format!("{parent_struct}{capitalized}"),
                    &f_decl.name,
                );
                quote! { #f_name: #f_tokens }
            });
            quote! { #struct_name { #(#field_tokens),* } }
        }
        (Value::Str(s), VarType::Enum(variants)) => {
            let variant = variants
                .iter()
                .find(|v| v.name == *s && v.fields.is_empty())
                .expect("variant exists");
            let capitalized = to_pascal_case(field_name);
            let (var_ident, _) = string_to_variant_ident(&variant.name);
            let enum_name = format_ident!("{parent_struct}{capitalized}");
            quote! { #enum_name::#var_ident }
        }
        (Value::Dict(d), VarType::Enum(variants)) => {
            let kind = d
                .get("__kind__")
                .and_then(|v| if let Value::Str(s) = v { Some(s) } else { None })
                .expect("enum value has kind tag");

            let variant = variants
                .iter()
                .find(|v| v.name == *kind)
                .expect("variant exists");
            let capitalized = to_pascal_case(field_name);
            let (var_ident, _) = string_to_variant_ident(&variant.name);
            let enum_name = format_ident!("{parent_struct}{capitalized}");

            if variant.fields.is_empty() {
                quote! { #enum_name::#var_ident }
            } else {
                let field_tokens = variant.fields.iter().map(|f_decl| {
                    let f_name = format_ident!("{}", f_decl.name);
                    let f_val = d.get(&f_decl.name).expect("field value exists");
                    let f_tokens = codegen_value_as_rust_literal(
                        f_val,
                        &f_decl.var_type,
                        &format!("{parent_struct}{capitalized}{var_ident}"),
                        &f_decl.name,
                    );
                    quote! { #f_name: #f_tokens }
                });
                quote! { #enum_name::#var_ident { #(#field_tokens),* } }
            }
        }
        _ => {
            // Last resort: if we can't generate a raw literal, emit a Value literal
            // but this will likely fail if the target type is a struct/enum.
            let v_tokens = codegen_value(v);
            quote! { #v_tokens }
        }
    }
}
