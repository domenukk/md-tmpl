use md_tmpl::{Template, ctx};

#[test]
fn adv_enum_compound_delimiters() {
    let source = r"---
types:
  - Priority = enum(Low, Medium, High)
  - Status = enum(Active, Inactive)

params:
  - p = Priority
  - s = Status

allow_unused: true
---
{{ p }} - {{ s }}";
    let tmpl = Template::from_source(source).expect("enum(...) should compile");
    let ctx = ctx! {
        p: "Medium",
        s: "Active",
    };
    let rendered = tmpl.render_ctx(&ctx).expect("should render successfully");
    assert_eq!(rendered, "Medium - Active");
}

#[test]
fn adv_tmpl_compound_delimiters() {
    let source = r"---
params:
  - widget = tmpl(title = str)
  - card = tmpl(body = str)

allow_unused: true
---
hello";
    let _tmpl = Template::from_source(source).expect("tmpl(...) should compile");
}

#[test]
fn adv_nested_compound_delimiters() {
    let source = r"---
params:
  - cfg = struct(items = list(str), count = option(int))

allow_unused: true
---
hello";
    let _tmpl =
        Template::from_source(source).expect("nested parentheses delimiters should compile");
}

#[test]
fn adv_negative_prohibited_brackets() {
    let bad1 = "---
params:
  - x = list[str]
---
hello";
    assert!(
        Template::from_source(bad1).is_err(),
        "list[str] should fail"
    );

    let bad2 = "---
params:
  - x = list<str>
---
hello";
    assert!(
        Template::from_source(bad2).is_err(),
        "list<str> should fail"
    );

    let bad3 = "---
params:
  - x = struct<name = str>
---
hello";
    assert!(
        Template::from_source(bad3).is_err(),
        "struct<name = str> should fail"
    );

    let bad4 = "---
params:
  - x = struct[name = str]
---
hello";
    assert!(
        Template::from_source(bad4).is_err(),
        "struct[name = str] should fail"
    );

    let bad5 = "---
types:
  - E = enum<A, B>
params:
  - x = E
---
hello";
    assert!(
        Template::from_source(bad5).is_err(),
        "enum<A, B> should fail"
    );

    let bad6 = "---
types:
  - E = enum[A, B]
params:
  - x = E
---
hello";
    assert!(
        Template::from_source(bad6).is_err(),
        "enum[A, B] should fail"
    );

    let bad7 = "---
params:
  - x = tmpl<name = str>
---
hello";
    assert!(
        Template::from_source(bad7).is_err(),
        "tmpl<name = str> should fail"
    );

    let bad8 = "---
params:
  - x = tmpl[name = str]
---
hello";
    assert!(
        Template::from_source(bad8).is_err(),
        "tmpl[name = str] should fail"
    );
}

#[test]
fn adv_negative_mismatched_delimiters() {
    let bad1 = "---
params:
  - x = list(str]
---
hello";
    assert!(
        Template::from_source(bad1).is_err(),
        "list(str] should fail"
    );

    let bad2 = "---
params:
  - x = struct(name = str]
---
hello";
    assert!(
        Template::from_source(bad2).is_err(),
        "struct(name = str] should fail"
    );

    let bad3 = "---
params:
  - x = option(int]
---
hello";
    assert!(
        Template::from_source(bad3).is_err(),
        "option(int] should fail"
    );

    let bad4 = "---
types:
  - E = enum(A, B]
params:
  - x = E
---
hello";
    assert!(
        Template::from_source(bad4).is_err(),
        "enum(A, B] should fail"
    );
}

#[test]
fn adv_quote_stripping_type_only() {
    let source = r#"---
params:
  - items = "list(str)"
  - count = 'int'

allow_unused: true
---
hello"#;
    let _tmpl = Template::from_source(source).expect("quoted type strings should compile");
}

#[test]
fn adv_bare_relative_filename_variations() {
    let bad_src1 = r#"---
imports:
  - "[sub](dir/file.tmpl.md)"
---
hello"#;
    assert!(
        Template::from_source(bad_src1).is_err(),
        "bare subdir path should fail"
    );

    let bad_src2 = r#"---
imports:
  - "[sub](dir\\file.tmpl.md)"
---
hello"#;
    assert!(
        Template::from_source(bad_src2).is_err(),
        "bare windows subdir path should fail"
    );
}

/// Verifies that `md_tmpl::template!` emits complete `#[doc = ...]` attributes on the generated
/// module, structs, fields, enums, variants, type aliases, and constants under `#[deny(missing_docs)]`.
#[deny(missing_docs)]
pub mod strict_docs_mod {
    md_tmpl::template!(
        r#"---
types:
  - Severity = enum(Low, High(score = int))
  - TagEntry = struct(key = str, val = str)

consts:
  - BANNER = str := "STRICT"

params:
  - title = untrusted str
  - sev = Severity
  - items = list(label = str, count = int)
  - meta = struct(owner = str)

allow_unused: true
---
{{ BANNER }}: {{ title }} ({{ kind(sev) }})"# => strict_doc_tmpl
    );
}

#[test]
fn template_macro_compiles_cleanly_under_deny_missing_docs() {
    use strict_docs_mod::strict_doc_tmpl::{BANNER, Params, ParamsItemsItem, ParamsMeta, Severity};

    assert_eq!(BANNER, "STRICT");
    let p = Params {
        title: "test".to_string(),
        sev: Severity::High { score: 9 },
        items: vec![ParamsItemsItem {
            label: "a".to_string(),
            count: 1,
        }],
        meta: ParamsMeta {
            owner: "team".to_string(),
        },
    };
    let rendered = p.render().expect("render succeeds");
    assert_eq!(rendered, "STRICT: test (High)");
}
