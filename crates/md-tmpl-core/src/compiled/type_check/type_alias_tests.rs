//! Type-check tests for type aliases, opaque roots, and displayability.

use super::{test_helpers::*, *};
use crate::types::{VarDecl, VarType};

// -- Complex type aliases in types: block ----------------------------------

/// Like `compile_and_check`, but uses the frontmatter's own declarations
/// (including type-alias-resolved params) instead of externally passed decls.
fn compile_and_check_self(template: &str) -> Vec<String> {
    let (fm, body) = crate::parse_frontmatter(template).expect("parse");
    let (segments, _) = crate::compiled::compile(body, &fm.type_aliases).expect("compile");
    validate_field_accesses(&segments, &fm.declarations)
}

#[test]
fn list_type_alias_in_types_block() {
    let tmpl = r"---
name: t
types:
  - TaskList = list(title = str, score = int)

params:
  - tasks = TaskList
---
> {% for b in tasks %}{{ b.title }}: {{ b.score }}
> {% /for %}";
    let errors = compile_and_check_self(tmpl);
    assert!(errors.is_empty(), "list type alias should work: {errors:?}");
}

#[test]
fn chained_type_alias_enum_in_list() {
    let tmpl = r"---
name: t
types:
  - Severity = enum(High, Medium, Low)
  - TaskReport = list(title = str, severity = Severity)

params:
  - tasks = TaskReport
---
> {% for b in tasks %}{{ b.title }} {% match b.severity %}{% case High %}🔴{% case Medium %}🟡{% case Low %}🟢{% /match %}
> {% /for %}";
    let errors = compile_and_check_self(tmpl);
    assert!(
        errors.is_empty(),
        "chained type alias should work: {errors:?}"
    );
}

// -- Opaque roots (import stems / consts) --------------------------------

fn compile_and_check_with_opaque(
    template: &str,
    decls: &[VarDecl],
    opaque: &[&str],
) -> Vec<String> {
    let (_fm, body) = crate::parse_frontmatter(template).expect("parse");
    let empty_aliases = crate::compat::HashMap::new();
    let (segments, _) = crate::compiled::compile(body, &empty_aliases).expect("compile");
    let opaque_set: HashSet<String> = opaque.iter().copied().map(String::from).collect();
    validate_field_accesses_with_opaque(&segments, decls, &opaque_set)
}

#[test]
fn opaque_root_skips_undeclared_error() {
    // `imported.NOTEBOOK_FILENAME` should not error when `imported` is opaque.
    let errors = compile_and_check_with_opaque(
        r"---
params: []
---
{{ imported.NOTEBOOK_FILENAME }}",
        &[],
        &["imported"],
    );
    assert!(
        errors.is_empty(),
        "opaque root should skip validation: {errors:?}"
    );
}

#[test]
fn opaque_root_dotted_path_deep() {
    // Deep dotted path on opaque root should also be skipped.
    let errors = compile_and_check_with_opaque(
        r"---
params: []
---
{{ config.STAGES.DESIGN }}",
        &[],
        &["config"],
    );
    assert!(
        errors.is_empty(),
        "deep opaque path should skip: {errors:?}"
    );
}

#[test]
fn opaque_root_bare_reference() {
    // Bare reference to opaque root (no dotted path) should also be valid.
    let errors = compile_and_check_with_opaque(
        r"---
params: []
---
{{ MAX_RETRIES }}",
        &[],
        &["MAX_RETRIES"],
    );
    assert!(
        errors.is_empty(),
        "bare opaque reference should work: {errors:?}"
    );
}

#[test]
fn non_opaque_unknown_variable_still_errors() {
    // Unknown variable that is NOT opaque should still error.
    let errors = compile_and_check_with_opaque(
        r"---
params: []
---
{{ unknown_var }}",
        &[],
        &["imported"],
    );
    assert_eq!(errors.len(), 1, "non-opaque unknown should error");
    assert!(errors[0].contains("undeclared variable"));
}

#[test]
fn opaque_root_in_conditional() {
    // Opaque roots used in if conditions should not error.
    let errors = compile_and_check_with_opaque(
        r"---
params: []
---
> {% if imported.ENABLED %}yes> {% /if %}",
        &[],
        &["imported"],
    );
    assert!(errors.is_empty(), "opaque root in conditional: {errors:?}");
}

#[test]
fn opaque_root_coexists_with_typed_params() {
    // Opaque roots and typed params should coexist.
    let decls = vec![VarDecl {
        name: "name".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let errors = compile_and_check_with_opaque(
        r"---
params:
  - name = str
---
{{ name }} {{ imported.CONST }}",
        &decls,
        &["imported"],
    );
    assert!(
        errors.is_empty(),
        "mixed opaque and typed should work: {errors:?}"
    );
}

#[test]
fn multiple_opaque_roots() {
    // Multiple opaque roots should all be recognized.
    let errors = compile_and_check_with_opaque(
        r"---
params: []
---
{{ imported.X }} {{ config.Y }} {{ MAX }}",
        &[],
        &["imported", "config", "MAX"],
    );
    assert!(errors.is_empty(), "multiple opaque roots: {errors:?}");
}

#[test]
fn empty_opaque_set_behaves_like_normal() {
    // Empty opaque set = normal validation.
    let errors = compile_and_check_with_opaque(
        r"---
params: []
---
{{ unknown }}",
        &[],
        &[],
    );
    assert_eq!(errors.len(), 1, "empty opaque set should not help");
}

// -- tmpl() displayability rejection ---------------------------------

#[test]
fn display_tmpl_param_is_compile_error() {
    let decls = vec![VarDecl {
        name: "widget".to_string(),
        var_type: VarType::Tmpl(vec![]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - widget = tmpl()
---
{{ widget }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected displayability error: {errors:?}");
    assert!(
        errors[0].contains("cannot display") && errors[0].contains("tmpl"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn display_tmpl_param_with_signature_is_compile_error() {
    let decls = vec![VarDecl {
        name: "formatter".to_string(),
        var_type: VarType::Tmpl(vec![VarDecl {
            name: "name".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - formatter = tmpl(name = str)
---
{{ formatter }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected displayability error: {errors:?}");
    assert!(
        errors[0].contains("cannot display") && errors[0].contains("include"),
        "should suggest using include, got: {}",
        errors[0]
    );
}
