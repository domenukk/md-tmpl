//! Type-check tests for `match` narrowing, variants, and exhaustiveness.

use super::{test_helpers::*, *};
use crate::{
    compiled::Segment,
    scope::CompiledPath,
    types::{VarDecl, VarType},
};

// -- Variant name validation -----------------------------------------

#[test]
fn match_valid_variant_names() {
    let decls = vec![enum_decl(
        "outcome",
        vec![unit_variant("Confirmed"), unit_variant("NotConfirmed")],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed, NotConfirmed)
---
> {% match outcome %}{% case Confirmed %}yes{% case NotConfirmed %}no{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}

#[test]
fn match_unknown_variant_name() {
    let decls = vec![enum_decl(
        "outcome",
        vec![unit_variant("Confirmed"), unit_variant("NotConfirmed")],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed, NotConfirmed)
---
> {% match outcome %}{% case Confrimed %}yes{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected 1 error, got: {errors:?}");
    assert!(
        errors[0].contains("unknown variant 'Confrimed'"),
        "got: {}",
        errors[0]
    );
    assert!(
        errors[0].contains("Confirmed, NotConfirmed"),
        "should list valid variants: {}",
        errors[0]
    );
}

// -- Field access outside match (must be on ALL variants) ------------

#[test]
fn field_on_all_variants_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("reason", VarType::Str)]),
            variant("Rejected", vec![("reason", VarType::Str)]),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(reason = str), Rejected(reason = str))
---
{{ outcome.reason }}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "unexpected: {errors:?}");
}

#[test]
fn field_not_on_all_variants_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(evidence = str), NotConfirmed)
---
{{ outcome.evidence }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected 1 error, got: {errors:?}");
    assert!(
        errors[0].contains("not available on variant") && errors[0].contains("NotConfirmed"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn tag_field_is_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(evidence = str), NotConfirmed)
---
{{ outcome.tag }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, ".tag should be an error: {errors:?}");
    assert!(
        errors[0].contains("tag") && errors[0].contains("does not exist"),
        "got: {}",
        errors[0]
    );
}

// -- Field access inside match arm (narrowed) ------------------------

#[test]
fn field_in_matching_arm_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(evidence = str), NotConfirmed)
---
> {% match outcome %}{% case Confirmed %}{{ outcome.evidence }}{% case NotConfirmed %}none{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "narrowed access should work: {errors:?}");
}

#[test]
fn field_in_wrong_arm_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(evidence = str), NotConfirmed)
---
> {% match outcome %}{% case NotConfirmed %}{{ outcome.evidence }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("evidence") && errors[0].contains("NotConfirmed"),
        "got: {}",
        errors[0]
    );
}

// -- Multi-variant case: intersection of fields ----------------------

#[test]
fn multi_variant_shared_field_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant(
                "Confirmed",
                vec![("reason", VarType::Str), ("evidence", VarType::Str)],
            ),
            variant("ConfirmedWithCaveats", vec![("reason", VarType::Str)]),
            unit_variant("Rejected"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(reason = str, evidence = str), ConfirmedWithCaveats(reason = str), Rejected)
---
> {% match outcome %}{% case Confirmed | ConfirmedWithCaveats %}{{ outcome.reason }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "shared field 'reason' should work: {errors:?}"
    );
}

#[test]
fn multi_variant_non_shared_field_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant(
                "Confirmed",
                vec![("reason", VarType::Str), ("evidence", VarType::Str)],
            ),
            variant("ConfirmedWithCaveats", vec![("reason", VarType::Str)]),
            unit_variant("Rejected"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(reason = str, evidence = str), ConfirmedWithCaveats(reason = str), Rejected)
---
> {% match outcome %}{% case Confirmed | ConfirmedWithCaveats %}{{ outcome.evidence }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("evidence") && errors[0].contains("ConfirmedWithCaveats"),
        "got: {}",
        errors[0]
    );
}

// -- Inline match case -----------------------------------------------

#[test]
fn inline_match_case_field_ok() {
    let decls = vec![enum_decl(
        "vt",
        vec![
            variant("Known", vec![("label", VarType::Str)]),
            unit_variant("Unknown"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - vt = enum(Known(label = str), Unknown)
---
> {% match vt case Known %}{{ vt.label }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "inline narrowing: {errors:?}");
}

// -- Nested match ----------------------------------------------------

#[test]
fn nested_match_narrows_independently() {
    let decls = vec![
        enum_decl(
            "a",
            vec![
                variant("X", vec![("x_val", VarType::Str)]),
                unit_variant("Y"),
            ],
        ),
        enum_decl(
            "b",
            vec![
                variant("P", vec![("p_val", VarType::Str)]),
                unit_variant("Q"),
            ],
        ),
    ];
    let tmpl = r"---
name: t
params:
  - a = enum(X(x_val = str), Y)
  - b = enum(P(p_val = str), Q)
---
> {% match a %}{% case X %}> {% match b %}{% case P %}{{ a.x_val }} {{ b.p_val }}{% /match %}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "nested narrowing: {errors:?}");
}

// -- Match on non-enum rejects at compile time ----------------------

#[test]
fn match_on_str_compiles_ok() {
    let decls = vec![VarDecl {
        name: "status".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl = r#"---
name: t
params:
  - status = str
---
> {% match status %}{% case "Active" %}active{% /match %}"#;
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "expected no errors: {errors:?}");
}

#[test]
fn match_on_int_compiles_ok() {
    let decls = vec![VarDecl {
        name: "count".to_string(),
        var_type: VarType::Int,
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - count = int
---
> {% match count %}{% case 0 %}zero{% case 1 %}one{% else %}many{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "match on int should compile: {errors:?}");
}

#[test]
fn match_on_bool_compiles_ok() {
    let decls = vec![VarDecl {
        name: "flag".to_string(),
        var_type: VarType::Bool,
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - flag = bool
---
> {% match flag %}{% case true %}yes{% case false %}no{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "match on bool should compile: {errors:?}"
    );
}

// -- Condition paths inside if inside match --------------------------

#[test]
fn condition_inside_match_arm_validated() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = r#"---
name: t
params:
  - outcome = enum(Confirmed(evidence = str), NotConfirmed)
---
> {% match outcome %}{% case Confirmed %}{% if outcome.evidence == "secret" %}yes{% /if %}{% /match %}"#;
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "condition in arm: {errors:?}");
}

// -- Enum comparison rejection ----------------------------------------

#[test]
fn eq_on_unit_enum_is_compile_error() {
    let decls = vec![enum_decl(
        "role",
        vec![
            unit_variant("Builder"),
            unit_variant("Analyst"),
            unit_variant("Chainer"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - role = enum(Builder, Analyst, Chainer)
---
> {% if role == Builder %}yes{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot compare enum") && errors[0].contains("match"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn eq_on_struct_enum_is_compile_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("Rejected"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
> {% if outcome == Confirmed %}yes{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot compare enum"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn ne_on_enum_is_compile_error() {
    let decls = vec![enum_decl(
        "status",
        vec![unit_variant("Active"), unit_variant("Inactive")],
    )];
    let tmpl = r"---
name: t
params:
  - status = enum(Active, Inactive)
---
> {% if status != Active %}no{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot compare enum"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn eq_enum_string_literal_is_compile_error() {
    // Even comparing an enum to a string literal should be rejected.
    let decls = vec![enum_decl(
        "role",
        vec![unit_variant("Builder"), unit_variant("Analyst")],
    )];
    let tmpl = r#"---
name: t
params:
  - role = enum(Builder, Analyst)
---
> {% if role == "Builder" %}yes{% /if %}"#;
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot compare enum") && errors[0].contains("match"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn elif_on_enum_is_compile_error() {
    // Full if/elif chain on an enum should be rejected.
    let decls = vec![enum_decl(
        "role",
        vec![
            unit_variant("Builder"),
            unit_variant("Analyst"),
            unit_variant("Support"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - role = enum(Builder, Analyst, Support)
---
> {% if role == Builder %}b{% elif role == Analyst %}a{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 2, "expected 2 errors (if + elif): {errors:?}");
    assert!(
        errors.iter().all(|e| e.contains("cannot compare enum")),
        "got: {errors:?}",
    );
}

// -- Field doesn't exist on any variant ------------------------------

#[test]
fn field_nonexistent_on_all_variants() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("A", vec![("x", VarType::Str)]),
            variant("B", vec![("y", VarType::Str)]),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(A(x = str), B(y = str))
---
> {% match outcome %}{% case A %}{{ outcome.z }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].contains("'z'") && errors[0].contains("does not exist"),
        "got: {}",
        errors[0]
    );
}

// -- Exhaustiveness --------------------------------------------------

#[test]
fn multi_arm_match_exhaustive_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(evidence = str), NotConfirmed)
---
> {% match outcome %}{% case Confirmed %}yes{% case NotConfirmed %}no{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "exhaustive match: {errors:?}");
}

#[test]
fn multi_arm_match_non_exhaustive_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("A", vec![("x", VarType::Str)]),
            unit_variant("B"),
            unit_variant("C"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(A(x = str), B, C)
---
> {% match outcome %}{% case A %}a{% case B %}b{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected exhaustiveness error: {errors:?}");
    assert!(
        errors[0].contains("non-exhaustive") && errors[0].contains('C'),
        "got: {}",
        errors[0]
    );
}

#[test]
fn single_arm_inline_not_exhaustive_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![unit_variant("A"), unit_variant("B")],
    )];
    // Single inline arm — intentionally non-exhaustive guard.
    let tmpl = r"---
name: t
params:
  - outcome = enum(A, B)
---
> {% match outcome case A %}a{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "inline guard: {errors:?}");
}

#[test]
fn match_with_no_arms_is_error() {
    // Construct directly since the parser might not produce this.
    let decls = vec![enum_decl("x", vec![unit_variant("A"), unit_variant("B")])];
    let segments = vec![Segment::Match {
        expr: CompiledPath::compile("x"),
        arms: vec![],
        is_option: false,
    }];
    let errors = validate_field_accesses(&segments, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(errors[0].contains("no case arms"), "got: {}", errors[0]);
}

// -- Else arm (catch-all) -----------------------------------------

#[test]
fn else_arm_satisfies_exhaustiveness() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("Rejected"),
            unit_variant("Pending"),
        ],
    )];
    let tmpl = r"---
name: t
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, Pending)
---
> {% match outcome %}{% case Confirmed %}yes{% else %}other{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "else should satisfy exhaustiveness: {errors:?}"
    );
}

#[test]
fn else_alone_satisfies_exhaustiveness() {
    let decls = vec![enum_decl(
        "status",
        vec![unit_variant("A"), unit_variant("B"), unit_variant("C")],
    )];
    let tmpl = r"---
name: t
params:
  - status = enum(A, B, C)
---
> {% match status %}{% else %}fallback{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "else-only should be valid: {errors:?}");
}

#[test]
fn else_arm_renders_correctly() {
    let tmpl = crate::Template::from_source(
        r"---
params:
  - status = enum(A, B, C)
---
> {% match status %}{% case A %}alpha{% else %}other{% /match %}",
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set("status", "B");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "other");
    ctx.set("status", "A");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "alpha");
}
