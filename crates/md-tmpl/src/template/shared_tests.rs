//! Shared cross-backend test runner.
//!
//! Reads test case definitions from `tests/shared/inline_tmpl_tests.toml`,
//! `tests/shared/include_tests.toml`, `tests/shared/inline_control_tests.toml`,
//! and `tests/shared/tmpl_param_tests.toml`, and runs them against the Rust
//! template engine.
//!
//! These same fixtures are consumed by the TypeScript backend's test runner,
//! ensuring behavioral parity between implementations.
//!
//! Templates use `template` / `parent_template` (proper multiline TOML strings)
//! so the fixtures stay readable and literal.

use crate::{CompileOptions, Context, Template, Value};

fn toml_to_value(val: &toml::Value) -> Value {
    match val {
        toml::Value::String(s) => Value::Str(s.clone()),
        toml::Value::Integer(i) => Value::Int(*i),
        toml::Value::Float(f) => Value::Float(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(dt) => Value::Str(dt.to_string()),
        toml::Value::Array(arr) => {
            Value::List(std::sync::Arc::new(arr.iter().map(toml_to_value).collect()))
        }
        toml::Value::Table(tbl) => {
            let map: crate::compat::HashMap<String, Value> = tbl
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_value(v)))
                .collect();
            Value::Struct(std::sync::Arc::new(map))
        }
    }
}

fn toml_to_context(val: Option<&toml::Value>) -> Context {
    let mut ctx = Context::new();
    if let Some(toml::Value::Table(tbl)) = val {
        for (k, v) in tbl {
            ctx.set(k, toml_to_value(v));
        }
    }
    ctx
}

fn get_template_src(tc: &toml::Table) -> String {
    let val = tc
        .get("template")
        .and_then(|v| v.as_str())
        .expect("missing template");
    if val.ends_with(".tmpl.md") {
        let full_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/shared")
            .join(val);
        if full_path.exists() {
            return std::fs::read_to_string(&full_path).unwrap_or_else(|e| {
                panic!("failed to read template file {}: {e}", full_path.display())
            });
        }
    }
    val.to_string()
}

// ---------------------------------------------------------------------------
// Shared inline template tests
// ---------------------------------------------------------------------------

#[test]
fn shared_inline_tmpl_tests() {
    let toml_str = include_str!("../../../../tests/shared/inline_tmpl_tests.toml");
    let root: toml::Table = toml::from_str(toml_str).expect("parse toml");
    let tests = root
        .get("tests")
        .and_then(|v| v.as_array())
        .expect("tests array");

    for tc_val in tests {
        let tc = tc_val.as_table().expect("test case table");
        let name = tc.get("name").and_then(|v| v.as_str()).unwrap();
        let template_src = get_template_src(tc);
        let params = tc.get("params");

        if let Some(expected) = tc.get("expected_output").and_then(|v| v.as_str()) {
            let tmpl = Template::from_source(&template_src)
                .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            let ctx = toml_to_context(params);
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_substr) = tc.get("expected_error").and_then(|v| v.as_str()) {
            let result = Template::from_source(&template_src).and_then(|tmpl| {
                let ctx = toml_to_context(params);
                tmpl.render_ctx(&ctx)
            });
            let err = result.unwrap_err();
            assert!(
                err.to_string()
                    .to_lowercase()
                    .contains(&expected_substr.to_lowercase()),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared file-based include tests
// ---------------------------------------------------------------------------

#[test]
fn shared_include_tests() {
    let toml_str = include_str!("../../../../tests/shared/include_tests.toml");
    let root: toml::Table = toml::from_str(toml_str).expect("parse toml");
    let tests = root
        .get("tests")
        .and_then(|v| v.as_array())
        .expect("tests array");

    for tc_val in tests {
        let tc = tc_val.as_table().expect("test case table");
        let name = tc.get("name").and_then(|v| v.as_str()).unwrap();
        let parent_template = {
            let val = tc
                .get("parent_template")
                .and_then(|v| v.as_str())
                .expect("missing parent_template");
            if val.ends_with(".tmpl.md") {
                let full_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/shared")
                    .join(val);
                if full_path.exists() {
                    std::fs::read_to_string(&full_path).unwrap_or_else(|e| {
                        panic!(
                            "failed to read parent template file {}: {e}",
                            full_path.display()
                        )
                    })
                } else {
                    val.to_string()
                }
            } else {
                val.to_string()
            }
        };
        let files = tc
            .get("files")
            .and_then(|v| v.as_table())
            .expect("missing files table");
        let params = tc.get("params");

        // Create temp dir with include files.
        let dir = tempfile::tempdir().unwrap();
        for (filename, content_val) in files {
            let content_str = content_val.as_str().expect("file content must be string");
            let content = if content_str.ends_with(".tmpl.md") {
                let full_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/shared")
                    .join(content_str);
                if full_path.exists() {
                    std::fs::read_to_string(&full_path).unwrap_or_else(|e| {
                        panic!("failed to read include file {}: {e}", full_path.display())
                    })
                } else {
                    content_str.to_string()
                }
            } else {
                content_str.to_string()
            };
            let file_path = dir.path().join(filename);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(file_path, content).unwrap();
        }

        if let Some(expected) = tc.get("expected_output").and_then(|v| v.as_str()) {
            let (tmpl, _fm) = Template::compile(
                &parent_template,
                CompileOptions::default().base_dir(dir.path()),
            )
            .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            let ctx = toml_to_context(params);
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_substr) = tc.get("expected_error").and_then(|v| v.as_str()) {
            let result = Template::compile(
                &parent_template,
                CompileOptions::default().base_dir(dir.path()),
            )
            .and_then(|(tmpl, _fm)| {
                let ctx = toml_to_context(params);
                tmpl.render_ctx(&ctx)
            });
            let err = result.unwrap_err();
            assert!(
                err.to_string()
                    .to_lowercase()
                    .contains(&expected_substr.to_lowercase()),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared inline control flow tests (if, for, match, etc.)
// ---------------------------------------------------------------------------

#[test]
fn shared_inline_control_tests() {
    let toml_str = include_str!("../../../../tests/shared/inline_control_tests.toml");
    let root: toml::Table = toml::from_str(toml_str).expect("parse toml");
    let tests = root
        .get("tests")
        .and_then(|v| v.as_array())
        .expect("tests array");

    for tc_val in tests {
        let tc = tc_val.as_table().expect("test case table");
        let name = tc.get("name").and_then(|v| v.as_str()).unwrap();
        let template_src = get_template_src(tc);
        let params = tc.get("params");

        if let Some(expected) = tc.get("expected_output").and_then(|v| v.as_str()) {
            let tmpl = Template::from_source(&template_src)
                .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            if let Ok((fm, _)) = crate::frontmatter::parse_frontmatter(&template_src) {
                let mut opaque_roots: crate::compat::HashSet<String> =
                    crate::compat::HashSet::new();
                for c in tmpl.consts.keys() {
                    opaque_roots.insert(c.clone());
                }
                for c in tmpl.imported_consts.keys() {
                    if let Some(stem) = c.split('.').next() {
                        opaque_roots.insert(stem.to_string());
                    }
                }
                let type_errors = crate::compiled::validate_field_accesses_full(
                    &tmpl.segments,
                    &tmpl.declared_variables,
                    &fm.type_aliases,
                    &opaque_roots,
                );
                if let Some(err) = type_errors.into_iter().next() {
                    panic!("[{name}] static type check failed: {err}");
                }
            }
            let ctx = toml_to_context(params);
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_substr) = tc.get("expected_error").and_then(|v| v.as_str()) {
            let result = Template::from_source(&template_src).and_then(|tmpl| {
                if let Ok((fm, _)) = crate::frontmatter::parse_frontmatter(&template_src) {
                    let mut opaque_roots: crate::compat::HashSet<String> =
                        crate::compat::HashSet::new();
                    for c in tmpl.consts.keys() {
                        opaque_roots.insert(c.clone());
                    }
                    for c in tmpl.imported_consts.keys() {
                        if let Some(stem) = c.split('.').next() {
                            opaque_roots.insert(stem.to_string());
                        }
                    }
                    let type_errors = crate::compiled::validate_field_accesses_full(
                        &tmpl.segments,
                        &tmpl.declared_variables,
                        &fm.type_aliases,
                        &opaque_roots,
                    );
                    if let Some(err) = type_errors.into_iter().next() {
                        return Err(crate::error::TemplateError::syntax(err));
                    }
                }
                let ctx = toml_to_context(params);
                tmpl.render_ctx(&ctx)
            });
            let err = match result {
                Err(e) => e,
                Ok(out) => panic!(
                    "[{name}] expected error containing \"{expected_substr}\", but got Ok({out:?})"
                ),
            };
            assert!(
                err.to_string()
                    .to_lowercase()
                    .contains(&expected_substr.to_lowercase()),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared tmpl() parameter tests
// ---------------------------------------------------------------------------

#[test]
fn shared_tmpl_param_tests() {
    let toml_str = include_str!("../../../../tests/shared/tmpl_param_tests.toml");
    let root: toml::Table = toml::from_str(toml_str).expect("parse toml");
    let tests = root
        .get("tests")
        .and_then(|v| v.as_array())
        .expect("tests array");

    for tc_val in tests {
        let tc = tc_val.as_table().expect("test case table");
        let name = tc.get("name").and_then(|v| v.as_str()).unwrap();
        let template_src = get_template_src(tc);
        let params = tc.get("params");

        if let Some(expected) = tc.get("expected_output").and_then(|v| v.as_str()) {
            let tmpl = Template::from_source(&template_src)
                .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            let ctx = toml_to_context(params);
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_substr) = tc.get("expected_error").and_then(|v| v.as_str()) {
            let result = Template::from_source(&template_src).and_then(|tmpl| {
                let ctx = toml_to_context(params);
                tmpl.render_ctx(&ctx)
            });
            let err = result.unwrap_err();
            assert!(
                err.to_string()
                    .to_lowercase()
                    .contains(&expected_substr.to_lowercase()),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared feature E2E tests (Milestone E2E.2)
// ---------------------------------------------------------------------------

#[test]
fn shared_feature_e2e_tests() {
    let toml_str = include_str!("../../../../tests/shared/feature_e2e_tests.toml");
    let root: toml::Table = toml::from_str(toml_str).expect("parse toml");
    let tests = root
        .get("tests")
        .and_then(|v| v.as_array())
        .expect("tests array");

    for tc_val in tests {
        let tc = tc_val.as_table().expect("test case table");
        let name = tc.get("name").and_then(|v| v.as_str()).unwrap();
        let template_src = get_template_src(tc);
        let params = tc.get("params");

        if let Some(expected) = tc.get("expected_output").and_then(|v| v.as_str()) {
            let tmpl = Template::from_source(&template_src)
                .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            let ctx = toml_to_context(params);
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_substr) = tc.get("expected_error").and_then(|v| v.as_str()) {
            let result = Template::from_source(&template_src).and_then(|tmpl| {
                let ctx = toml_to_context(params);
                tmpl.render_ctx(&ctx)
            });
            match result {
                Ok(output) => {
                    panic!(
                        "[{name}] expected error containing \"{expected_substr}\", but template succeeded with output: \"{output}\""
                    );
                }
                Err(err) => {
                    assert!(
                        err.to_string()
                            .to_lowercase()
                            .contains(&expected_substr.to_lowercase()),
                        "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
                    );
                }
            }
        }
    }
}
