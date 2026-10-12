//! Cross-crate integration tests exercising compile-time macros with complex templates.
//!
//! These tests verify that `md-tmpl` (runtime) and
//! `md-tmpl-macros` (compile-time codegen) work together correctly
//! across crate boundaries with enums, lists, and typed parameters.

use std::str::FromStr;

use md_tmpl::{Template, Value, ctx};
use md_tmpl_macros::include_template;

// ── Generate typed modules from templates ────────────────────────────────

include_template!("prompts/cross_crate_complex.tmpl.md");
include_template!("prompts/task_report.tmpl.md");

// Imported-type reuse: the consumer's `role` param references an enum imported
// from `roles_lib`. The `imports = { ... }` mapping tells codegen to reference
// the already-generated `roles_lib::WorkRole` directly instead of emitting a
// duplicate per-template enum.
include_template!("prompts/roles_lib.tmpl.md");
include_template!(
    "prompts/role_consumer.tmpl.md",
    imports = { roles_lib = crate::roles_lib }
);

// ── Test 1: include_template with complex types ─────────────────────────

#[test]
fn test_include_template_with_complex_types() {
    let tmpl = cross_crate_complex::template();

    let ctx = ctx! {
        username: "alice",
        role: "Admin",
        score: 95.5_f64,
        active: true,
        tags: [
            { label: "rust" },
            { label: "testing" }
        ]
    };

    let output = tmpl.render_ctx(&ctx).unwrap();

    assert_eq!(
        output,
        "\nUser: alice\nRole: Admin\nScore: 95.5\nActive: true\n\nTags:\n- rust\n- testing\n"
    );
}

// ── Test 2: include_template generates correct struct fields ───────────────

#[test]
fn test_include_template_generates_correct_structs() {
    // Construct the Params struct with all field types to verify codegen.
    let params = cross_crate_complex::Params {
        username: "bob".into(),
        role: cross_crate_complex::ParamsRole::Admin,
        score: 42.0,
        active: false,
        tags: vec![cross_crate_complex::ParamsTagsItem {
            label: "integration".into(),
        }],
    };

    // Verify field values are correctly stored.
    assert_eq!(params.username, "bob");
    assert!((params.score - 42.0).abs() < f64::EPSILON);
    assert!(!params.active);
    assert_eq!(params.tags.len(), 1);
    assert_eq!(params.tags[0].label, "integration");
}

// ── Test 3: include_template enum variant generation ───────────────────────

#[test]
fn test_include_template_enum_variants() {
    // The `task_report` template defines `Priority = enum(Critical, High, Medium, Low)`.
    // Verify all variants exist and can be pattern-matched.
    let severity = task_report::Priority::Critical;

    let label = match severity {
        task_report::Priority::Critical => "critical",
        task_report::Priority::High => "high",
        task_report::Priority::Medium => "medium",
        task_report::Priority::Low => "low",
    };

    assert_eq!(label, "critical");

    // Verify Display trait works for the top-level type alias enum.
    assert_eq!(task_report::Priority::High.to_string(), "High");
    assert_eq!(task_report::Priority::Low.to_string(), "Low");

    // Verify FromStr roundtrip.
    let parsed = task_report::Priority::from_str("medium").unwrap();
    assert_eq!(parsed, task_report::Priority::Medium);

    // Verify VARIANT_NAMES constant.
    assert_eq!(
        task_report::Priority::VARIANT_NAMES,
        ["Critical", "High", "Medium", "Low"]
    );

    // Verify ALL constant.
    assert_eq!(task_report::Priority::ALL.len(), 4);
}

// ── Test 4: validate_template catches parameter drift ───────────────────

#[test]
fn test_validate_template_catches_drift() {
    // Construct a "drifted" template at runtime that has different params than
    // what the generated struct expects.
    let drifted_source = "\
---
name: cross_crate_complex
description: Drifted version with different params
params:
  - totally_new_param = str
---

{{ totally_new_param }}
";

    let drifted_tmpl = Template::from_source(drifted_source).unwrap();

    // The generated Params::validate_template should reject the drifted template
    // because the parameter sets differ.
    let result = cross_crate_complex::Params::validate_template(&drifted_tmpl);
    assert!(
        result.is_err(),
        "validate_template should catch param drift, but got Ok"
    );
}

// ── Test 5: full roundtrip with generated Params struct ─────────────────

#[test]
fn test_render_with_generated_params() {
    let params = cross_crate_complex::Params {
        username: "eve".into(),
        role: cross_crate_complex::ParamsRole::Viewer,
        score: 77.3,
        active: true,
        tags: vec![
            cross_crate_complex::ParamsTagsItem {
                label: "performance".into(),
            },
            cross_crate_complex::ParamsTagsItem {
                label: "review".into(),
            },
        ],
    };

    // Use zero-arg render() which uses the embedded template.
    let output = params.render().unwrap();

    assert_eq!(
        output,
        "\nUser: eve\nRole: Viewer\nScore: 77.3\nActive: true\n\nTags:\n- performance\n- review\n"
    );
}

// ── Test 6: validate_template succeeds for matching template ────────────

#[test]
fn test_validate_template_succeeds_for_matching() {
    let tmpl = cross_crate_complex::template();
    cross_crate_complex::Params::validate_template(tmpl).unwrap();
}

// ── Test 7: task_report full roundtrip with typed enum params ────────────

#[test]
fn test_task_report_roundtrip_with_typed_enums() {
    let params = task_report::Params {
        title: "Update deployment pipeline".into(),
        priority: task_report::ParamsPriority::Critical,
        tasks: vec![
            task_report::ParamsTasksItem {
                name: "fix build script".into(),
                urgency: task_report::ParamsTasksItemUrgency::High,
            },
            task_report::ParamsTasksItem {
                name: "write tests".into(),
                urgency: task_report::ParamsTasksItemUrgency::Medium,
            },
        ],
    };

    let output = params.render().unwrap();

    assert_eq!(
        output,
        "\n# Task Report: Update deployment pipeline\n\nPriority: Critical\n- fix build script (High)\n- write tests (Medium)\n"
    );
}

// ── Test 8: ctx! macro interop with include_template ────────────────────

#[test]
fn test_ctx_macro_with_include_template() {
    let tmpl = task_report::template();

    // Use the ctx! macro (from md_tmpl) with macros (from md_tmpl_macros).
    let ctx = ctx! {
        title: "Refactor auth module",
        priority: "High",
        tasks: [
            { name: "extract interfaces", urgency: "Critical" },
            { name: "add error handling", urgency: "High" }
        ]
    };

    let output = tmpl.render_ctx(&ctx).unwrap();

    assert_eq!(
        output,
        "\n# Task Report: Refactor auth module\n\nPriority: High\n- extract interfaces (Critical)\n- add error handling (High)\n"
    );
}

// ── Test 9: to_context produces renderable context ──────────────────────

#[test]
fn test_to_context_interop() {
    let params = cross_crate_complex::Params {
        username: "frank".into(),
        role: cross_crate_complex::ParamsRole::Editor,
        score: 88.0,
        active: false,
        tags: vec![],
    };

    let ctx = params.to_context();

    // Verify the context contains expected keys.
    assert!(ctx.get("username").is_some());
    assert!(ctx.get("role").is_some());
    assert!(ctx.get("score").is_some());
    assert!(ctx.get("active").is_some());

    // Render with the context to verify it's valid.
    let tmpl = cross_crate_complex::template();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        output,
        "\nUser: frank\nRole: Editor\nScore: 88\nActive: false\n\nTags:\n"
    );
}

// ── Test 10: Value enum interop with template rendering ─────────────────

#[test]
fn test_value_dict_rendering() {
    let tmpl = cross_crate_complex::template();

    // Build context entirely from Value constructors.
    let mut ctx = md_tmpl::Context::new();
    ctx.set("username", Value::Str("grace".into()));
    ctx.set("role", Value::Str("Admin".into()));
    ctx.set("score", Value::Float(100.0));
    ctx.set("active", Value::Bool(true));
    ctx.set(
        "tags",
        Value::List(std::sync::Arc::new(vec![Value::new_struct([(
            "label",
            Value::Str("perf".into()),
        )])])),
    );

    let output = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        output,
        "\nUser: grace\nRole: Admin\nScore: 100\nActive: true\n\nTags:\n- perf\n"
    );
}

// ── Test 11: render_reloaded for hot-reload ─────────────────────────────

#[test]
fn test_render_reloaded_hot_reload() {
    let params = cross_crate_complex::Params {
        username: "hot_reload".into(),
        role: cross_crate_complex::ParamsRole::Admin,
        score: 50.0,
        active: true,
        tags: vec![],
    };

    // Load the same template from disk (hot-reload scenario).
    let disk_tmpl =
        Template::from_file(std::path::Path::new("prompts/cross_crate_complex.tmpl.md")).unwrap();

    // render_reloaded() validates then renders.
    let output = params.render_reloaded(&disk_tmpl).unwrap();
    assert_eq!(
        output,
        "\nUser: hot_reload\nRole: Admin\nScore: 50\nActive: true\n\nTags:\n"
    );
}

// ── Test 12: imported enum type is reused, not duplicated ───────────────

#[test]
fn test_imported_enum_type_reuse() {
    // Accepts the library enum directly; proves the consumer's param type is a
    // nominal alias of `roles_lib::WorkRole` (declared first per clippy pedantic
    // `items_after_statements`).
    fn takes_lib_role(_r: roles_lib::WorkRole) {}

    // The consumer's `role` param type is a direct alias of the library enum,
    // so it renders identically to a locally-generated enum.
    let params = role_consumer::Params {
        role: roles_lib::WorkRole::Judge,
    };
    assert_eq!(params.render().unwrap(), "\nRole: Judge\n");

    // Nominal type identity: `role_consumer::ParamsRole` IS `roles_lib::WorkRole`
    // (a `pub type` alias), so no conversion is needed at the boundary. This is
    // the whole point of the feature — downstream code can pass the shared type
    // directly instead of matching on a duplicated per-template enum.
    takes_lib_role(role_consumer::ParamsRole::Researcher);

    let alias_value: role_consumer::ParamsRole = roles_lib::WorkRole::Orchestrator;
    assert_eq!(alias_value, roles_lib::WorkRole::Orchestrator);
}

// ── Test 13: struct parameter to_context() and render_reloaded() ────────

md_tmpl_macros::template!(
    r#"
---
types:
  - LocalStatus = enum(Pending, Running, Done(exit_code = int, tags = list(str), note = option(str)))
  - DataOnlyEvent = enum(Created(id = int), Deleted(reason = str))
  - LocalUser = struct(name = str, age = int, tags = list(str), bio = option(str))
  - LocalRowList = list(key = str, weight = int)

imports:
  - "[roles_lib](./prompts/roles_lib.tmpl.md)"

params:
  - status = LocalStatus
  - opt_status = option(LocalStatus)
  - status_list = list(LocalStatus)
  - event = DataOnlyEvent
  - user = LocalUser
  - opt_user = option(LocalUser)
  - user_list = list(LocalUser)
  - rows = LocalRowList
  - inline_dict = struct(host = str, port = int)
  - ext_role = roles_lib.WorkRole
  - opt_ext_role = option(roles_lib.WorkRole)
  - ext_role_list = list(roles_lib.WorkRole)
  - ext_worker = roles_lib.WorkerSpec
  - opt_ext_worker = option(roles_lib.WorkerSpec)
  - ext_worker_list = list(roles_lib.WorkerSpec)
  - ext_rows = roles_lib.TaskRows
---
Status={{ kind(status) }} Event={{ kind(event) }} User={{ user.name }}:{{ user.age }}
Host={{ inline_dict.host }}:{{ inline_dict.port }}
ExtRole={{ kind(ext_role) }} ExtWorker={{ ext_worker.handle }}:{{ ext_worker.quota }}

> {% match opt_status %}
> {% case Some %}

OptStatus={{ kind(opt_status) }}

> {% case None %}

OptStatus=none

> {% /match %}

> {% match opt_user %}
> {% case Some %}

OptUser={{ opt_user.name }}

> {% case None %}

OptUser=none

> {% /match %}

> {% match opt_ext_role %}
> {% case Some %}

OptExtRole={{ kind(opt_ext_role) }}

> {% case None %}

OptExtRole=none

> {% /match %}

> {% match opt_ext_worker %}
> {% case Some %}

OptExtWorker={{ opt_ext_worker.handle }}

> {% case None %}

OptExtWorker=none

> {% /match %}

> {% for s in status_list %}

- S:{{ kind(s) }}

> {% /for %}

> {% for u in user_list %}

- U:{{ u.name }}

> {% /for %}

> {% for r in rows %}

- R:{{ r.key }}={{ r.weight }}

> {% /for %}

> {% for er in ext_role_list %}

- ER:{{ kind(er) }}

> {% /for %}

> {% for ew in ext_worker_list %}

- EW:{{ ew.handle }}={{ ew.quota }}

> {% /for %}

> {% for tr in ext_rows %}

- TR:{{ tr.task_id }}={{ tr.points }}

> {% /for %}
"# => unified_types_and_eq_hash,
    imports = { roles_lib = crate::roles_lib }
);

#[test]
fn test_unified_nominal_type_reuse_and_enum_helpers_and_serde() {
    use core::any::TypeId;

    // 1. Compile-time & TypeId nominal identity checks for local `types:` and `imports:`
    //    across bare `T`, `option(T)`, and `list(T)`!
    assert_eq!(
        TypeId::of::<unified_types_and_eq_hash::ParamsStatus>(),
        TypeId::of::<unified_types_and_eq_hash::LocalStatus>()
    );
    assert_eq!(
        TypeId::of::<unified_types_and_eq_hash::ParamsUser>(),
        TypeId::of::<unified_types_and_eq_hash::LocalUser>()
    );
    assert_eq!(
        TypeId::of::<unified_types_and_eq_hash::ParamsUserListItem>(),
        TypeId::of::<unified_types_and_eq_hash::LocalUser>()
    );
    assert_eq!(
        TypeId::of::<unified_types_and_eq_hash::ParamsRowsItem>(),
        TypeId::of::<unified_types_and_eq_hash::LocalRowListItem>()
    );
    assert_eq!(
        TypeId::of::<unified_types_and_eq_hash::ParamsExtRole>(),
        TypeId::of::<roles_lib::WorkRole>()
    );
    assert_eq!(
        TypeId::of::<unified_types_and_eq_hash::ParamsExtWorker>(),
        TypeId::of::<roles_lib::WorkerSpec>()
    );
    assert_eq!(
        TypeId::of::<unified_types_and_eq_hash::ParamsExtWorkerListItem>(),
        TypeId::of::<roles_lib::WorkerSpec>()
    );
    assert_eq!(
        TypeId::of::<unified_types_and_eq_hash::ParamsExtRowsItem>(),
        TypeId::of::<roles_lib::TaskRowsItem>()
    );

    // 2. DataOnly and Mixed enum helper methods (Display, as_str, VARIANT_NAMES, FromStr)
    let ev = unified_types_and_eq_hash::DataOnlyEvent::Created { id: 7 };
    assert_eq!(ev.as_str(), "Created");
    assert_eq!(ev.to_string(), "Created");
    assert_eq!(
        unified_types_and_eq_hash::DataOnlyEvent::VARIANT_NAMES,
        ["Created", "Deleted"]
    );

    let done_status = unified_types_and_eq_hash::LocalStatus::Done {
        exit_code: 0,
        tags: vec!["fast".into(), "clean".into()],
        note: Some("ok".into()),
    };
    assert_eq!(done_status.as_str(), "Done");
    assert_eq!(done_status.to_string(), "Done");
    assert_eq!(
        unified_types_and_eq_hash::LocalStatus::from_str("pending").unwrap(),
        unified_types_and_eq_hash::LocalStatus::Pending
    );
    // Data variant names and unknown strings must NOT match FromStr on mixed enums:
    assert!(unified_types_and_eq_hash::LocalStatus::from_str("done").is_err());
    assert!(unified_types_and_eq_hash::LocalStatus::from_str("nonexistent").is_err());

    // 3. Mixed enum serde round-trip with complex fields (`list(str)`, `option(str)`),
    //    including omitted optional field deserialization via `md_tmpl::to_value` / `from_value`.
    let val_done = md_tmpl::to_value(&done_status).unwrap();
    let roundtrip_done: unified_types_and_eq_hash::LocalStatus =
        md_tmpl::from_value(&val_done).unwrap();
    assert_eq!(roundtrip_done, done_status);

    let val_omitted_opt = Value::new_struct([
        ("__kind__", Value::from("Done")),
        ("exit_code", Value::from(42_i64)),
        ("tags", Value::list([Value::from("a")])),
    ]);
    let parsed_omitted: unified_types_and_eq_hash::LocalStatus =
        md_tmpl::from_value(&val_omitted_opt).unwrap();
    assert_eq!(
        parsed_omitted,
        unified_types_and_eq_hash::LocalStatus::Done {
            exit_code: 42,
            tags: vec!["a".into()],
            note: None,
        }
    );
}

#[test]
fn test_unified_eq_hash_and_struct_to_context_render_parity() {
    use std::collections::HashSet;

    let ev = unified_types_and_eq_hash::DataOnlyEvent::Created { id: 7 };
    let done_status = unified_types_and_eq_hash::LocalStatus::Done {
        exit_code: 0,
        tags: vec!["fast".into(), "clean".into()],
        note: Some("ok".into()),
    };

    // 4. Eq + Hash auto-derivation on LocalStatus (Mixed), DataOnlyEvent (DataOnly),
    //    LocalUser (Struct), and Params!
    let mut status_set = HashSet::new();
    status_set.insert(done_status.clone());
    status_set.insert(unified_types_and_eq_hash::LocalStatus::Pending);
    assert!(status_set.contains(&done_status));

    let mut event_set = HashSet::new();
    event_set.insert(ev.clone());
    assert!(event_set.contains(&ev));

    let user = unified_types_and_eq_hash::LocalUser {
        name: "alice".into(),
        age: 30,
        tags: vec!["staff".into()],
        bio: None,
    };
    let mut user_set = HashSet::new();
    user_set.insert(user.clone());
    assert!(user_set.contains(&user));

    let params = unified_types_and_eq_hash::Params {
        status: done_status,
        opt_status: Some(unified_types_and_eq_hash::LocalStatus::Running),
        status_list: vec![unified_types_and_eq_hash::LocalStatus::Pending],
        event: ev,
        user: user.clone(),
        opt_user: Some(user.clone()),
        user_list: vec![user],
        rows: vec![unified_types_and_eq_hash::LocalRowListItem {
            key: "k1".into(),
            weight: 10,
        }],
        inline_dict: unified_types_and_eq_hash::ParamsInlineDict {
            host: "localhost".into(),
            port: 8080,
        },
        ext_role: roles_lib::WorkRole::Researcher,
        opt_ext_role: Some(roles_lib::WorkRole::Judge),
        ext_role_list: vec![roles_lib::WorkRole::Orchestrator],
        ext_worker: roles_lib::WorkerSpec {
            handle: "w1".into(),
            quota: 5,
        },
        opt_ext_worker: Some(roles_lib::WorkerSpec {
            handle: "w2".into(),
            quota: 9,
        }),
        ext_worker_list: vec![roles_lib::WorkerSpec {
            handle: "w3".into(),
            quota: 12,
        }],
        ext_rows: vec![roles_lib::TaskRowsItem {
            task_id: "t1".into(),
            points: 3,
        }],
    };

    let mut params_set = HashSet::new();
    params_set.insert(params.clone());
    assert!(params_set.contains(&params));

    // 5. Verify native `render()`, `to_context()` + `render_ctx()`, and `render_reloaded()`
    //    all produce identical output (exercising the fixed `typed_dict_codegen` inner_ctx!).
    let native_rendered = params.render().unwrap();
    let ctx = params.to_context();
    let ctx_rendered = unified_types_and_eq_hash::template()
        .render_ctx(&ctx)
        .unwrap();
    let reloaded_rendered = params
        .render_reloaded(unified_types_and_eq_hash::template())
        .unwrap();
    assert_eq!(native_rendered, ctx_rendered);
    assert_eq!(native_rendered, reloaded_rendered);
    assert!(native_rendered.contains("Status=Done Event=Created User=alice:30"));
    assert!(native_rendered.contains("Host=localhost:8080"));
    assert!(native_rendered.contains("ExtRole=Researcher ExtWorker=w1:5"));
    assert!(native_rendered.contains("OptStatus=Running"));
    assert!(native_rendered.contains("OptUser=alice"));
    assert!(native_rendered.contains("OptExtRole=Judge"));
    assert!(native_rendered.contains("OptExtWorker=w2"));
    assert!(native_rendered.contains("- S:Pending"));
    assert!(native_rendered.contains("- U:alice"));
    assert!(native_rendered.contains("- R:k1=10"));
    assert!(native_rendered.contains("- ER:Orchestrator"));
    assert!(native_rendered.contains("- EW:w3=12"));
    assert!(native_rendered.contains("- TR:t1=3"));
}

// ── Test 14: large template segment chunking under deny(large_stack_arrays) ──

#[deny(clippy::large_stack_arrays)]
mod large_segment_chunk_test {
    md_tmpl_macros::template!(
        r#"
---
params:
  - x = int
---
{{ x }}-1 {{ x }}-2 {{ x }}-3 {{ x }}-4 {{ x }}-5 {{ x }}-6 {{ x }}-7 {{ x }}-8 {{ x }}-9 {{ x }}-10
{{ x }}-11 {{ x }}-12 {{ x }}-13 {{ x }}-14 {{ x }}-15 {{ x }}-16 {{ x }}-17 {{ x }}-18 {{ x }}-19 {{ x }}-20
{{ x }}-21 {{ x }}-22 {{ x }}-23 {{ x }}-24 {{ x }}-25 {{ x }}-26 {{ x }}-27 {{ x }}-28 {{ x }}-29 {{ x }}-30
{{ x }}-31 {{ x }}-32 {{ x }}-33 {{ x }}-34 {{ x }}-35 {{ x }}-36 {{ x }}-37 {{ x }}-38 {{ x }}-39 {{ x }}-40
{{ x }}-41 {{ x }}-42 {{ x }}-43 {{ x }}-44 {{ x }}-45 {{ x }}-46 {{ x }}-47 {{ x }}-48 {{ x }}-49 {{ x }}-50
{{ x }}-51 {{ x }}-52 {{ x }}-53 {{ x }}-54 {{ x }}-55 {{ x }}-56 {{ x }}-57 {{ x }}-58 {{ x }}-59 {{ x }}-60
{{ x }}-61 {{ x }}-62 {{ x }}-63 {{ x }}-64 {{ x }}-65 {{ x }}-66 {{ x }}-67 {{ x }}-68 {{ x }}-69 {{ x }}-70
{{ x }}-71 {{ x }}-72 {{ x }}-73 {{ x }}-74 {{ x }}-75 {{ x }}-76 {{ x }}-77 {{ x }}-78 {{ x }}-79 {{ x }}-80
"# => many_segments
    );
}

#[test]
fn test_large_segment_chunk_outlining_matches_runtime() {
    let p = large_segment_chunk_test::many_segments::Params { x: 9 };
    let direct = p.render().unwrap();
    let via_ctx = large_segment_chunk_test::many_segments::template()
        .render_ctx(&p.to_context())
        .unwrap();
    assert_eq!(direct, via_ctx);
    assert!(direct.starts_with("9-1 9-2 9-3"));
    assert!(direct.trim_end().ends_with("9-79 9-80"));
}
