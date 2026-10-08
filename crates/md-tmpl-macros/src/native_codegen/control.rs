//! Native Rust code generation for `{% if %}` conditionals, boolean conditions,
//! and comparisons.

use md_tmpl_core::{
    Value, VarType,
    compiled::{ComparisonOp, CompiledExpr, CompiledPath, Condition, ConditionOperand, Segment},
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::{
    Binding, NativeScope, NumExpr, ResolvedValue, codegen_expr, codegen_kind_expr_str,
    codegen_segments, fold_numeric_filters, is_copy_scalar, resolve_static_kinds, sanitize_ident,
    try_extract_num_expr,
};
use crate::{crate_path, type_gen::deduplicate_variant_idents};

/// Information needed to narrow an `Option` path inside a branch or condition.
#[derive(Clone)]
struct OptionNarrowing {
    path_str: String,
    opt_expr: TokenStream,
    inner_vt: VarType,
    type_prefix: String,
}

fn extract_has_narrowing(condition: &Condition, scope: &NativeScope) -> Option<OptionNarrowing> {
    let Condition::Truthy(ConditionOperand::Has(path) | ConditionOperand::Path { path, .. }) =
        condition
    else {
        return None;
    };
    let ResolvedValue::Runtime(b) = scope.resolve_path(path)? else {
        return None;
    };
    let VarType::Option(inner) = b.var_type else {
        return None;
    };
    Some(OptionNarrowing {
        path_str: path.as_str().to_string(),
        opt_expr: b.expr,
        inner_vt: (*inner).clone(),
        type_prefix: b.type_prefix,
    })
}

fn collect_and_narrowings(
    condition: &Condition,
    scope: &NativeScope,
    out: &mut Vec<OptionNarrowing>,
) {
    match condition {
        Condition::And(left, right) => {
            collect_and_narrowings(left, scope, out);
            let mut right_scope = scope.clone();
            for n in out.iter() {
                apply_option_narrowing_inline(&mut right_scope, n);
            }
            collect_and_narrowings(right, &right_scope, out);
        }
        other => {
            if let Some(n) = extract_has_narrowing(other, scope) {
                out.push(n);
            }
        }
    }
}

fn extract_not_has_narrowing(
    condition: &Condition,
    scope: &NativeScope,
) -> Option<OptionNarrowing> {
    let Condition::Not(inner) = condition else {
        return None;
    };
    extract_has_narrowing(inner, scope)
}

/// Apply an `OptionNarrowing` directly as an inline `.as_ref().expect(...)` expression
/// (used when evaluating the right-hand side of `has(x) && ...` where short-circuiting
/// guarantees `x.is_some()`).
fn apply_option_narrowing_inline(scope: &mut NativeScope, n: &OptionNarrowing) {
    let opt_e = &n.opt_expr;
    let expr = if is_copy_scalar(&n.inner_vt) {
        quote! { (*(#opt_e).as_ref().expect("option verified by short-circuiting has()")) }
    } else {
        quote! { (#opt_e).as_ref().expect("option verified by short-circuiting has()") }
    };
    scope.bindings.insert(
        n.path_str.clone(),
        Binding {
            expr,
            var_type: n.inner_vt.clone(),
            type_prefix: n.type_prefix.clone(),
        },
    );
}

/// Emit `let _md_opt_... = (#opt_e).as_ref().expect(...);` bindings at the start of a branch body.
fn bind_option_narrowings_in_block(
    scope: &mut NativeScope,
    narrowings: &[OptionNarrowing],
    counter: &mut usize,
) -> Vec<TokenStream> {
    let mut lets = Vec::with_capacity(narrowings.len());
    for n in narrowings {
        let id = *counter;
        *counter += 1;
        let val_ident = format_ident!("_md_opt_{id}_{}", sanitize_ident(&n.path_str));
        let opt_e = &n.opt_expr;
        lets.push(quote! {
            let #val_ident = (#opt_e).as_ref().expect("option verified by enclosing condition");
        });
        let bound_expr = if is_copy_scalar(&n.inner_vt) {
            quote! { (*#val_ident) }
        } else {
            quote! { #val_ident }
        };
        scope.bindings.insert(
            n.path_str.clone(),
            Binding {
                expr: bound_expr,
                var_type: n.inner_vt.clone(),
                type_prefix: n.type_prefix.clone(),
            },
        );
    }
    lets
}

pub(super) fn codegen_if(
    branches: &[(Condition, Vec<Segment>)],
    else_body: &[Segment],
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let mut branch_tokens = Vec::with_capacity(branches.len());
    let mut carry: Vec<OptionNarrowing> = Vec::new();

    for (idx, (cond, body)) in branches.iter().enumerate() {
        let mut cond_scope = scope.clone();
        for n in &carry {
            apply_option_narrowing_inline(&mut cond_scope, n);
        }

        // Fast-path single `has(x)` or truthy `Option` check with `if let Some(...)`.
        if carry.is_empty()
            && idx == 0
            && branches.len() == 1
            && let Some(n) = extract_has_narrowing(cond, &cond_scope)
        {
            let id = *counter;
            *counter += 1;
            let val_ident = format_ident!("_md_opt_{id}_{}", sanitize_ident(&n.path_str));
            let opt_e = &n.opt_expr;
            let bound_expr = if is_copy_scalar(&n.inner_vt) {
                quote! { (*#val_ident) }
            } else {
                quote! { #val_ident }
            };
            let mut body_scope = scope.clone();
            body_scope.bindings.insert(
                n.path_str,
                Binding {
                    expr: bound_expr,
                    var_type: n.inner_vt,
                    type_prefix: n.type_prefix,
                },
            );
            let body_stmts = codegen_segments(body, &body_scope, counter)?;
            return if else_body.is_empty() {
                Some(quote! {
                    if let ::core::option::Option::Some(#val_ident) = &(#opt_e) {
                        #body_stmts
                    }
                })
            } else {
                let else_stmts = codegen_segments(else_body, scope, counter)?;
                Some(quote! {
                    if let ::core::option::Option::Some(#val_ident) = &(#opt_e) {
                        #body_stmts
                    } else {
                        #else_stmts
                    }
                })
            };
        }

        let cond_expr = codegen_condition(cond, &cond_scope, counter)?;

        let mut branch_narrowings = carry.clone();
        collect_and_narrowings(cond, &cond_scope, &mut branch_narrowings);

        let mut body_scope = scope.clone();
        let narrow_lets =
            bind_option_narrowings_in_block(&mut body_scope, &branch_narrowings, counter);
        let body_stmts = codegen_segments(body, &body_scope, counter)?;

        if idx == 0 {
            branch_tokens.push(quote! {
                if #cond_expr {
                    #(#narrow_lets)*
                    #body_stmts
                }
            });
        } else {
            branch_tokens.push(quote! {
                else if #cond_expr {
                    #(#narrow_lets)*
                    #body_stmts
                }
            });
        }

        if let Some(neg) = extract_not_has_narrowing(cond, &cond_scope) {
            carry.push(neg);
        }
    }

    let else_part = if else_body.is_empty() {
        quote! {}
    } else {
        let mut else_scope = scope.clone();
        let narrow_lets = bind_option_narrowings_in_block(&mut else_scope, &carry, counter);
        let else_stmts = codegen_segments(else_body, &else_scope, counter)?;
        quote! {
            else {
                #(#narrow_lets)*
                #else_stmts
            }
        }
    };

    Some(quote! {
        #(#branch_tokens)*
        #else_part
    })
}

pub(super) fn codegen_condition(
    condition: &Condition,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    match condition {
        Condition::Truthy(operand) => codegen_truthy(operand, scope, counter),
        Condition::Not(inner) => {
            let inner_tok = codegen_condition(inner, scope, counter)?;
            Some(quote! { !(#inner_tok) })
        }
        Condition::And(left, right) => {
            let left_tok = codegen_condition(left, scope, counter)?;
            let mut right_scope = scope.clone();
            let mut narrowings = Vec::new();
            collect_and_narrowings(left, scope, &mut narrowings);
            for n in &narrowings {
                apply_option_narrowing_inline(&mut right_scope, n);
            }
            let right_tok = codegen_condition(right, &right_scope, counter)?;
            Some(quote! { ((#left_tok) && (#right_tok)) })
        }
        Condition::Or(left, right) => {
            let left_tok = codegen_condition(left, scope, counter)?;
            let right_tok = codegen_condition(right, scope, counter)?;
            Some(quote! { ((#left_tok) || (#right_tok)) })
        }
        Condition::Comparison { left, op, right } => {
            codegen_comparison(left, *op, right, scope, counter)
        }
        Condition::MatchVariant {
            expr,
            variants,
            is_option,
        } => codegen_match_variant_cond(expr, variants, *is_option, scope),
    }
}

fn codegen_truthy(
    operand: &ConditionOperand,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    match operand {
        ConditionOperand::Literal(val) => {
            let b = val.is_truthy();
            Some(quote! { #b })
        }
        ConditionOperand::Has(path) => {
            let ResolvedValue::Runtime(b) = scope.resolve_path(path)? else {
                return None;
            };
            if matches!(b.var_type, VarType::Option(_)) {
                let e = &b.expr;
                Some(quote! { (#e).is_some() })
            } else {
                None
            }
        }
        ConditionOperand::Idx(binding) => {
            let (idx_ident, _) = scope.loop_meta.get(binding.as_ref())?;
            Some(quote! { (#idx_ident != 0) })
        }
        ConditionOperand::Len(path) => match scope.resolve_path(path)? {
            ResolvedValue::Runtime(b) if matches!(b.var_type, VarType::List(_) | VarType::Str) => {
                let e = &b.expr;
                Some(quote! { !(#e).is_empty() })
            }
            ResolvedValue::Static(Value::List(l)) => {
                let non_empty = !l.is_empty();
                Some(quote! { #non_empty })
            }
            ResolvedValue::Static(Value::Str(s)) => {
                let non_empty = !s.is_empty();
                Some(quote! { #non_empty })
            }
            _ => None,
        },
        ConditionOperand::Kind(path) => {
            let k_expr = codegen_kind_expr_str(path, scope)?;
            Some(quote! { !(#k_expr).is_empty() })
        }
        ConditionOperand::Kinds(path) => {
            let names = resolve_static_kinds(path, scope)?;
            let non_empty = !names.is_empty();
            Some(quote! { #non_empty })
        }
        ConditionOperand::InterpolatedStr(segs) => {
            let seg_tokens = codegen_segments(segs, scope, counter)?;
            let cp = crate_path();
            Some(quote! {
                !({
                    let mut __interp = #cp::__private::String::new();
                    {
                        let __out = &mut __interp;
                        #seg_tokens
                    }
                    __interp
                }).is_empty()
            })
        }
        ConditionOperand::Path { path, filters } => {
            if !filters.is_empty() {
                let op_val = codegen_operand_typed(operand, scope, counter)?;
                return match op_val {
                    TypedOperand::Bool(e) => Some(e),
                    TypedOperand::Int(e) => Some(quote! { (#e != 0) }),
                    TypedOperand::Float(e) => Some(quote! { (#e != 0.0) }),
                    TypedOperand::Str(e) => Some(quote! { !(#e).is_empty() }),
                    TypedOperand::List { .. } => None,
                };
            }
            match scope.resolve_path(path)? {
                ResolvedValue::Static(val) => {
                    let b = val.is_truthy();
                    Some(quote! { #b })
                }
                ResolvedValue::Runtime(b) => {
                    let e = &b.expr;
                    match b.var_type {
                        VarType::Bool => Some(quote! { (#e) }),
                        VarType::Int => Some(quote! { (#e != 0) }),
                        VarType::Float => Some(quote! { (#e != 0.0) }),
                        VarType::Str | VarType::List(_) => Some(quote! { !(#e).is_empty() }),
                        VarType::Option(_) => Some(quote! { (#e).is_some() }),
                        _ => None,
                    }
                }
            }
        }
    }
}

/// Strongly-typed representation of a resolved [`ConditionOperand`].
pub(super) enum TypedOperand {
    Str(TokenStream),
    Int(TokenStream),
    Float(TokenStream),
    Bool(TokenStream),
    List { expr: TokenStream, elem_vt: VarType },
}

pub(super) fn codegen_operand_typed(
    operand: &ConditionOperand,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TypedOperand> {
    let cp = crate_path();
    match operand {
        ConditionOperand::Literal(val) => match val {
            Value::Str(s) => Some(TypedOperand::Str(quote! { #s })),
            Value::Int(i) => Some(TypedOperand::Int(quote! { #i })),
            Value::Float(f) => {
                let bits = f.to_bits();
                Some(TypedOperand::Float(quote! {
                    ::core::primitive::f64::from_bits(#bits)
                }))
            }
            Value::Bool(b) => Some(TypedOperand::Bool(quote! { #b })),
            _ => None,
        },
        ConditionOperand::Idx(binding) => {
            let (idx_ident, _) = scope.loop_meta.get(binding.as_ref())?;
            Some(TypedOperand::Int(quote! { #idx_ident }))
        }
        ConditionOperand::Len(path) => match scope.resolve_path(path)? {
            ResolvedValue::Runtime(b) if matches!(b.var_type, VarType::List(_) | VarType::Str) => {
                let e = &b.expr;
                Some(TypedOperand::Int(quote! {
                    i64::try_from((#e).len()).expect("len fits i64")
                }))
            }
            ResolvedValue::Static(Value::List(l)) => {
                let n = i64::try_from(l.len()).expect("collection length fits i64");
                Some(TypedOperand::Int(quote! { #n }))
            }
            ResolvedValue::Static(Value::Str(s)) => {
                let n = i64::try_from(s.len()).expect("string length fits i64");
                Some(TypedOperand::Int(quote! { #n }))
            }
            _ => None,
        },
        ConditionOperand::Has(path) => {
            let ResolvedValue::Runtime(b) = scope.resolve_path(path)? else {
                return None;
            };
            if matches!(b.var_type, VarType::Option(_)) {
                let e = &b.expr;
                Some(TypedOperand::Bool(quote! { (#e).is_some() }))
            } else {
                None
            }
        }
        ConditionOperand::Kind(path) => {
            let k_expr = codegen_kind_expr_str(path, scope)?;
            Some(TypedOperand::Str(k_expr))
        }
        ConditionOperand::Kinds(path) => {
            let names = resolve_static_kinds(path, scope)?;
            Some(TypedOperand::List {
                expr: quote! { [#(#names),*] },
                elem_vt: VarType::Str,
            })
        }
        ConditionOperand::InterpolatedStr(segs) => {
            let seg_tokens = codegen_segments(segs, scope, counter)?;
            Some(TypedOperand::Str(quote! {
                ({
                    let mut __interp = #cp::__private::String::new();
                    {
                        let __out = &mut __interp;
                        #seg_tokens
                    }
                    __interp
                }).as_str()
            }))
        }
        ConditionOperand::Path { path, filters } => {
            codegen_path_operand_typed(path, filters, scope)
        }
    }
}

fn codegen_path_operand_typed(
    path: &CompiledPath,
    filters: &[md_tmpl_core::compiled::ParsedFilter],
    scope: &NativeScope,
) -> Option<TypedOperand> {
    let cp = crate_path();
    let expr = CompiledExpr::Path(path.clone());
    if filters.is_empty() {
        return match scope.resolve_path(path)? {
            ResolvedValue::Static(Value::Str(s)) => Some(TypedOperand::Str(quote! { #s })),
            ResolvedValue::Static(Value::Int(i)) => Some(TypedOperand::Int(quote! { #i })),
            ResolvedValue::Static(Value::Float(f)) => {
                let bits = f.to_bits();
                Some(TypedOperand::Float(quote! {
                    ::core::primitive::f64::from_bits(#bits)
                }))
            }
            ResolvedValue::Static(Value::Bool(b)) => Some(TypedOperand::Bool(quote! { #b })),
            ResolvedValue::Static(Value::List(items)) => {
                let mut strs = Vec::with_capacity(items.len());
                let mut ints = Vec::with_capacity(items.len());
                for item in items.iter() {
                    match item {
                        Value::Str(s) => strs.push(s.clone()),
                        Value::Int(i) => ints.push(*i),
                        _ => return None,
                    }
                }
                if !strs.is_empty() && ints.is_empty() {
                    Some(TypedOperand::List {
                        expr: quote! { [#(#strs),*] },
                        elem_vt: VarType::Str,
                    })
                } else if !ints.is_empty() && strs.is_empty() {
                    Some(TypedOperand::List {
                        expr: quote! { [#(#ints),*] },
                        elem_vt: VarType::Int,
                    })
                } else if items.is_empty() {
                    Some(TypedOperand::List {
                        expr: quote! { [&str; 0]::default() },
                        elem_vt: VarType::Str,
                    })
                } else {
                    None
                }
            }
            ResolvedValue::Runtime(b) => {
                let e = b.expr;
                match b.var_type {
                    VarType::Str => Some(TypedOperand::Str(quote! {
                        ::core::convert::AsRef::<str>::as_ref(&(#e))
                    })),
                    VarType::Int => Some(TypedOperand::Int(e)),
                    VarType::Float => Some(TypedOperand::Float(e)),
                    VarType::Bool => Some(TypedOperand::Bool(e)),
                    VarType::Enum(ref variants) if variants.iter().all(|v| v.fields.is_empty()) => {
                        let k = codegen_kind_expr_str(path, scope)?;
                        Some(TypedOperand::Str(k))
                    }
                    VarType::List(ref fields) if fields.len() == 1 && fields[0].name.is_empty() => {
                        Some(TypedOperand::List {
                            expr: e,
                            elem_vt: fields[0].var_type.clone(),
                        })
                    }
                    _ => None,
                }
            }
            ResolvedValue::Static(_) => None,
        };
    }

    // Fold numeric `add`/`sub` filter chains directly into a typed numeric expression.
    if let Some(num_base) = try_extract_num_expr(&expr, scope)
        && let Some(folded) = fold_numeric_filters(num_base, filters)
    {
        return Some(match folded {
            NumExpr::Int(t) => TypedOperand::Int(t),
            NumExpr::Float(t) => TypedOperand::Float(t),
        });
    }

    // Any other filter chain produces a String.
    let write_stmts = codegen_expr(&expr, filters, scope)?;
    Some(TypedOperand::Str(quote! {
        ({
            let mut __op_str = #cp::__private::String::new();
            {
                let __out = &mut __op_str;
                #write_stmts
            }
            __op_str
        }).as_str()
    }))
}

fn codegen_comparison(
    left: &ConditionOperand,
    op: ComparisonOp,
    right: &ConditionOperand,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    if op == ComparisonOp::In {
        return codegen_in_comparison(left, right, scope, counter);
    }

    let lhs = codegen_operand_typed(left, scope, counter)?;
    let rhs = codegen_operand_typed(right, scope, counter)?;
    let cp = crate_path();

    match (lhs, rhs) {
        (TypedOperand::Int(l), TypedOperand::Int(r))
        | (TypedOperand::Str(l), TypedOperand::Str(r)) => Some(match op {
            ComparisonOp::Eq => quote! { ((#l) == (#r)) },
            ComparisonOp::Ne => quote! { ((#l) != (#r)) },
            ComparisonOp::Lt => quote! { ((#l) < (#r)) },
            ComparisonOp::Le => quote! { ((#l) <= (#r)) },
            ComparisonOp::Gt => quote! { ((#l) > (#r)) },
            ComparisonOp::Ge => quote! { ((#l) >= (#r)) },
            ComparisonOp::In => unreachable!(),
        }),
        (TypedOperand::Float(l), TypedOperand::Float(r)) => {
            let cmp = quote! { (#l).partial_cmp(&(#r)) };
            Some(codegen_ordering_check(&cmp, op))
        }
        (TypedOperand::Int(l), TypedOperand::Float(r)) => {
            let cmp = quote! { #cp::__private::cmp_int_float(#l, #r) };
            Some(codegen_ordering_check(&cmp, op))
        }
        (TypedOperand::Float(l), TypedOperand::Int(r)) => {
            let cmp = quote! {
                #cp::__private::cmp_int_float(#r, #l).map(::core::cmp::Ordering::reverse)
            };
            Some(codegen_ordering_check(&cmp, op))
        }
        (TypedOperand::Bool(l), TypedOperand::Bool(r)) => match op {
            ComparisonOp::Eq => Some(quote! { ((#l) == (#r)) }),
            ComparisonOp::Ne => Some(quote! { ((#l) != (#r)) }),
            _ => None,
        },
        _ => None,
    }
}

fn codegen_ordering_check(cmp_opt: &TokenStream, op: ComparisonOp) -> TokenStream {
    match op {
        ComparisonOp::Eq => quote! { (#cmp_opt).is_some_and(::core::cmp::Ordering::is_eq) },
        ComparisonOp::Ne => quote! { (#cmp_opt).is_none_or(::core::cmp::Ordering::is_ne) },
        ComparisonOp::Lt => quote! { (#cmp_opt).is_some_and(::core::cmp::Ordering::is_lt) },
        ComparisonOp::Le => quote! { (#cmp_opt).is_some_and(::core::cmp::Ordering::is_le) },
        ComparisonOp::Gt => quote! { (#cmp_opt).is_some_and(::core::cmp::Ordering::is_gt) },
        ComparisonOp::Ge => quote! { (#cmp_opt).is_some_and(::core::cmp::Ordering::is_ge) },
        ComparisonOp::In => unreachable!(),
    }
}

fn codegen_in_comparison(
    left: &ConditionOperand,
    right: &ConditionOperand,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let lhs = codegen_operand_typed(left, scope, counter)?;
    let rhs = codegen_operand_typed(right, scope, counter)?;

    match (lhs, rhs) {
        // Substring check: `"sub" in str_val`
        (TypedOperand::Str(l), TypedOperand::Str(r)) => Some(quote! {
            (#r).contains(#l)
        }),
        // String membership in `list(str)`
        (
            TypedOperand::Str(l),
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Str,
            },
        ) => Some(quote! {
            match #l {
                __needle => (#r_list).iter().any(|__item| ::core::convert::AsRef::<str>::as_ref(__item) == __needle),
            }
        }),
        // Int membership in `list(int)`
        (
            TypedOperand::Int(l),
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Int,
            },
        ) => Some(quote! {
            match #l {
                __needle => (#r_list).iter().any(|__item| *__item == __needle),
            }
        }),
        // Subset check: `list(str) in list(str)`
        (
            TypedOperand::List {
                expr: l_list,
                elem_vt: VarType::Str,
            },
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Str,
            },
        ) => Some(quote! {
            (#l_list).iter().all(|__l_item| {
                let __l_s: &str = ::core::convert::AsRef::<str>::as_ref(__l_item);
                (#r_list).iter().any(|__r_item| ::core::convert::AsRef::<str>::as_ref(__r_item) == __l_s)
            })
        }),
        (
            TypedOperand::List {
                expr: l_list,
                elem_vt: VarType::Int,
            },
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Int,
            },
        ) => Some(quote! {
            (#l_list).iter().all(|__l_item| (#r_list).iter().any(|__r_item| *__r_item == *__l_item))
        }),
        _ => None,
    }
}

fn codegen_match_variant_cond(
    expr: &CompiledPath,
    variants: &[std::borrow::Cow<'static, str>],
    is_option: bool,
    scope: &NativeScope,
) -> Option<TokenStream> {
    let ResolvedValue::Runtime(b) = scope.resolve_path(expr)? else {
        return None;
    };
    if variants
        .iter()
        .any(|v| v.as_ref() == md_tmpl_core::consts::MATCH_DEFAULT)
    {
        return Some(quote! { true });
    }
    let e = &b.expr;
    if is_option || matches!(b.var_type, VarType::Option(_)) {
        let has_some = variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::OPTION_SOME);
        let has_none = variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::OPTION_NONE);
        return match (has_some, has_none) {
            (true, true) => Some(quote! { true }),
            (true, false) => Some(quote! { (#e).is_some() }),
            (false, true) => Some(quote! { (#e).is_none() }),
            (false, false) => Some(quote! { false }),
        };
    }
    let VarType::Enum(declared) = &b.var_type else {
        return None;
    };
    let enum_ident = format_ident!("{}", b.type_prefix);
    let decl_names: Vec<String> = declared.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&decl_names);
    let mut pat_tokens = Vec::new();
    for v_name in variants {
        let target =
            md_tmpl_core::consts::strip_string_literal(v_name.as_ref()).unwrap_or(v_name.as_ref());
        let (decl, (v_ident, _)) = declared
            .iter()
            .zip(deduped.iter())
            .find(|(d, _)| d.name == target)?;
        if decl.fields.is_empty() {
            pat_tokens.push(quote! { #enum_ident::#v_ident });
        } else {
            pat_tokens.push(quote! { #enum_ident::#v_ident { .. } });
        }
    }
    if pat_tokens.is_empty() {
        Some(quote! { false })
    } else {
        Some(quote! { matches!(&(#e), #(#pat_tokens)|*) })
    }
}
