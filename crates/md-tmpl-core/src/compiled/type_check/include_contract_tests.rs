//! Type-check tests for include contracts, cycles, and type compatibility.

use super::{test_helpers::*, *};
use crate::{
    compiled::{CompiledInclude, CompiledInlineTemplate, Segment},
    scope::CompiledExpr,
    types::{VarDecl, VarType},
};

// -- Include contract tests ------------------------------------------

#[test]
fn include_missing_required_params_error() {
    let inc = make_include(
        "child.tmpl.md",
        vec![],
        None,
        vec![str_decl("msg"), int_decl("count")],
        vec![],
    );
    let parent_decls: Vec<VarDecl> = vec![];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.iter().any(|e| e.contains("missing required param")),
        "expected missing params error: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("msg") && e.contains("count")),
        "should mention both missing params: {errors:?}"
    );
}

#[test]
fn include_all_params_provided_ok() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("msg", "my_msg"), ("count", "my_count")],
        None,
        vec![str_decl("msg"), int_decl("count")],
        vec![],
    );
    let parent_decls = vec![str_decl("my_msg"), int_decl("my_count")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "all params provided should be OK: {errors:?}"
    );
}

#[test]
fn include_for_binding_counts_as_provided() {
    let inc = make_include(
        "row.tmpl.md",
        vec![],
        Some(("item", "items")),
        vec![str_decl("item")],
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "items".to_string(),
        var_type: VarType::List(vec![]),
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "for binding should satisfy param: {errors:?}"
    );
}

#[test]
fn include_no_declarations_always_ok() {
    let inc = make_include("static.tmpl.md", vec![], None, vec![], vec![]);
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.is_empty(),
        "no declarations should always be OK: {errors:?}"
    );
}

// -- Include type-check tests ----------------------------------------

#[test]
fn include_type_match_str_ok() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("name", "parent_name")],
        None,
        vec![str_decl("name")],
        vec![],
    );
    let parent_decls = vec![str_decl("parent_name")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(errors.is_empty(), "matching types should be OK: {errors:?}");
}

#[test]
fn include_type_mismatch_error() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("count", "name")],
        None,
        vec![int_decl("count")],
        vec![],
    );
    let parent_decls = vec![str_decl("name")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.iter().any(|e| e.contains("type mismatch")),
        "should report type mismatch: {errors:?}"
    );
}

#[test]
fn include_dotted_path_type_resolution() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("label", "config.host")],
        None,
        vec![str_decl("label")],
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "config".to_string(),
        var_type: VarType::Struct(vec![str_decl("host")]),
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "dotted path type should resolve: {errors:?}"
    );
}

#[test]
fn include_dotted_path_type_mismatch() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("count", "config.host")],
        None,
        vec![int_decl("count")], // expects int
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "config".to_string(),
        var_type: VarType::Struct(vec![str_decl("host")]), // host is str
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.iter().any(|e| e.contains("type mismatch")),
        "dotted path type mismatch: {errors:?}"
    );
}

#[test]
fn include_literal_value_skips_type_check() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("msg", "\"hello\"")],
        None,
        vec![str_decl("msg")],
        vec![],
    );
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.is_empty(),
        "literals should skip type check: {errors:?}"
    );
}

#[test]
fn include_enum_type_forwarding() {
    let enum_variants = vec![
        variant("Active", vec![("label", VarType::Str)]),
        unit_variant("Stopped"),
    ];
    let enum_type = VarType::Enum(enum_variants.clone());
    let inc = make_include(
        "child.tmpl.md",
        vec![("status", "status")],
        None,
        vec![VarDecl {
            name: "status".to_string(),
            var_type: enum_type.clone(),
            default_value: None,
        }],
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "status".to_string(),
        var_type: enum_type,
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "enum forwarding should be OK: {errors:?}"
    );
}

// -- Include body validation tests ------------------------------------

#[test]
fn include_body_undeclared_var_error() {
    let inc = make_include(
        "child.tmpl.md",
        vec![],
        None,
        vec![], // no declarations in child
        vec![Segment::Expr {
            expr: CompiledExpr::compile("ghost").unwrap(),
            filters: vec![],
        }],
    );
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.iter().any(|e| e.contains("undeclared")),
        "should catch undeclared var in included body: {errors:?}"
    );
}

#[test]
fn include_body_field_on_scalar_error() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("name", "parent_name")],
        None,
        vec![str_decl("name")],
        vec![Segment::Expr {
            expr: CompiledExpr::compile("name.length").unwrap(),
            filters: vec![],
        }],
    );
    let parent_decls = vec![str_decl("parent_name")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot access field") && e.contains("str")),
        "should catch field access on scalar in included body: {errors:?}"
    );
}

#[test]
fn include_body_valid_references_ok() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("msg", "parent_msg")],
        None,
        vec![str_decl("msg")],
        vec![Segment::Expr {
            expr: CompiledExpr::compile("msg").unwrap(),
            filters: vec![],
        }],
    );
    let parent_decls = vec![str_decl("parent_msg")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "valid references in body should be OK: {errors:?}"
    );
}

// -- Cycle detection tests -------------------------------------------

#[test]
fn self_recursive_include_no_infinite_loop() {
    // A template that includes itself — should not infinite loop.
    // Boundary is checked, body is walked once.
    let inc = make_include(
        "self.tmpl.md",
        vec![],
        None,
        vec![],
        // Body includes itself again (with the same path).
        vec![Segment::Include(CompiledInclude {
            path: "self.tmpl.md".into(),
            with_vars: vec![],
            for_each: None,
            inline_compiled: Some(CompiledInlineTemplate {
                segments: std::sync::Arc::from(vec![Segment::Static("leaf".into())]),
                declarations: std::sync::Arc::from(vec![]),
                consts: std::sync::Arc::new(crate::compat::HashMap::new()),
                imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            }),
        })],
    );
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    // Should complete without hanging. No errors expected (no params).
    assert!(
        errors.is_empty(),
        "self-recursive with no params should be OK: {errors:?}"
    );
}

#[test]
fn flip_flop_includes_no_infinite_loop() {
    // A→B→A cycle — should not infinite loop.
    let inc_b = CompiledInclude {
        path: "a.tmpl.md".into(),
        with_vars: vec![],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: std::sync::Arc::from(vec![Segment::Static("leaf".into())]),
            declarations: std::sync::Arc::from(vec![]),
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };
    let inc_a = make_include(
        "b.tmpl.md",
        vec![],
        None,
        vec![],
        vec![Segment::Include(inc_b)],
    );
    let segments = vec![Segment::Include(inc_a)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.is_empty(),
        "flip-flop cycle with no params should be OK: {errors:?}"
    );
}

#[test]
fn self_recursive_include_with_type_mismatch_error() {
    // Self-recursive include with wrong type at boundary.
    let inc = make_include(
        "self.tmpl.md",
        vec![("name", "name")],
        None,
        vec![int_decl("name")], // child expects int
        vec![Segment::Include(CompiledInclude {
            path: "self.tmpl.md".into(),
            with_vars: vec![(
                std::borrow::Cow::Borrowed("name"),
                std::borrow::Cow::Borrowed("name"),
            )],
            for_each: None,
            inline_compiled: Some(CompiledInlineTemplate {
                segments: std::sync::Arc::from(vec![]),
                declarations: std::sync::Arc::from(vec![int_decl("name")]),
                consts: std::sync::Arc::new(crate::compat::HashMap::new()),
                imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            }),
        })],
    );
    let parent_decls = vec![str_decl("name")]; // parent has str
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.iter().any(|e| e.contains("type mismatch")),
        "should catch type mismatch at recursive boundary: {errors:?}"
    );
}

#[test]
fn self_recursive_include_contract_missing_params_error() {
    // Self-recursive include missing required params at boundary.
    let inc = make_include(
        "self.tmpl.md",
        vec![], // no with vars at outer call
        None,
        vec![str_decl("msg")], // child requires msg
        vec![Segment::Include(CompiledInclude {
            path: "self.tmpl.md".into(),
            with_vars: vec![],
            for_each: None,
            inline_compiled: Some(CompiledInlineTemplate {
                segments: std::sync::Arc::from(vec![]),
                declarations: std::sync::Arc::from(vec![str_decl("msg")]),
                consts: std::sync::Arc::new(crate::compat::HashMap::new()),
                imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            }),
        })],
    );
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.iter().any(|e| e.contains("missing required param")),
        "should catch missing params at recursive boundary: {errors:?}"
    );
}

// -- types_compatible tests ------------------------------------------

#[test]
fn types_compatible_scalars() {
    assert!(types_compatible(&VarType::Str, &VarType::Str));
    assert!(types_compatible(&VarType::Int, &VarType::Int));
    assert!(types_compatible(&VarType::Float, &VarType::Float));
    assert!(types_compatible(&VarType::Bool, &VarType::Bool));
    assert!(!types_compatible(&VarType::Str, &VarType::Int));
    assert!(!types_compatible(&VarType::Int, &VarType::Bool));
}

#[test]
fn types_compatible_untyped_containers() {
    // Untyped list is compatible with typed list.
    assert!(types_compatible(
        &VarType::List(vec![]),
        &VarType::List(vec![str_decl("x")])
    ));
    assert!(types_compatible(
        &VarType::List(vec![str_decl("x")]),
        &VarType::List(vec![])
    ));
    // Same typed lists are compatible.
    assert!(types_compatible(
        &VarType::List(vec![str_decl("x")]),
        &VarType::List(vec![str_decl("x")])
    ));
    // Different typed lists are not.
    assert!(!types_compatible(
        &VarType::List(vec![str_decl("x")]),
        &VarType::List(vec![int_decl("x")])
    ));
}

#[test]
fn types_compatible_cross_kind() {
    assert!(!types_compatible(&VarType::Str, &VarType::List(vec![])));
    assert!(!types_compatible(&VarType::Struct(vec![]), &VarType::Int));
}

#[test]
fn types_compatible_option_types() {
    // Same inner type: compatible.
    assert!(types_compatible(
        &VarType::Option(Box::new(VarType::Str)),
        &VarType::Option(Box::new(VarType::Str)),
    ));
    assert!(types_compatible(
        &VarType::Option(Box::new(VarType::Int)),
        &VarType::Option(Box::new(VarType::Int)),
    ));
    // Different inner types: not compatible.
    assert!(!types_compatible(
        &VarType::Option(Box::new(VarType::Str)),
        &VarType::Option(Box::new(VarType::Int)),
    ));
    // Option vs non-option: not compatible.
    assert!(!types_compatible(
        &VarType::Option(Box::new(VarType::Str)),
        &VarType::Str,
    ));
    assert!(!types_compatible(
        &VarType::Str,
        &VarType::Option(Box::new(VarType::Str)),
    ));
}
