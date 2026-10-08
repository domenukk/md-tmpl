//! Type-check tests for flow-sensitive option narrowing.

use super::{test_helpers::*, *};
use crate::types::{VarDecl, VarType};

// -- Option narrowing and inverse tests ------------------------------

#[test]
fn option_field_access_inside_has_guard_ok() {
    let decls = vec![VarDecl {
        name: "opt".to_string(),
        var_type: VarType::Option(Box::new(VarType::Struct(vec![str_decl("title")]))),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - opt = option(struct(title = str))
---
> {% if has(opt) %}{{ opt.title }}{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "field access inside has() guard should pass: {errors:?}"
    );
}

#[test]
fn direct_option_rendering_without_guard_is_error() {
    let decls = vec![VarDecl {
        name: "opt".to_string(),
        var_type: VarType::Option(Box::new(VarType::Str)),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - opt = option(str)
---
{{ opt }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot display value of type option"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn option_field_access_without_has_guard_is_error() {
    let decls = vec![VarDecl {
        name: "opt".to_string(),
        var_type: VarType::Option(Box::new(VarType::Struct(vec![str_decl("title")]))),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - opt = option(struct(title = str))
---
{{ opt.title }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot access field") && errors[0].contains("option"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn option_field_access_in_else_arm_is_error() {
    let decls = vec![VarDecl {
        name: "opt".to_string(),
        var_type: VarType::Option(Box::new(VarType::Struct(vec![str_decl("title")]))),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - opt = option(struct(title = str))
---
> {% if has(opt) %}present{% else %}{{ opt.title }}{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error in else arm: {errors:?}");
    assert!(
        errors[0].contains("cannot access field") && errors[0].contains("option"),
        "got: {}",
        errors[0]
    );
}

// -- Flow-sensitive option narrowing: field access in all branch positions --
//
// Lock in the contract that an option's inner value (here, its struct fields)
// is accessible ONLY in the branch where the option is proven present, and
// produces a compile-time "cannot access field" error anywhere it is absent.
//
// Note: bare display of a *scalar* option (e.g. `{{ o }}` where `o = option(str)`)
// is intentionally allowed — it renders "None" when absent — so field access on
// an `option(struct)` is used to observe the narrowing boundary.

/// Build a single `option(struct(title = str))` param named `o`.
fn option_struct_decls() -> Vec<VarDecl> {
    vec![VarDecl {
        name: "o".to_string(),
        var_type: VarType::Option(Box::new(VarType::Struct(vec![str_decl("title")]))),
        default_value: None,
    }]
}

const OPTION_STRUCT_HEADER: &str = "---
name: t
params:
  - o = option(struct(title = str))
---
";

fn assert_field_option_error(errors: &[String]) {
    assert_eq!(errors.len(), 1, "expected 1 error, got: {errors:?}");
    assert!(
        errors[0].contains("cannot access field") && errors[0].contains("option"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn narrow_if_true_field_usable() {
    // Position 1 (if-true): `o` is present → inner field accessible.
    let tmpl = format!("{OPTION_STRUCT_HEADER}> {{% if has(o) %}}{{{{ o.title }}}}{{% /if %}}");
    let errors = compile_and_check(&tmpl, &option_struct_decls());
    assert!(
        errors.is_empty(),
        "has() branch should allow field: {errors:?}"
    );
}

#[test]
fn narrow_if_else_field_is_error() {
    // Position 2 (if-else): `o` is absent in the else → compile error.
    let tmpl = format!(
        "{OPTION_STRUCT_HEADER}> {{% if has(o) %}}x{{% else %}}{{{{ o.title }}}}{{% /if %}}"
    );
    let errors = compile_and_check(&tmpl, &option_struct_decls());
    assert_field_option_error(&errors);
}

#[test]
fn narrow_negated_true_field_is_error() {
    // Position 3 (negated-true): inside `!has(o)` the option is absent → error.
    let tmpl = format!("{OPTION_STRUCT_HEADER}> {{% if !has(o) %}}{{{{ o.title }}}}{{% /if %}}");
    let errors = compile_and_check(&tmpl, &option_struct_decls());
    assert_field_option_error(&errors);
}

#[test]
fn narrow_negated_else_field_usable() {
    // Position 4 (negated-else): once `!has(o)` is skipped, `o` is present in
    // the else → inner field accessible.
    let tmpl = format!(
        "{OPTION_STRUCT_HEADER}> {{% if !has(o) %}}x{{% else %}}{{{{ o.title }}}}{{% /if %}}"
    );
    let errors = compile_and_check(&tmpl, &option_struct_decls());
    assert!(
        errors.is_empty(),
        "negated-has else branch should allow field: {errors:?}"
    );
}

#[test]
fn narrow_negated_else_carries_to_later_branch() {
    // A `!has(o)` guard also proves presence in a subsequent `elif` branch.
    let tmpl = format!(
        "{OPTION_STRUCT_HEADER}> {{% if !has(o) %}}x{{% elif true %}}{{{{ o.title }}}}{{% /if %}}"
    );
    let errors = compile_and_check(&tmpl, &option_struct_decls());
    assert!(
        errors.is_empty(),
        "negated-has should carry presence to elif: {errors:?}"
    );
}

#[test]
fn narrow_match_some_arm_field_usable() {
    // Match Some arm: `o` present → inner field accessible.
    let tmpl = format!(
        "{OPTION_STRUCT_HEADER}> {{% match o %}}{{% case Some %}}{{{{ o.title }}}}{{% case None %}}x{{% /match %}}"
    );
    let errors = compile_and_check(&tmpl, &option_struct_decls());
    assert!(errors.is_empty(), "Some arm should allow field: {errors:?}");
}

#[test]
fn narrow_match_none_arm_field_is_error() {
    // Match None arm: `o` absent → compile error.
    let tmpl = format!(
        "{OPTION_STRUCT_HEADER}> {{% match o %}}{{% case Some %}}x{{% case None %}}{{{{ o.title }}}}{{% /match %}}"
    );
    let errors = compile_and_check(&tmpl, &option_struct_decls());
    assert_field_option_error(&errors);
}
