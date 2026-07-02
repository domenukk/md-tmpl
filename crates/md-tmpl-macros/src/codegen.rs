use quote::{format_ident, quote};

use crate::{crate_path, type_gen::string_to_variant_ident};

pub(crate) fn codegen_segment(seg: &md_tmpl::compiled::Segment) -> proc_macro2::TokenStream {
    use md_tmpl::compiled::Segment;
    let cp = crate_path();
    match seg {
        Segment::Static(s) => quote! {
            #cp::compiled::Segment::Static(#cp::__private::Cow::Borrowed(#s))
        },
        Segment::Expr { expr, filters } => {
            let filters_tokens = filters.iter().map(codegen_parsed_filter);
            let expr_tokens = codegen_compiled_expr(expr);
            quote! {
                #cp::compiled::Segment::Expr {
                    expr: #expr_tokens,
                    filters: #cp::__private::vec![#(#filters_tokens),*],
                }
            }
        }
        Segment::ForLoop {
            binding,
            list_expr,
            body,
            else_body,
        } => {
            let body_tokens = body.iter().map(codegen_segment);
            let else_body_tokens = else_body.iter().map(codegen_segment);
            let list_expr_tokens = codegen_compiled_expr(list_expr);
            quote! {
                #cp::compiled::Segment::ForLoop {
                    binding: #cp::__private::Cow::Borrowed(#binding),
                    list_expr: #list_expr_tokens,
                    body: #cp::__private::vec![#(#body_tokens),*],
                    else_body: #cp::__private::vec![#(#else_body_tokens),*],
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
                    (#cond_tokens, #cp::__private::vec![#(#body_tokens),*])
                }
            });
            let else_tokens = else_body.iter().map(codegen_segment);
            quote! {
                #cp::compiled::Segment::If {
                    branches: #cp::__private::vec![#(#branch_tokens),*],
                    else_body: #cp::__private::vec![#(#else_tokens),*],
                }
            }
        }
        Segment::Raw(s) => quote! {
            #cp::compiled::Segment::Raw(#cp::__private::Cow::Borrowed(#s))
        },
        Segment::Comment(refs) => {
            quote! {
                #cp::compiled::Segment::Comment(#cp::__private::vec![#(#cp::__private::Cow::Borrowed(#refs)),*])
            }
        }
        Segment::Include(inc) => codegen_segment_include(inc),
        Segment::Match {
            expr,
            arms,
            is_option,
        } => codegen_segment_match(expr, arms, *is_option),
        Segment::Panic(segs) => {
            let segs_tokens = segs.iter().map(codegen_segment);
            quote! {
                #cp::compiled::Segment::Panic(#cp::__private::vec![#(#segs_tokens),*])
            }
        }
    }
}

fn codegen_segment_include(inc: &md_tmpl::compiled::CompiledInclude) -> proc_macro2::TokenStream {
    let cp = crate_path();
    let path = &inc.path;
    let with_vars = inc.with_vars.iter().map(|(k, v)| {
        quote! { (#cp::__private::Cow::Borrowed(#k), #cp::__private::Cow::Borrowed(#v)) }
    });
    let for_each = inc.for_each.as_ref().map_or_else(
        || quote! { ::core::option::Option::None },
        |(b, l)| quote! { ::core::option::Option::Some((#cp::__private::Cow::Borrowed(#b), #cp::__private::Cow::Borrowed(#l))) },
    );
    let inline_compiled = inc.inline_compiled.as_ref().map_or_else(
        || quote! { ::core::option::Option::None },
        |ic| {
            let ic_tokens = codegen_compiled_inline_template(ic);
            quote! { ::core::option::Option::Some(#ic_tokens) }
        },
    );
    quote! {
        #cp::compiled::Segment::Include(
            #cp::compiled::CompiledInclude {
                path: #cp::__private::Cow::Borrowed(#path),
                with_vars: #cp::__private::vec![#(#with_vars),*],
                for_each: #for_each,
                inline_compiled: #inline_compiled,
            }
        )
    }
}

fn codegen_segment_match(
    expr: &md_tmpl::compiled::CompiledPath,
    arms: &[md_tmpl::compiled::MatchArm],
    is_option: bool,
) -> proc_macro2::TokenStream {
    let cp = crate_path();
    let arm_tokens = arms.iter().map(|arm| {
        let body_tokens = arm.body.iter().map(codegen_segment);
        let variant_tokens = arm.variants.iter().map(|v| {
            quote! { #cp::__private::Cow::Borrowed(#v) }
        });
        let guard_tokens = arm.guard.as_ref().map_or_else(
            || quote! { ::core::option::Option::None },
            |g| {
                let g_tokens = codegen_condition(g);
                quote! { ::core::option::Option::Some(#g_tokens) }
            },
        );
        quote! {
            #cp::compiled::MatchArm {
                variants: #cp::__private::vec![#(#variant_tokens),*],
                guard: #guard_tokens,
                body: #cp::__private::vec![#(#body_tokens),*],
            }
        }
    });
    let expr_tokens = codegen_compiled_path(expr);
    quote! {
        #cp::compiled::Segment::Match {
            expr: #expr_tokens,
            arms: #cp::__private::vec![#(#arm_tokens),*],
            is_option: #is_option,
        }
    }
}

pub(crate) fn codegen_parsed_filter(
    f: &md_tmpl::compiled::ParsedFilter,
) -> proc_macro2::TokenStream {
    let cp = crate_path();
    let kind = codegen_filter_kind(f.kind);
    let args = f.args.as_ref().map_or_else(
        || quote! { ::core::option::Option::None },
        |a| quote! { ::core::option::Option::Some(#cp::__private::Cow::Borrowed(#a)) },
    );
    let parsed_num = f.parsed_num.map_or_else(
        || quote! { ::core::option::Option::None },
        |n| quote! { ::core::option::Option::Some(#n) },
    );
    quote! {
        #cp::compiled::ParsedFilter {
            kind: #kind,
            args: #args,
            parsed_num: #parsed_num,
        }
    }
}

pub(crate) fn codegen_filter_kind(k: md_tmpl::compiled::FilterKind) -> proc_macro2::TokenStream {
    use md_tmpl::compiled::FilterKind;
    let cp = crate_path();
    match k {
        FilterKind::Upper => quote! { #cp::compiled::FilterKind::Upper },
        FilterKind::Lower => quote! { #cp::compiled::FilterKind::Lower },
        FilterKind::Trim => quote! { #cp::compiled::FilterKind::Trim },
        FilterKind::Fixed => quote! { #cp::compiled::FilterKind::Fixed },
        FilterKind::Join => quote! { #cp::compiled::FilterKind::Join },
        FilterKind::Limit => quote! { #cp::compiled::FilterKind::Limit },
        FilterKind::Add => quote! { #cp::compiled::FilterKind::Add },
        FilterKind::Sub => quote! { #cp::compiled::FilterKind::Sub },
    }
}

pub(crate) fn codegen_condition(c: &md_tmpl::compiled::Condition) -> proc_macro2::TokenStream {
    use md_tmpl::compiled::Condition;
    let cp = crate_path();
    match c {
        Condition::Truthy(v) => {
            let operand_tokens = codegen_condition_operand(v);
            quote! {
                #cp::compiled::Condition::Truthy(#operand_tokens)
            }
        }
        Condition::Not(inner) => {
            let inner_tokens = codegen_condition(inner);
            quote! {
                #cp::compiled::Condition::Not(#cp::__private::Box::new(#inner_tokens))
            }
        }
        Condition::And(left, right) => {
            let left_tokens = codegen_condition(left);
            let right_tokens = codegen_condition(right);
            quote! {
                #cp::compiled::Condition::And(
                    #cp::__private::Box::new(#left_tokens),
                    #cp::__private::Box::new(#right_tokens),
                )
            }
        }
        Condition::Or(left, right) => {
            let left_tokens = codegen_condition(left);
            let right_tokens = codegen_condition(right);
            quote! {
                #cp::compiled::Condition::Or(
                    #cp::__private::Box::new(#left_tokens),
                    #cp::__private::Box::new(#right_tokens),
                )
            }
        }
        Condition::Comparison { left, op, right } => {
            let op_tokens = codegen_comparison_op(*op);
            let left_tokens = codegen_condition_operand(left);
            let right_tokens = codegen_condition_operand(right);
            quote! {
                #cp::compiled::Condition::Comparison {
                    left: #left_tokens,
                    op: #op_tokens,
                    right: #right_tokens,
                }
            }
        }
        Condition::MatchVariant {
            expr,
            variants,
            is_option,
        } => {
            let expr_tokens = codegen_compiled_path(expr);
            let variant_tokens = variants.iter().map(|v| {
                quote! { #cp::__private::Cow::Borrowed(#v) }
            });
            quote! {
                #cp::compiled::Condition::MatchVariant {
                    expr: #expr_tokens,
                    variants: #cp::__private::vec![#(#variant_tokens),*],
                    is_option: #is_option,
                }
            }
        }
    }
}

fn codegen_compiled_path(path: &md_tmpl::compiled::CompiledPath) -> proc_macro2::TokenStream {
    let cp = crate_path();
    let raw = path.as_str();
    let parts: Vec<&str> = path.parts().iter().map(String::as_str).collect();
    quote! { #cp::compiled::CompiledPath::from_static(#raw, &[#(#parts),*]) }
}

fn codegen_compiled_expr(expr: &md_tmpl::compiled::CompiledExpr) -> proc_macro2::TokenStream {
    use md_tmpl::compiled::CompiledExpr;
    let cp = crate_path();
    match expr {
        CompiledExpr::Path(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::CompiledExpr::Path(#path_tokens) }
        }
        CompiledExpr::Idx(binding) => {
            let binding = binding.as_ref();
            quote! { #cp::compiled::CompiledExpr::Idx(#cp::__private::Cow::Borrowed(#binding)) }
        }
        CompiledExpr::Len(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::CompiledExpr::Len(#path_tokens) }
        }
        CompiledExpr::Kind(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::CompiledExpr::Kind(#path_tokens) }
        }
        CompiledExpr::Kinds(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::CompiledExpr::Kinds(#path_tokens) }
        }
        CompiledExpr::Has(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::CompiledExpr::Has(#path_tokens) }
        }
    }
}

fn codegen_condition_operand(op: &md_tmpl::compiled::ConditionOperand) -> proc_macro2::TokenStream {
    use md_tmpl::compiled::ConditionOperand;
    let cp = crate_path();
    match op {
        ConditionOperand::Literal(val) => {
            let val_tokens = codegen_value(val);
            quote! { #cp::compiled::ConditionOperand::Literal(#val_tokens) }
        }
        ConditionOperand::Path { path, filters } => {
            let path_tokens = codegen_compiled_path(path);
            let filters_tokens = filters.iter().map(codegen_parsed_filter);
            quote! {
                #cp::compiled::ConditionOperand::Path {
                    path: #path_tokens,
                    filters: #cp::__private::vec![#(#filters_tokens),*],
                }
            }
        }
        ConditionOperand::Idx(binding) => {
            let binding = binding.as_ref();
            quote! { #cp::compiled::ConditionOperand::Idx(#cp::__private::Cow::Borrowed(#binding)) }
        }
        ConditionOperand::Len(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::ConditionOperand::Len(#path_tokens) }
        }
        ConditionOperand::Kind(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::ConditionOperand::Kind(#path_tokens) }
        }
        ConditionOperand::Kinds(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::ConditionOperand::Kinds(#path_tokens) }
        }
        ConditionOperand::Has(path) => {
            let path_tokens = codegen_compiled_path(path);
            quote! { #cp::compiled::ConditionOperand::Has(#path_tokens) }
        }
        ConditionOperand::InterpolatedStr(segs) => {
            let segs_tokens = segs.iter().map(codegen_segment);
            quote! { #cp::compiled::ConditionOperand::InterpolatedStr(#cp::__private::vec![#(#segs_tokens),*]) }
        }
    }
}

pub(crate) fn codegen_comparison_op(
    op: md_tmpl::compiled::ComparisonOp,
) -> proc_macro2::TokenStream {
    use md_tmpl::compiled::ComparisonOp;
    let cp = crate_path();
    match op {
        ComparisonOp::Eq => quote! { #cp::compiled::ComparisonOp::Eq },
        ComparisonOp::Ne => quote! { #cp::compiled::ComparisonOp::Ne },
        ComparisonOp::Le => quote! { #cp::compiled::ComparisonOp::Le },
        ComparisonOp::Ge => quote! { #cp::compiled::ComparisonOp::Ge },
        ComparisonOp::Lt => quote! { #cp::compiled::ComparisonOp::Lt },
        ComparisonOp::Gt => quote! { #cp::compiled::ComparisonOp::Gt },
        ComparisonOp::In => quote! { #cp::compiled::ComparisonOp::In },
    }
}

pub(crate) fn codegen_compiled_inline_template(
    t: &md_tmpl::compiled::CompiledInlineTemplate,
) -> proc_macro2::TokenStream {
    let cp = crate_path();
    let segments_tokens = t.segments.iter().map(codegen_segment);
    let decls_tokens = t.declarations.iter().map(codegen_var_decl);
    let const_entries = t.consts.iter().map(|(k, v)| {
        let v_tokens = codegen_value(v);
        quote! { (#cp::__private::String::from(#k), #v_tokens) }
    });
    let imported_const_entries = t.imported_consts.iter().map(|(k, v)| {
        let v_tokens = codegen_value(v);
        quote! { (#cp::__private::String::from(#k), #v_tokens) }
    });
    quote! {
        #cp::compiled::CompiledInlineTemplate {
            segments: #cp::__private::Arc::from([#(#segments_tokens),*]),
            declarations: #cp::__private::Arc::from([#(#decls_tokens),*]),
            consts: #cp::__private::Arc::new([#(#const_entries),*].into_iter().collect()),
            imported_consts: #cp::__private::Arc::new([#(#imported_const_entries),*].into_iter().collect()),
        }
    }
}

pub(crate) fn codegen_var_decl(d: &md_tmpl::VarDecl) -> proc_macro2::TokenStream {
    let cp = crate_path();
    let name = &d.name;
    let type_tokens = codegen_var_type(&d.var_type);
    let default_tokens = if let Some(v) = &d.default_value {
        let v_tokens = codegen_value(v);
        quote! { ::core::option::Option::Some(#v_tokens) }
    } else {
        quote! { ::core::option::Option::None }
    };
    quote! {
        #cp::VarDecl {
            name: #cp::__private::String::from(#name),
            var_type: #type_tokens,
            default_value: #default_tokens,
        }
    }
}

pub(crate) fn codegen_value(v: &md_tmpl::Value) -> proc_macro2::TokenStream {
    use md_tmpl::Value;
    let cp = crate_path();
    match v {
        Value::Str(s) => {
            quote! { #cp::Value::Str(#cp::__private::String::from(#s)) }
        }
        Value::Int(i) => quote! { #cp::Value::Int(#i) },
        Value::Float(f) => quote! { #cp::Value::Float(#f) },
        Value::Bool(b) => quote! { #cp::Value::Bool(#b) },
        Value::List(l) => {
            let items = l.iter().map(codegen_value);
            quote! { #cp::Value::List(#cp::__private::Arc::new(#cp::__private::vec![#(#items),*])) }
        }
        Value::Struct(d) => {
            let entries = d.iter().map(|(k, v)| {
                let v_tokens = codegen_value(v);
                quote! { (#cp::__private::String::from(#k), #v_tokens) }
            });
            quote! {
                #cp::Value::Struct(
                    #cp::__private::Arc::new([#(#entries),*].into_iter().collect())
                )
            }
        }
        Value::Tmpl(_) => {
            quote! {
                compile_error!("Value::Tmpl cannot be used as a compile-time constant literal")
            }
        }
        Value::None => quote! { #cp::Value::None },
    }
}

pub(crate) fn codegen_var_type(t: &md_tmpl::VarType) -> proc_macro2::TokenStream {
    use md_tmpl::VarType;
    let cp = crate_path();
    match t {
        VarType::Str => quote! { #cp::VarType::Str },
        VarType::Bool => quote! { #cp::VarType::Bool },
        VarType::Int => quote! { #cp::VarType::Int },
        VarType::Float => quote! { #cp::VarType::Float },
        VarType::List(fields) => {
            let fields_tokens = fields.iter().map(codegen_var_decl);
            quote! { #cp::VarType::List(#cp::__private::vec![#(#fields_tokens),*]) }
        }
        VarType::Struct(fields) => {
            let fields_tokens = fields.iter().map(codegen_var_decl);
            quote! { #cp::VarType::Struct(#cp::__private::vec![#(#fields_tokens),*]) }
        }
        VarType::Enum(variants) => {
            let variants_tokens = variants.iter().map(codegen_variant_decl);
            quote! { #cp::VarType::Enum(#cp::__private::vec![#(#variants_tokens),*]) }
        }
        VarType::Tmpl(fields) => {
            let fields_tokens = fields.iter().map(codegen_var_decl);
            quote! { #cp::VarType::Tmpl(#cp::__private::vec![#(#fields_tokens),*]) }
        }
        VarType::Option(inner) => {
            let inner_tokens = codegen_var_type(inner);
            quote! { #cp::VarType::Option(#cp::__private::Box::new(#inner_tokens)) }
        }
    }
}

pub(crate) fn codegen_variant_decl(v: &md_tmpl::VariantDecl) -> proc_macro2::TokenStream {
    let cp = crate_path();
    let name = &v.name;
    let fields_tokens = v.fields.iter().map(codegen_var_decl);
    quote! {
        #cp::VariantDecl {
            name: #cp::__private::String::from(#name),
            fields: #cp::__private::vec![#(#fields_tokens),*],
        }
    }
}

pub(crate) fn is_scalar(t: &md_tmpl::VarType) -> bool {
    use md_tmpl::VarType;
    matches!(
        t,
        VarType::Str | VarType::Int | VarType::Float | VarType::Bool
    )
}

/// Generate a Rust literal for a `List` value with typed fields.
///
/// Handles both single-anonymous-field lists and named-field struct lists.
pub(crate) fn codegen_list_literal(
    items: &[md_tmpl::Value],
    fields: &[md_tmpl::VarDecl],
    parent_struct: &str,
    field_name: &str,
) -> proc_macro2::TokenStream {
    use md_tmpl::Value;
    let cp = crate_path();

    if fields.len() == 1 && fields[0].name.is_empty() {
        let item_tokens = items.iter().map(|item| {
            codegen_value_as_rust_literal(item, &fields[0].var_type, parent_struct, field_name)
        });
        quote! { #cp::__private::vec![#(#item_tokens),*] }
    } else {
        let capitalized = md_tmpl::to_pascal_case(field_name);
        let item_struct_name = format_ident!("{parent_struct}{capitalized}Item");
        let item_tokens = items.iter().map(|item| {
            if let Value::Struct(d) = item {
                let field_tokens = fields.iter().map(|f_decl| {
                    let f_name = format_ident!("{}", f_decl.name);
                    let f_tokens = if let Some(f_val) = d.get(&f_decl.name) {
                        codegen_value_as_rust_literal(
                            f_val,
                            &f_decl.var_type,
                            &format!("{parent_struct}{capitalized}Item"),
                            &f_decl.name,
                        )
                    } else {
                        let msg =
                            format!("missing field `{}` in constant list item dict", f_decl.name);
                        quote! { compile_error!(#msg) }
                    };
                    quote! { #f_name: #f_tokens }
                });
                quote! { #item_struct_name { #(#field_tokens),* } }
            } else {
                let msg = format!(
                    "type mismatch in constant: expected Struct for list item, got {item:?}"
                );
                quote! { compile_error!(#msg) }
            }
        });
        quote! { #cp::__private::vec![#(#item_tokens),*] }
    }
}

pub(crate) fn codegen_value_as_rust_literal(
    v: &md_tmpl::Value,
    t: &md_tmpl::VarType,
    parent_struct: &str,
    field_name: &str,
) -> proc_macro2::TokenStream {
    use md_tmpl::{Value, VarType};
    let cp = crate_path();

    match (v, t) {
        (Value::Str(s), VarType::Str) => quote! { #cp::__private::String::from(#s) },
        (Value::Int(i), VarType::Int) => quote! { #i },
        (Value::Float(f), VarType::Float) => quote! { #f },
        (Value::Bool(b), VarType::Bool) => quote! { #b },
        (Value::List(items), VarType::List(fields)) => {
            codegen_list_literal(items, fields, parent_struct, field_name)
        }
        (Value::Struct(d), VarType::Struct(fields)) => {
            codegen_struct_literal(d, fields, parent_struct, field_name)
        }
        // option(T): Value::Str("None") → Rust None
        (Value::Str(s), VarType::Enum(_)) if t.is_option() && s == "None" => {
            quote! { ::core::option::Option::None }
        }
        // option(T): Value::Struct({__kind__: "Some", val: v}) → Rust Some(inner)
        (Value::Struct(d), VarType::Enum(_)) if t.is_option() => {
            let inner_vt = t
                .option_inner_type()
                .expect("is_option() was true but option_inner_type() is None");
            let Some(inner_val) = d.get("val") else {
                let msg = format!("option constant for `{field_name}` is missing `val` field");
                return quote! { compile_error!(#msg) };
            };
            let inner_tokens =
                codegen_value_as_rust_literal(inner_val, inner_vt, parent_struct, field_name);
            quote! { ::core::option::Option::Some(#inner_tokens) }
        }
        (Value::Str(s), VarType::Enum(variants)) => {
            let Some(variant) = variants
                .iter()
                .find(|v| v.name == *s && v.fields.is_empty())
            else {
                let msg = format!("unknown or non-unit enum variant `{s}` in constant");
                return quote! { compile_error!(#msg) };
            };
            let capitalized = md_tmpl::to_pascal_case(field_name);
            let (var_ident, _) = string_to_variant_ident(&variant.name);
            let enum_name = format_ident!("{parent_struct}{capitalized}");
            quote! { #enum_name::#var_ident }
        }
        (Value::Struct(d), VarType::Enum(variants)) => {
            codegen_data_enum_literal(d, variants, parent_struct, field_name)
        }
        _ => {
            let msg =
                format!("type mismatch in constant: cannot generate literal for {v:?} as {t:?}");
            quote! { compile_error!(#msg) }
        }
    }
}

/// Generate a Rust struct literal from a constant `Value::Struct`.
fn codegen_struct_literal(
    d: &std::sync::Arc<hashbrown::HashMap<String, md_tmpl::Value>>,
    fields: &[md_tmpl::VarDecl],
    parent_struct: &str,
    field_name: &str,
) -> proc_macro2::TokenStream {
    let capitalized = md_tmpl::to_pascal_case(field_name);
    let struct_name = format_ident!("{parent_struct}{capitalized}");
    let field_tokens = fields.iter().map(|f_decl| {
        let f_name = format_ident!("{}", f_decl.name);
        let f_tokens = if let Some(f_val) = d.get(&f_decl.name) {
            codegen_value_as_rust_literal(
                f_val,
                &f_decl.var_type,
                &format!("{parent_struct}{capitalized}"),
                &f_decl.name,
            )
        } else {
            let msg = format!("missing field `{}` in constant dict", f_decl.name);
            quote! { compile_error!(#msg) }
        };
        quote! { #f_name: #f_tokens }
    });
    quote! { #struct_name { #(#field_tokens),* } }
}

/// Generate a Rust data-enum literal from a constant `Value::Struct` with `__kind__`.
fn codegen_data_enum_literal(
    d: &std::sync::Arc<hashbrown::HashMap<String, md_tmpl::Value>>,
    variants: &[md_tmpl::VariantDecl],
    parent_struct: &str,
    field_name: &str,
) -> proc_macro2::TokenStream {
    use md_tmpl::Value;

    let Some(kind) = d
        .get("__kind__")
        .and_then(|v| if let Value::Str(s) = v { Some(s) } else { None })
    else {
        let msg =
            format!("enum dict constant is missing `__kind__` string tag for field `{field_name}`");
        return quote! { compile_error!(#msg) };
    };

    let Some(variant) = variants.iter().find(|v| v.name == *kind) else {
        let msg = format!("unknown enum variant `{kind}` in constant for field `{field_name}`");
        return quote! { compile_error!(#msg) };
    };
    let capitalized = md_tmpl::to_pascal_case(field_name);
    let (var_ident, _) = string_to_variant_ident(&variant.name);
    let enum_name = format_ident!("{parent_struct}{capitalized}");

    if variant.fields.is_empty() {
        quote! { #enum_name::#var_ident }
    } else {
        let field_tokens = variant.fields.iter().map(|f_decl| {
            let f_name = format_ident!("{}", f_decl.name);
            let f_tokens = if let Some(f_val) = d.get(&f_decl.name) {
                codegen_value_as_rust_literal(
                    f_val,
                    &f_decl.var_type,
                    &format!("{parent_struct}{capitalized}{var_ident}"),
                    &f_decl.name,
                )
            } else {
                let msg = format!(
                    "missing field `{}` in enum variant `{kind}` constant",
                    f_decl.name
                );
                quote! { compile_error!(#msg) }
            };
            quote! { #f_name: #f_tokens }
        });
        quote! { #enum_name::#var_ident { #(#field_tokens),* } }
    }
}
