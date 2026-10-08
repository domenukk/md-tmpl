//! Shared cross-backend test runner.
//!
//! Reads test case definitions from `tests/shared/` — the single-file suites
//! (`inline_tmpl_tests.toml`, `tmpl_param_tests.toml`, `env_tests.toml`) and
//! the per-topic suite directories (`include_tests/`, `inline_control_tests/`,
//! `feature_e2e_tests/`) — and runs them against the Rust template engine.
//!
//! These same fixtures are consumed by the TypeScript backend's test runner,
//! ensuring behavioral parity between implementations.
//!
//! Templates use `template` / `parent_template` (proper multiline TOML strings)
//! so the fixtures stay readable and literal.

use std::path::PathBuf;

use crate::{CompileOptions, Context, Template, Value};

/// Shared fixture root, relative to this crate's manifest directory.
const SHARED_DIR: &str = "../../tests/shared";

/// File extension of shared fixture files.
const FIXTURE_EXTENSION: &str = "toml";

/// A shared fixture file embedded at compile time.
struct SharedFixture {
    /// Path relative to `tests/shared/`.
    path: &'static str,
    /// Embedded TOML source.
    source: &'static str,
}

/// Embed a shared fixture file (path relative to `tests/shared/`).
macro_rules! shared_fixture {
    ($path:literal) => {
        SharedFixture {
            path: $path,
            source: include_str!(concat!("../../../../tests/shared/", $path)),
        }
    };
}

const INLINE_TMPL_FIXTURES: &[SharedFixture] = &[shared_fixture!("inline_tmpl_tests.toml")];

const TMPL_PARAM_FIXTURES: &[SharedFixture] = &[shared_fixture!("tmpl_param_tests.toml")];

const ENV_FIXTURES: &[SharedFixture] = &[shared_fixture!("env_tests.toml")];

/// Sorted contents of `tests/shared/include_tests/`.
const INCLUDE_FIXTURES: &[SharedFixture] = &[
    shared_fixture!("include_tests/basics_and_path_interpolation.toml"),
    shared_fixture!("include_tests/chained_imports.toml"),
    shared_fixture!("include_tests/circular_and_mutual.toml"),
    shared_fixture!("include_tests/cross_directory.toml"),
    shared_fixture!("include_tests/env_paths.toml"),
    shared_fixture!("include_tests/import_usage_tracking.toml"),
    shared_fixture!("include_tests/imported_const_iteration.toml"),
    shared_fixture!("include_tests/included_file_imports.toml"),
    shared_fixture!("include_tests/inline_tmpl_scoping.toml"),
    shared_fixture!("include_tests/param_paths.toml"),
    shared_fixture!("include_tests/whitespace.toml"),
    shared_fixture!("include_tests/with_args.toml"),
];

/// Sorted contents of `tests/shared/inline_control_tests/`.
const INLINE_CONTROL_FIXTURES: &[SharedFixture] = &[
    shared_fixture!("inline_control_tests/conditionals_loops_match.toml"),
    shared_fixture!("inline_control_tests/local_const_iteration.toml"),
    shared_fixture!("inline_control_tests/match_type_safety.toml"),
    shared_fixture!("inline_control_tests/string_literal_escapes.toml"),
    shared_fixture!("inline_control_tests/whitespace_trim.toml"),
];

/// Sorted contents of `tests/shared/feature_e2e_tests/`.
const FEATURE_E2E_FIXTURES: &[SharedFixture] = &[
    shared_fixture!("feature_e2e_tests/boolean_expressions.toml"),
    shared_fixture!("feature_e2e_tests/builtins_loops_raw_string_match.toml"),
    shared_fixture!("feature_e2e_tests/enums.toml"),
    shared_fixture!("feature_e2e_tests/filters_whitespace.toml"),
    shared_fixture!("feature_e2e_tests/frontmatter_and_validation.toml"),
    shared_fixture!("feature_e2e_tests/frontmatter_regressions.toml"),
    shared_fixture!("feature_e2e_tests/list_defaults_membership_diagnostics_panic.toml"),
    shared_fixture!("feature_e2e_tests/literals_truthiness.toml"),
    shared_fixture!("feature_e2e_tests/nested_type_patterns_blockquotes.toml"),
    shared_fixture!("feature_e2e_tests/option_types.toml"),
    shared_fixture!("feature_e2e_tests/reserved_names_keywords.toml"),
    shared_fixture!("feature_e2e_tests/strict_builtin_typing.toml"),
    shared_fixture!("feature_e2e_tests/string_interpolation_field_access.toml"),
    shared_fixture!("feature_e2e_tests/unused_params.toml"),
];

/// Absolute path of the shared fixture root.
fn shared_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SHARED_DIR)
}

/// Assert that every suite directory referenced by `fixtures` lists exactly the
/// embedded files, so a newly added fixture file can never be silently skipped.
fn assert_fixture_dirs_complete(fixtures: &[SharedFixture]) {
    let dirs: std::collections::BTreeSet<&str> = fixtures
        .iter()
        .filter_map(|f| f.path.rsplit_once('/').map(|(dir, _)| dir))
        .collect();
    for dir in dirs {
        let dir_path = shared_dir().join(dir);
        let mut listed: Vec<String> = std::fs::read_dir(&dir_path)
            .unwrap_or_else(|e| panic!("failed to list {}: {e}", dir_path.display()))
            .map(|entry| {
                entry
                    .unwrap_or_else(|e| {
                        panic!("failed to read entry in {}: {e}", dir_path.display())
                    })
                    .path()
            })
            .filter(|p| p.extension().is_some_and(|ext| ext == FIXTURE_EXTENSION))
            .map(|p| {
                let file = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .expect("UTF-8 file name");
                format!("{dir}/{file}")
            })
            .collect();
        listed.sort();
        let declared: Vec<&str> = fixtures
            .iter()
            .map(|f| f.path)
            .filter(|p| p.rsplit_once('/').is_some_and(|(d, _)| d == dir))
            .collect();
        assert_eq!(
            listed, declared,
            "embedded fixture list is out of sync with tests/shared/{dir}/"
        );
    }
}

/// Parse every fixture and return all test cases in order. Panics if any
/// fixture is malformed or declares no test cases.
fn load_cases(fixtures: &[SharedFixture]) -> Vec<toml::Value> {
    assert!(!fixtures.is_empty(), "no shared fixtures declared");
    assert_fixture_dirs_complete(fixtures);
    fixtures
        .iter()
        .flat_map(|fixture| {
            let root: toml::Table = toml::from_str(fixture.source)
                .unwrap_or_else(|e| panic!("failed to parse {}: {e}", fixture.path));
            let tests = root
                .get("tests")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| panic!("{}: missing tests array", fixture.path))
                .clone();
            assert!(!tests.is_empty(), "{}: tests array is empty", fixture.path);
            tests
        })
        .collect()
}

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
        let full_path = shared_dir().join(val);
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
    for tc_val in &load_cases(INLINE_TMPL_FIXTURES) {
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
        let full_path = shared_dir().join(val);
        if full_path.exists() {
            return std::fs::read_to_string(&full_path)
                .unwrap_or_else(|e| panic!("failed to read file {}: {e}", full_path.display()));
        }
    }
    val.to_string()
}

#[test]
fn shared_include_tests() {
    for tc_val in &load_cases(INCLUDE_FIXTURES) {
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
            .map_or_else(Vec::new, |t| {
                t.iter()
                    .map(|(k, v)| (k.clone(), toml_to_value(v)))
                    .collect()
            });
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
    for tc_val in &load_cases(INLINE_CONTROL_FIXTURES) {
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
    for tc_val in &load_cases(TMPL_PARAM_FIXTURES) {
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
    for tc_val in &load_cases(FEATURE_E2E_FIXTURES) {
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
    for tc_val in &load_cases(ENV_FIXTURES) {
        let tc = tc_val.as_table().expect("test case table");
        let name = tc.get("name").and_then(|v| v.as_str()).unwrap();
        let template_src = get_template_src(tc);
        let params = tc.get("params");

        // Build env pairs from [tests.env] table.
        let env_owned: Vec<(String, crate::Value)> = tc
            .get("env")
            .and_then(|v| v.as_table())
            .map_or_else(Vec::new, |tbl| {
                tbl.iter()
                    .map(|(k, v)| (k.clone(), toml_to_value(v)))
                    .collect()
            });
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
