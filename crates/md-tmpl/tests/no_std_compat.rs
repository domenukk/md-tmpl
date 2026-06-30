//! Integration tests for `no_std` compatibility.
//!
//! These tests verify that the public API surface works correctly with the
//! `alloc`-based types that back the `no_std` mode.  While integration tests
//! always run under `std`, the code paths exercised here are the *same* paths
//! compiled under `no_std` — they use `alloc` types re-exported through
//! `__private` and the `compat` module.

use md_tmpl::{CompileOptions, Context, Template, Value, ctx};

// ---------------------------------------------------------------------------
// 1. __private re-exports — used by proc-macro generated code
// ---------------------------------------------------------------------------

#[test]
fn private_arc_is_usable() {
    let arc = md_tmpl::__private::Arc::new(42);
    assert_eq!(*arc, 42);
}

#[test]
fn private_cow_borrowed() {
    let cow = md_tmpl::__private::Cow::Borrowed("hello");
    assert_eq!(&*cow, "hello");
}

#[test]
fn private_cow_owned() {
    let cow: md_tmpl::__private::Cow<'_, str> =
        md_tmpl::__private::Cow::Owned(md_tmpl::__private::String::from("owned"));
    assert_eq!(&*cow, "owned");
}

#[test]
fn private_string() {
    let s = md_tmpl::__private::String::from("test");
    assert_eq!(s, "test");
}

#[test]
fn private_vec_macro() {
    let v = md_tmpl::__private::vec![1, 2, 3];
    assert_eq!(v, [1, 2, 3]);
}

#[test]
fn private_vec_type() {
    let v: md_tmpl::__private::Vec<i32> = md_tmpl::__private::vec![10, 20];
    assert_eq!(v.len(), 2);
}

#[test]
fn private_format_macro() {
    let s = md_tmpl::__private::format!("hello {}", "world");
    assert_eq!(s, "hello world");
}

#[test]
fn private_lazy_initializes() {
    static VAL: md_tmpl::__private::LazyLock<i32> = md_tmpl::__private::LazyLock::new(|| 99);
    assert_eq!(*VAL, 99);
}

// ---------------------------------------------------------------------------
// 2. ctx! / __value! macros — must work without `extern crate alloc`
// ---------------------------------------------------------------------------

#[test]
fn ctx_macro_simple_values() {
    let ctx = ctx! {
        name: "Alice",
        count: 42_i64,
        flag: true,
    };
    assert_eq!(ctx.get("name"), Some(&Value::from("Alice")));
    assert_eq!(ctx.get("count"), Some(&Value::from(42_i64)));
    assert_eq!(ctx.get("flag"), Some(&Value::from(true)));
}

#[test]
fn ctx_macro_with_list() {
    let ctx = ctx! {
        items: ["a", "b", "c"],
    };
    let items = ctx.get("items").unwrap();
    if let Value::List(list) = items {
        assert_eq!(list.len(), 3);
    } else {
        panic!("expected List, got {items:?}");
    }
}

#[test]
fn ctx_macro_with_nested_dict() {
    let ctx = ctx! {
        user: { name: "Bob", age: 30_i64 },
    };
    let user = ctx.get("user").unwrap();
    assert_eq!(user.get_field("name"), Some(&Value::from("Bob")));
    assert_eq!(user.get_field("age"), Some(&Value::from(30_i64)));
}

#[test]
fn ctx_macro_with_nested_list_of_dicts() {
    let ctx = ctx! {
        items: [
            { label: "x" },
            { label: "y" },
        ],
    };
    let items = ctx.get("items").unwrap();
    if let Value::List(list) = items {
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].get_field("label"), Some(&Value::from("x")));
    } else {
        panic!("expected List");
    }
}

#[test]
fn value_struct_basic() {
    let d = Value::new_struct([("key", Value::from("val")), ("num", Value::from(7_i64))]);
    assert_eq!(d.get_field("key"), Some(&Value::from("val")));
    assert_eq!(d.get_field("num"), Some(&Value::from(7_i64)));
}

#[test]
fn value_struct_nested() {
    let d = Value::new_struct([("outer", Value::new_struct([("inner", Value::from("deep"))]))]);
    let outer = d.get_field("outer").unwrap();
    assert_eq!(outer.get_field("inner"), Some(&Value::from("deep")));
}

// ---------------------------------------------------------------------------
// 3. Value type — alloc-based operations
// ---------------------------------------------------------------------------

#[test]
fn value_str_from_string() {
    let v = Value::from(String::from("hello"));
    assert_eq!(v.to_string(), "hello");
}

#[test]
fn value_str_from_str_ref() {
    let v = Value::from("hello");
    assert_eq!(v.to_string(), "hello");
}

#[test]
fn value_list_construction() {
    let v = Value::List(std::sync::Arc::new(vec![
        Value::from("a"),
        Value::from("b"),
    ]));
    if let Value::List(list) = &v {
        assert_eq!(list.len(), 2);
    } else {
        panic!("expected List");
    }
}

#[test]
fn value_dict_construction() {
    let v = Value::new_struct([("key", Value::from("val"))]);
    assert_eq!(v.get_field("key"), Some(&Value::from("val")));
}

#[test]
fn value_display_types() {
    assert_eq!(Value::from("text").to_string(), "text");
    assert_eq!(Value::from(42_i64).to_string(), "42");
    assert_eq!(Value::from(true).to_string(), "true");
    assert_eq!(Value::from(false).to_string(), "false");
    assert_eq!(Value::from(3.25_f64).to_string(), "3.25");
}

#[test]
fn value_type_name() {
    assert_eq!(Value::from("s").type_name(), "str");
    assert_eq!(Value::from(1_i64).type_name(), "int");
    assert_eq!(Value::from(true).type_name(), "bool");
    assert_eq!(Value::from(1.0_f64).type_name(), "float");
}

// ---------------------------------------------------------------------------
// 4. Context — alloc-based key/value store
// ---------------------------------------------------------------------------

#[test]
fn context_new_is_empty() {
    let ctx = Context::new();
    assert!(ctx.get("anything").is_none());
}

#[test]
fn context_set_and_get() {
    let mut ctx = Context::new();
    ctx.set("key", Value::from("value"));
    assert_eq!(ctx.get("key"), Some(&Value::from("value")));
}

#[test]
fn context_overwrite() {
    let mut ctx = Context::new();
    ctx.set("k", Value::from(1_i64));
    ctx.set("k", Value::from(2_i64));
    assert_eq!(ctx.get("k"), Some(&Value::from(2_i64)));
}

// ---------------------------------------------------------------------------
// 5. Template — from_source and render_ctx (the core no_std path)
// ---------------------------------------------------------------------------

#[test]
fn template_from_source_simple() {
    let tmpl = Template::from_source(
        r"---
params: [name = str]
---
Hello, {{ name }}!",
    )
    .unwrap();
    assert_eq!(tmpl.declarations().len(), 1);
    assert_eq!(tmpl.declarations()[0].name, "name");
}

#[test]
fn template_render_simple() {
    let tmpl = Template::from_source(
        r"---
params: [greeting = str]
---
{{ greeting }}, world!",
    )
    .unwrap();
    let output = tmpl.render_ctx(&ctx! { greeting: "Hello" }).unwrap();
    assert_eq!(output, "Hello, world!");
}

#[test]
fn template_render_with_list() {
    let tmpl = Template::from_source(
        r"---
params: [items = list(label = str)]
---

> {% for item in items %}

{{ item.label }}

> {% /for %}",
    )
    .unwrap();
    let output = tmpl
        .render_ctx(&ctx! {
            items: [{ label: "alpha" }, { label: "beta" }],
        })
        .unwrap();
    assert_eq!(output, "alpha\nbeta\n");
}

#[test]
fn template_render_with_conditional() {
    let tmpl = Template::from_source(
        r"---
params: [show = bool, msg = str]
---

> {% if show %}

{{ msg }}

> {% /if %}",
    )
    .unwrap();
    let shown = tmpl
        .render_ctx(&ctx! { show: true, msg: "visible" })
        .unwrap();
    assert_eq!(shown, "visible\n");
    let hidden = tmpl
        .render_ctx(&ctx! { show: false, msg: "invisible" })
        .unwrap();
    assert_eq!(hidden, "");
}

#[test]
fn template_render_with_defaults() {
    let tmpl = Template::from_source(
        r#"---
params: [name = str := "world"]
---
Hello, {{ name }}!"#,
    )
    .unwrap();
    let output = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(output, "Hello, world!");
}

#[test]
fn template_render_with_filters() {
    let tmpl = Template::from_source(
        r"---
params: [name = str]
---
{{ name | upper }}",
    )
    .unwrap();
    let output = tmpl.render_ctx(&ctx! { name: "hello" }).unwrap();
    assert_eq!(output, "HELLO");
}

#[test]
fn template_render_with_nested_dict() {
    let tmpl = Template::from_source(
        r"---
params: [user = struct(name = str, role = str)]
---
{{ user.name }} is a {{ user.role }}",
    )
    .unwrap();
    let output = tmpl
        .render_ctx(&ctx! {
            user: { name: "Alice", role: "admin" },
        })
        .unwrap();
    assert_eq!(output, "Alice is a admin");
}

#[test]
fn template_render_enum_match() {
    let tmpl = Template::from_source(
        r"---
params: [status = enum(Open, Closed)]
---

> {% match status %}
> {% case Open %}

open

> {% case Closed %}

closed

> {% /match %}",
    )
    .unwrap();
    let output = tmpl.render_ctx(&ctx! { status: "Open" }).unwrap();
    assert_eq!(output, "open\n");
}

// ---------------------------------------------------------------------------
// 6. Frontmatter parsing — works under no_std (no filesystem ops)
// ---------------------------------------------------------------------------

#[test]
fn parse_frontmatter_returns_declarations() {
    let (fm, body) = md_tmpl::parse_frontmatter(
        r"---
params: [x = str, y = int]
---
body text",
    )
    .unwrap();
    assert_eq!(fm.declarations.len(), 2);
    assert_eq!(fm.declarations[0].name, "x");
    assert_eq!(fm.declarations[1].name, "y");
    assert_eq!(body, "body text");
}

#[test]
fn parse_frontmatter_with_types() {
    let (fm, _) = md_tmpl::parse_frontmatter(
        r"---
types:
  - Priority = enum(Low, High)

params: [p = Priority]
---
body",
    )
    .unwrap();
    assert!(fm.type_aliases.contains_key("Priority"));
}

#[test]
fn strip_frontmatter_removes_header() {
    let body = md_tmpl::strip_frontmatter(
        r"---
params: [x = str]
---
actual body",
    );
    assert_eq!(body.unwrap(), "actual body");
}

#[test]
#[cfg(feature = "std")]
fn extract_template_stem_works() {
    use std::path::Path;
    assert_eq!(
        md_tmpl::extract_template_stem(Path::new("hello.tmpl.md")),
        "hello"
    );
    assert_eq!(
        md_tmpl::extract_template_stem(Path::new("no_match.txt")),
        "no_match.txt"
    );
}

#[test]
#[cfg(not(feature = "std"))]
fn extract_template_stem_works() {
    assert_eq!(md_tmpl::extract_template_stem("hello.tmpl.md"), "hello");
    assert_eq!(
        md_tmpl::extract_template_stem("no_match.txt"),
        "no_match.txt"
    );
}

// ---------------------------------------------------------------------------
// 7. Error types — available under no_std
// ---------------------------------------------------------------------------

#[test]
fn template_error_display() {
    let err = Template::from_source(
        r"---
params: [x = UnknownType]
---
body",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(!msg.is_empty(), "error should have a display message");
}

#[test]
fn template_error_undefined_variable() {
    let tmpl = Template::from_source(
        r"---
params: [name = str]
---
{{ name }}",
    )
    .unwrap();
    let err = tmpl.render_ctx(&Context::new()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("name"),
        "should mention the missing variable: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 8. source_hash — stable across alloc backends
// ---------------------------------------------------------------------------

#[test]
fn source_hash_stable() {
    let src = r"---
params: [x = str]
---
{{ x }}";
    let t1 = Template::from_source(src).unwrap();
    let t2 = Template::from_source(src).unwrap();
    assert_eq!(t1.source_hash(), t2.source_hash());
}

#[test]
fn source_hash_differs_for_different_source() {
    let t1 = Template::from_source(
        r"---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();
    let t2 = Template::from_source(
        r"---
params: [y = str]
---
{{ y }}",
    )
    .unwrap();
    assert_ne!(t1.source_hash(), t2.source_hash());
}

// ---------------------------------------------------------------------------
// 9. VarDecl / VarType — type system works under alloc
// ---------------------------------------------------------------------------

#[test]
fn var_type_display() {
    use md_tmpl::VarType;
    assert_eq!(VarType::Str.to_string(), "str");
    assert_eq!(VarType::Int.to_string(), "int");
    assert_eq!(VarType::Bool.to_string(), "bool");
    assert_eq!(VarType::Float.to_string(), "float");
}

#[test]
fn to_pascal_case() {
    assert_eq!(md_tmpl::to_pascal_case("hello_world"), "HelloWorld");
    assert_eq!(md_tmpl::to_pascal_case("simple"), "Simple");
    assert_eq!(md_tmpl::to_pascal_case("already_Pascal"), "AlreadyPascal");
}

// ---------------------------------------------------------------------------
// 10. Frontmatter — name/description metadata
// ---------------------------------------------------------------------------

#[test]
fn frontmatter_name_and_description() {
    let (fm, _) = md_tmpl::parse_frontmatter(
        r"---
name: test_tmpl
description: A test
params: [x = str]
---
body",
    )
    .unwrap();
    assert_eq!(fm.name, Some("test_tmpl".to_string()));
    assert_eq!(fm.description, Some("A test".to_string()));
}

#[test]
fn frontmatter_allow_unused() {
    let (fm, _) = md_tmpl::parse_frontmatter(
        r"---
params: [x = str]
allow_unused: true
---
body",
    )
    .unwrap();
    assert!(fm.allow_unused);
}

// ---------------------------------------------------------------------------
// 11. Template::defaults — default values work under alloc
// ---------------------------------------------------------------------------

#[test]
fn template_defaults_extracted() {
    let tmpl = Template::from_source(
        r#"---
params: [name = str := "world", count = int := 5]
---
{{ name }} {{ count }}"#,
    )
    .unwrap();
    let defaults = tmpl.defaults();
    assert_eq!(defaults.get("name"), Some(&Value::from("world")));
    assert_eq!(defaults.get("count"), Some(&Value::from(5_i64)));
}

// ---------------------------------------------------------------------------
// 12. Inline template includes — work under no_std
// ---------------------------------------------------------------------------

#[test]
fn inline_template_include_no_std() {
    let tmpl = Template::from_source(
        r"---
params: [name = str]
---

> {% tmpl greeting %}

---

params: [name = str]
---
Hello, {{ name }}!

> {% /tmpl %}

> {% include greeting with name=name %}
",
    )
    .unwrap();
    let output = tmpl.render_ctx(&ctx! { name: "World" }).unwrap();
    assert_eq!(output, "Hello, World!\n");
}

#[test]
fn inline_template_include_with_override() {
    let tmpl = Template::from_source(
        r"---
params: [name = str, greeting = str]
---

> {% tmpl greet %}

---

params:
  - name = str
  - greeting = str
---
{{ greeting }} {{ name }}!

> {% /tmpl %}

> {% include greet with name=name, greeting=greeting %}
",
    )
    .unwrap();
    let output = tmpl
        .render_ctx(&ctx! { name: "Alice", greeting: "Hey" })
        .unwrap();
    assert_eq!(output, "Hey Alice!\n");
}

#[test]
fn tmpl_param_include_no_std() {
    // Create a "child" template as a Value::Tmpl parameter.
    let (child, _fm) = Template::compile(
        r"---
params: [msg = str]
---
[{{ msg }}]",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap();

    // Create a "parent" template that includes the child via a tmpl parameter.
    let parent = Template::from_source(
        r"---
params: [widget = tmpl(msg = str), text = str]
---
before

> {% include widget with msg=text %}

after
",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("widget", Value::Tmpl(std::sync::Arc::new(child)));
    ctx.set("text", "hello");
    let output = parent.render_ctx(&ctx).unwrap();
    assert_eq!(output, "before\n[hello]after\n");
}

#[test]
fn tmpl_param_include_for_each() {
    let (child, _fm) = Template::compile(
        r"---
params: [item = str]
---
- {{ item.label }}
",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap();

    let parent = Template::from_source(
        r"---
params: [row = tmpl(item = str), items = list(label = str)]
---
> {% include row for item in items %}
",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("row", Value::Tmpl(std::sync::Arc::new(child)));
    ctx.set(
        "items",
        Value::List(std::sync::Arc::new(vec![
            Value::new_struct([("label", Value::from("alpha"))]),
            Value::new_struct([("label", Value::from("beta"))]),
        ])),
    );
    let output = parent.render_ctx(&ctx).unwrap();
    assert_eq!(output, "- alpha\n- beta\n");
}

#[test]
fn tmpl_param_include_type_mismatch_errors() {
    let child = Template::compile(
        r"---
params: [count = int]
---
{{ count }}",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap()
    .0;

    let parent = Template::from_source(
        r"---
params: [widget = tmpl(count = int), val = str]
---
> {% include widget with count=val %}
",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("widget", Value::Tmpl(std::sync::Arc::new(child)));
    ctx.set("val", "not an int");
    let err = parent.render_ctx(&ctx).unwrap_err();
    assert!(
        matches!(err, md_tmpl::TemplateError::TypeMismatch { .. }),
        "expected TypeMismatch, got: {err}"
    );
}

#[test]
fn tmpl_param_include_contract_rejects_missing_params() {
    let (child, _fm) = Template::compile(
        r"---
params: [title = str, count = int]
---
{{ title }} ({{ count }})",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap();

    let parent = Template::from_source(
        r"---
params: [widget = tmpl(title = str, count = int)]
---
> {% include widget %}
",
    )
    .unwrap();

    let mut ctx = Context::new();
    ctx.set("widget", Value::Tmpl(std::sync::Arc::new(child)));
    let err = parent.render_ctx(&ctx).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("title"), "error should mention 'title': {msg}");
    assert!(msg.contains("count"), "error should mention 'count': {msg}");
}

// ---------------------------------------------------------------------------
// 13. Ergonomic tmpl param API — From<Template>, From<&Template>, ctx!
// ---------------------------------------------------------------------------

#[test]
fn tmpl_param_via_from_template_owned() {
    let (child, _fm) = Template::compile(
        r"---
params: [msg = str]
---
[{{ msg }}]",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap();
    let parent = Template::from_source(
        r"---
params: [widget = tmpl(msg = str), text = str]
---
> {% include widget with msg=text %}
",
    )
    .unwrap();

    let mut ctx = Context::new();
    // Ergonomic: pass Template directly, no Arc wrapping
    ctx.set("widget", child);
    ctx.set("text", "hello");
    let output = parent.render_ctx(&ctx).unwrap();
    assert_eq!(output, "[hello]");
}

#[test]
fn tmpl_param_via_from_template_ref() {
    let (child, _fm) = Template::compile(
        r"---
params: [msg = str]
---
({{ msg }})",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap();
    let parent = Template::from_source(
        r"---
params: [widget = tmpl(msg = str), text = str]
---
> {% include widget with msg=text %}
",
    )
    .unwrap();

    let mut ctx = Context::new();
    // Ergonomic: pass &Template (like from include_template!/template!)
    ctx.set("widget", &child);
    ctx.set("text", "world");
    let output = parent.render_ctx(&ctx).unwrap();
    assert_eq!(output, "(world)");
}

#[test]
fn tmpl_param_via_context_builder() {
    let child = Template::compile(
        r"---
params: [msg = str]
---
{{ msg }}!",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap()
    .0;
    let parent = Template::from_source(
        r"---
params: [widget = tmpl(msg = str), text = str]
---
> {% include widget with msg=text %}
",
    )
    .unwrap();

    // Builder chain — .var() returns Self
    let ctx = Context::new().var("widget", child).var("text", "hi");
    let output = parent.render_ctx(&ctx).unwrap();
    assert_eq!(output, "hi!");
}

#[test]
fn tmpl_param_via_ctx_macro() {
    let (child, _fm) = Template::compile(
        r"---
params: [msg = str]
---
{{ msg }}!!",
        CompileOptions::default().allow_unused(true),
    )
    .unwrap();
    let parent = Template::from_source(
        r"---
params: [widget = tmpl(msg = str), text = str]
---
> {% include widget with msg=text %}
",
    )
    .unwrap();

    // ctx! macro with parenthesized expression for the template
    let ctx = ctx! { widget: (child), text: "boom" };
    let output = parent.render_ctx(&ctx).unwrap();
    assert_eq!(output, "boom!!");
}

#[test]
fn tmpl_param_from_source_inline() {
    // One-liner: create and pass a template in one expression
    let parent = Template::from_source(
        r"---
params: [widget = tmpl(name = str), who = str]
---
> {% include widget with name=who %}
",
    )
    .unwrap();

    let ctx = ctx! {
        widget: (Template::compile(
            r"---
params: [name = str]
---
Hi {{ name }}!",
            CompileOptions::default().allow_unused(true),
        ).unwrap().0),
        who: "Alice"
    };
    let output = parent.render_ctx(&ctx).unwrap();
    assert_eq!(output, "Hi Alice!");
}
