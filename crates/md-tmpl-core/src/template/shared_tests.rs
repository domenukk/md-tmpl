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

/// Check if an error message matches a pipe-separated `expected_error` pattern.
/// `"reserved keyword|shadows built-in"` means the error must contain at
/// least one of the alternatives (case-insensitive).
fn matches_expected_error(message: &str, pattern: &str) -> bool {
    let msg_lower = message.to_lowercase();
    pattern
        .split('|')
        .any(|alt| msg_lower.contains(&alt.to_lowercase()))
}

fn toml_to_value(val: &toml::Value) -> Value {
    match val {
        toml::Value::String(s) => {
            if s == "None" {
                Value::None
            } else if let Some(inner) = s.strip_prefix("Some(").and_then(|r| r.strip_suffix(')')) {
                Value::Str(inner.to_string())
            } else {
                Value::Str(s.clone())
            }
        }
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
                matches_expected_error(&err.to_string(), expected_substr),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared file-based include tests
// ---------------------------------------------------------------------------

/// Resolve a `.tmpl.md` reference to its content by reading from `tests/shared/`.
/// Falls back to using the value as inline content.
fn resolve_shared_file(val: &str) -> String {
    if val.ends_with(".tmpl.md") {
        let full_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/shared")
            .join(val);
        if full_path.exists() {
            return std::fs::read_to_string(&full_path)
                .unwrap_or_else(|e| panic!("failed to read file {}: {e}", full_path.display()));
        }
    }
    val.to_string()
}

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
        let parent_template = resolve_shared_file(
            tc.get("parent_template")
                .and_then(|v| v.as_str())
                .expect("missing parent_template"),
        );
        let files = tc
            .get("files")
            .and_then(|v| v.as_table())
            .expect("missing files table");
        let params = tc.get("params");

        // Create temp dir with include files.
        let dir = tempfile::tempdir().unwrap();
        for (filename, content_val) in files {
            let content =
                resolve_shared_file(content_val.as_str().expect("file content must be string"));
            let file_path = dir.path().join(filename);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(file_path, content).unwrap();
        }

        // Build env pairs from optional [tests.env] table.
        let env_owned: Vec<(String, crate::Value)> = tc
            .get("env")
            .and_then(|v| v.as_table())
            .map(|t| {
                t.iter()
                    .map(|(k, v)| (k.clone(), toml_to_value(v)))
                    .collect()
            })
            // NOLINT: missing [env] table means no env vars — empty vec is correct
            .unwrap_or_default();
        let env_pairs: Vec<(&str, crate::Value)> = env_owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();

        if let Some(expected) = tc.get("expected_output").and_then(|v| v.as_str()) {
            let (tmpl, fm) = Template::compile(
                &parent_template,
                CompileOptions::default()
                    .base_dir(dir.path())
                    .env(&env_pairs),
            )
            .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            // Exercise the compile-time field-type checker (same path as the
            // `template!` proc-macro) so imported-const field access is validated
            // statically, not only at render time.
            if let Some(err) = fm.validate_field_types(&tmpl.segments).into_iter().next() {
                panic!("[{name}] static type check failed: {err}");
            }
            let ctx = toml_to_context(params);
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_substr) = tc.get("expected_error").and_then(|v| v.as_str()) {
            let result = Template::compile(
                &parent_template,
                CompileOptions::default()
                    .base_dir(dir.path())
                    .env(&env_pairs),
            )
            .and_then(|(tmpl, fm)| {
                if let Some(err) = fm.validate_field_types(&tmpl.segments).into_iter().next() {
                    return Err(crate::error::TemplateError::syntax(err));
                }
                let ctx = toml_to_context(params);
                tmpl.render_ctx(&ctx)
            });
            let err = result.unwrap_err();
            assert!(
                matches_expected_error(&err.to_string(), expected_substr),
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
                // Use the same compile-time checker as the `template!` proc-macro
                // so the shared tests genuinely exercise production validation.
                let type_errors = fm.validate_field_types(&tmpl.segments);
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
                    // Use the same compile-time checker as the `template!` proc-macro.
                    let type_errors = fm.validate_field_types(&tmpl.segments);
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
                matches_expected_error(&err.to_string(), expected_substr),
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
                matches_expected_error(&err.to_string(), expected_substr),
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
                        matches_expected_error(&err.to_string(), expected_substr),
                        "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared env: tests (compile-time environment variables)
// ---------------------------------------------------------------------------

#[test]
fn shared_env_tests() {
    let toml_str = include_str!("../../../../tests/shared/env_tests.toml");
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

        // Build env pairs from [tests.env] table.
        let env_owned: Vec<(String, crate::Value)> = tc
            .get("env")
            .and_then(|v| v.as_table())
            .map(|tbl| {
                tbl.iter()
                    .map(|(k, v)| (k.clone(), toml_to_value(v)))
                    .collect()
            })
            // NOLINT: missing [env] table means no env vars — empty vec is correct
            .unwrap_or_default();
        let env_pairs: Vec<(&str, crate::Value)> = env_owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();

        if let Some(expected) = tc.get("expected_output").and_then(|v| v.as_str()) {
            let (tmpl, _fm) =
                Template::compile(&template_src, CompileOptions::default().env(&env_pairs))
                    .unwrap_or_else(|e| panic!("[{name}] compile failed: {e}"));
            let ctx = toml_to_context(params);
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_substr) = tc.get("expected_error").and_then(|v| v.as_str()) {
            let result =
                Template::compile(&template_src, CompileOptions::default().env(&env_pairs))
                    .and_then(|(tmpl, _fm)| {
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
                        matches_expected_error(&err.to_string(), expected_substr),
                        "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
                    );
                }
            }
        }
    }
}
