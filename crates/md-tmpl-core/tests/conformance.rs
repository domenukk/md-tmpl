// Cross-language conformance harness (Rust side).
//
// Replays the shared JSON corpus in `<repo>/conformance` through the Rust
// `md-tmpl-core` engine and asserts that every case matches the recorded
// expectation. Those expectations were derived by executing the TypeScript
// reference implementation, and the exact same corpus is replayed by the
// TypeScript harness (`crates/md-tmpl-typescript/src/tests/conformance.test.ts`).
// If both harnesses pass, the two backends are behaviourally identical on the
// covered surface.
//
// The corpus is parsed with `serde_yaml` (already a dev-dependency). JSON is a
// strict subset of YAML, so no additional dependency is needed.

use std::path::{Path, PathBuf};

use md_tmpl_core::{CompileOptions, Template, Value};
use serde::Deserialize;
use serde_yaml::Value as Yaml;

// Every corpus file, regardless of category, holds a flat list of `Case`s whose
// `expect.kind` selects how they are checked.
const CORPUS_FILES: &[&str] = &[
    "render.json",
    "interpolation.json",
    "frontmatter.json",
    "errors.json",
    "escapes.json",
    "comments.json",
];

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    #[serde(default)]
    params: Yaml,
    #[serde(default)]
    env: Option<Yaml>,
    expect: Expect,
}

#[derive(Deserialize)]
struct Expect {
    kind: String,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    defaults: Option<Yaml>,
    #[serde(default)]
    phase: Option<String>,
    #[serde(default)]
    error_contains: Option<String>,
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

fn load(file: &str) -> Vec<Case> {
    let path = corpus_dir().join(file);
    let txt = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read corpus file {}: {e}", path.display()));
    serde_yaml::from_str(&txt)
        .unwrap_or_else(|e| panic!("parse corpus file {}: {e}", path.display()))
}

// Compile a case, threading through the optional string-only environment used by
// env-substitution cases. Errors are flattened to `String` for uniform matching.
fn compile(source: &str, env: Option<&Yaml>) -> Result<Template, String> {
    match env {
        None => Template::from_source(source).map_err(|e| e.to_string()),
        Some(env_yaml) => {
            let mapping = env_yaml.as_mapping().expect("env must be a mapping");
            let owned: Vec<(String, Value)> = mapping
                .iter()
                .map(|(k, v)| {
                    let key = k.as_str().expect("env key must be a string").to_owned();
                    let val = v.as_str().expect("env value must be a string").to_owned();
                    (key, Value::Str(val))
                })
                .collect();
            let refs: Vec<(&str, Value)> =
                owned.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            Template::compile(source, CompileOptions::default().env(&refs))
                .map(|(t, _fm)| t)
                .map_err(|e| e.to_string())
        }
    }
}

// Project an engine `Value` into the corpus's native JSON/YAML shape so it can
// be compared structurally against the recorded `defaults`.
fn value_to_yaml(v: &Value) -> Yaml {
    match v {
        Value::Str(s) => Yaml::String(s.clone()),
        Value::Bool(b) => Yaml::Bool(*b),
        Value::Int(i) => Yaml::Number(serde_yaml::Number::from(*i)),
        Value::Float(f) => Yaml::Number(serde_yaml::Number::from(*f)),
        Value::List(items) => Yaml::Sequence(items.iter().map(value_to_yaml).collect()),
        Value::Struct(map) => {
            let mut m = serde_yaml::Mapping::new();
            for (k, val) in map.iter() {
                m.insert(Yaml::String(k.clone()), value_to_yaml(val));
            }
            Yaml::Mapping(m)
        }
        Value::Tmpl(_) => Yaml::String("<tmpl>".to_owned()),
        Value::None => Yaml::Null,
    }
}

fn check_render(file: &str, case: &Case, fails: &mut Vec<String>) {
    let name = &case.name;
    let want = case
        .expect
        .output
        .as_deref()
        .expect("render case needs expect.output");
    let tmpl = match compile(&case.source, case.env.as_ref()) {
        Ok(t) => t,
        Err(e) => {
            fails.push(format!("[{file}] {name}: Rust COMPILE error: {e}"));
            return;
        }
    };
    match tmpl.render(&case.params) {
        Ok(got) => {
            if got != want {
                fails.push(format!(
                    "[{file}] {name}: render mismatch\n    want: {want:?}\n    got : {got:?}"
                ));
            }
        }
        Err(e) => fails.push(format!("[{file}] {name}: Rust RENDER error: {e}")),
    }
}

fn check_default(file: &str, case: &Case, fails: &mut Vec<String>) {
    let name = &case.name;
    let want = case
        .expect
        .defaults
        .as_ref()
        .expect("default case needs expect.defaults");
    let tmpl = match compile(&case.source, None) {
        Ok(t) => t,
        Err(e) => {
            fails.push(format!("[{file}] {name}: Rust COMPILE error: {e}"));
            return;
        }
    };
    let mut m = serde_yaml::Mapping::new();
    for (k, v) in &tmpl.defaults() {
        m.insert(Yaml::String(k.clone()), value_to_yaml(v));
    }
    let got = Yaml::Mapping(m);
    if &got != want {
        fails.push(format!(
            "[{file}] {name}: defaults mismatch\n    want: {want:?}\n    got : {got:?}"
        ));
    }
}

// Assert `haystack` contains `needle` when a needle is recorded; a missing
// needle means "any error is acceptable" (used by phase-agnostic cases).
fn check_needle(
    file: &str,
    name: &str,
    kind: &str,
    needle: Option<&str>,
    haystack: &str,
    fails: &mut Vec<String>,
) {
    if let Some(n) = needle {
        if !haystack.contains(n) {
            fails.push(format!(
                "[{file}] {name}: Rust {kind} error {haystack:?} lacks substring {n:?}"
            ));
        }
    }
}

fn check_error(file: &str, case: &Case, fails: &mut Vec<String>) {
    let name = &case.name;
    let phase = case
        .expect
        .phase
        .as_deref()
        .expect("error case needs phase");
    let needle = case.expect.error_contains.as_deref();
    let compiled = compile(&case.source, None);
    match phase {
        "compile" => match compiled {
            Ok(_) => fails.push(format!(
                "[{file}] {name}: expected COMPILE error but Rust compiled OK"
            )),
            Err(e) => check_needle(file, name, "compile", needle, &e, fails),
        },
        "render" => match compiled {
            Err(e) => fails.push(format!(
                "[{file}] {name}: expected RENDER error but Rust failed at COMPILE: {e}"
            )),
            Ok(tmpl) => match tmpl.render(&case.params) {
                Ok(_) => fails.push(format!(
                    "[{file}] {name}: expected RENDER error but Rust rendered OK"
                )),
                Err(e) => check_needle(file, name, "render", needle, &e.to_string(), fails),
            },
        },
        // Phase-agnostic leak safety: Rust must error in EITHER phase. The phase
        // itself is allowed to differ from TS (Rust tends to reject at compile,
        // TS at render); only the presence of an error is required.
        "any" => match compiled {
            Err(e) => check_needle(file, name, "compile", needle, &e, fails),
            Ok(tmpl) => match tmpl.render(&case.params) {
                Ok(_) => fails.push(format!(
                    "[{file}] {name}: expected an error in either phase but Rust succeeded in both"
                )),
                Err(e) => check_needle(file, name, "render", needle, &e.to_string(), fails),
            },
        },
        other => fails.push(format!("[{file}] {name}: bad phase {other}")),
    }
}

#[test]
fn conformance_corpus_matches_rust_backend() {
    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for file in CORPUS_FILES {
        for case in load(file) {
            checked += 1;
            match case.expect.kind.as_str() {
                "render" => check_render(file, &case, &mut fails),
                "default" => check_default(file, &case, &mut fails),
                "error" => check_error(file, &case, &mut fails),
                other => fails.push(format!(
                    "[{file}] {}: unknown expect.kind {other}",
                    case.name
                )),
            }
        }
    }

    assert!(
        checked > 0,
        "conformance corpus was empty — is {:?} populated?",
        corpus_dir()
    );
    assert!(
        fails.is_empty(),
        "Rust backend diverges from the shared conformance corpus on {} case(s):\n{}",
        fails.len(),
        fails.join("\n")
    );
}
