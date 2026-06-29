use prompt_templates::{Template, ctx};

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
    let bad1 = "---\nparams: [x = list[str]]\n---\nhello";
    assert!(
        Template::from_source(bad1).is_err(),
        "list[str] should fail"
    );

    let bad2 = "---\nparams: [x = list<str>]\n---\nhello";
    assert!(
        Template::from_source(bad2).is_err(),
        "list<str> should fail"
    );

    let bad3 = "---\nparams: [x = struct<name = str>]\n---\nhello";
    assert!(
        Template::from_source(bad3).is_err(),
        "struct<name = str> should fail"
    );

    let bad4 = "---\nparams: [x = struct[name = str]]\n---\nhello";
    assert!(
        Template::from_source(bad4).is_err(),
        "struct[name = str] should fail"
    );

    let bad5 = "---\ntypes: [E = enum<A, B>]\nparams: [x = E]\n---\nhello";
    assert!(
        Template::from_source(bad5).is_err(),
        "enum<A, B> should fail"
    );

    let bad6 = "---\ntypes: [E = enum[A, B]]\nparams: [x = E]\n---\nhello";
    assert!(
        Template::from_source(bad6).is_err(),
        "enum[A, B] should fail"
    );

    let bad7 = "---\nparams: [x = tmpl<name = str>]\n---\nhello";
    assert!(
        Template::from_source(bad7).is_err(),
        "tmpl<name = str> should fail"
    );

    let bad8 = "---\nparams: [x = tmpl[name = str]]\n---\nhello";
    assert!(
        Template::from_source(bad8).is_err(),
        "tmpl[name = str] should fail"
    );
}

#[test]
fn adv_negative_mismatched_delimiters() {
    let bad1 = "---\nparams: [x = list(str]]\n---\nhello";
    assert!(
        Template::from_source(bad1).is_err(),
        "list(str] should fail"
    );

    let bad2 = "---\nparams: [x = struct(name = str]]\n---\nhello";
    assert!(
        Template::from_source(bad2).is_err(),
        "struct(name = str] should fail"
    );

    let bad3 = "---\nparams: [x = option(int]]\n---\nhello";
    assert!(
        Template::from_source(bad3).is_err(),
        "option(int] should fail"
    );

    let bad4 = "---\ntypes: [E = enum(A, B]]\nparams: [x = E]\n---\nhello";
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
