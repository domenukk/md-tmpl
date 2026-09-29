# Agent Development Guidelines (`md-tmpl`)

This document defines the architectural, coding, specification, and testing standards for `md-tmpl`. All human and autonomous contributors must adhere to these rules without exception.

---

## 1. Specification & Cross-Language Parity (`SPEC.md` + All Backends/Frontends)

`md-tmpl` maintains two independent core implementations (**Rust** and **pure TypeScript**) and five first-class language targets (**Rust**, **TypeScript**, **WASM**, **Python**, and **Go**).

- **Every behavioral or API change MUST be reflected in [`SPEC.md`](SPEC.md):**
  - Any new syntax, type rule, built-in function, filter, whitespace rule, error diagnostic, or programmatic API primitive must be documented accurately in `SPEC.md` (and in the relevant language `README.md` files) in the same change.
  - Keep `SPEC.md` and `README.md` prose concise, technical, and reference-style — zero marketing buzzwords or filler.
- **Every feature and edge case MUST be implemented and tested across all languages:**
  1. **Shared Conformance Suite (`tests/conformance/*.toml`, `tests/shared/*.toml`):** Add positive and negative test cases so all engines and bindings validate identical inputs and outputs.
  2. **Rust (`crates/md-tmpl-core`, `crates/md-tmpl-macros`, `crates/md-tmpl`, `crates/md-tmpl-ffi`, `crates/md-tmpl-compile-tests`):** Runtime rendering, `no_std` + `alloc` compatibility, compile-time proc-macro (`include_template!`, `template!`) codegen, and C FFI (`pt_*`).
  3. **TypeScript (`crates/md-tmpl-typescript`):** Pure-TypeScript parser, validator, AST compiler, direct renderer, and type generator (`generateTypes`).
  4. **WebAssembly (`crates/md-tmpl-wasm`):** `wasm-bindgen` bindings and `ITemplate` parity with pure TypeScript.
  5. **Python (`crates/md-tmpl-python`):** PyO3 native extension (`_native`), Python package (`md_tmpl`), `.pyi` type stubs, and `pytest` suite.
  6. **Go (`go/md_tmpl`, `go/cmd/pt-gen-go`):** cgo bindings over `md-tmpl-ffi` and `go test` suite.

---

## 2. Code Hygiene & Structural Limits

- **1,200-Line Hard Cap:**
  - Every source file must remain strictly below **1,200 lines** (enforced by `just lint-hygiene` / `python3 scripts/lint_hygiene.py`).
  - Split growing modules into focused submodules before they approach the limit.
- **DRY & Single Source of Truth:**
  - Reuse shared parsing, validation, escaping (`escape_xml_str`, `escape_json_str`), and sanitization (`TOKEN_DELIMITERS`, `ROLE_TOKEN_DELIMITERS`, `sanitize_tokens_str`, `sanitize_untrusted_str`, `quarantine_str`, `quarantine_untrusted_str`) primitives from `md-tmpl-core` across all Rust crates (`md-tmpl`, `md-tmpl-macros`, `md-tmpl-ffi`, `md-tmpl-wasm`, `md-tmpl-python`) instead of duplicating logic or tables.
- **No Inline Magic Strings or Magic Numbers:**
  - Never scatter inline string literals (such as ad-hoc `.replace('\\', "\\\\").replace('"', "\\\"")`, sentinel keys, error kind tags, or repeated FFI fallback messages) or magic numbers across functions.
  - Define named `const` items, strong types, and exhaustive `enum`s in a single canonical location and reference them everywhere.
- **Multiline Template Strings:**
  - Always use proper multiline strings (`r#"..."#` in Rust, `` `...` `` in TypeScript/Go, `"""..."""` in Python/TOML) for `.tmpl.md` template sources — never single-line `\n`-escaped strings (`"---\nparams:\n..."`).
- **TypeScript Only for Scripts/Tooling in JS Ecosystem:**
  - Never add `.js` / `.mjs` / `.cjs` source files; write all Node/web tests, benchmarks, and helpers in strict TypeScript (`.ts`).

---

## 3. Error Handling & Lint Discipline

- **Never Ignore Errors or Return Values:**
  - `let _ = ...` on fallible results, silent `.ok()`, `.unwrap_or_default()` that hides parse/IO failures, `if let Ok(...)` that silently drops `Err`, `Err(_)` / `Err(_err)` arms that discard error context, and empty `catch {}` blocks are forbidden.
  - Always propagate errors with `?` / `Result`, or convert them with full error context.
  - Never use silent fallback paths for malformed configuration or environment variables (e.g., `MD_TMPL_MAX_INCLUDE_DEPTH` or `CARGO_MANIFEST_DIR` must fail loudly when malformed).
- **Never Suppress Lints:**
  - Do **not** add `#[allow(...)]`, `#[expect(...)]` (except where strictly required by PyO3/FFI macro expansion), `// NOLINT`, `@ts-ignore`, `@ts-expect-error`, or `eslint-disable`.
  - Never suppress `clippy::too_many_lines`, `clippy::type_complexity`, or `clippy::cognitive_complexity` — refactor the code instead.
- **Never Ignore or Skip Tests:**
  - `#[ignore]`, `pytest.mark.skip`, `pytest.mark.xfail`, `t.Skip`, `describe.skip`, `it.skip`, `test.skip`, and `.only`/`.todo` are forbidden and blocked by `scripts/lint_hygiene.py`.

---

## 4. Resource-Safe Verification Gates

Before finishing any task, run the full formatting, lint, hygiene, documentation, and cross-language test suite (serializing heavy Cargo builds with `flock` on shared hosts):

```bash
# 1. Code formatting (Rust, TypeScript, Python, Go, Markdown)
just fmt-check

# 2. Hygiene & lint gates (Clippy -D warnings, tsc --noEmit, ruff, mypy, go vet, lint_hygiene.py)
flock /tmp/jetski-build.lock just lint

# 3. Rustdoc warnings gate
flock /tmp/jetski-build.lock just doc

# 4. Full cross-language test suite (Rust + no_std + TypeScript + WASM + Python + Go)
flock /tmp/jetski-build.lock just test

# 5. Feature matrix check (no_std / alloc / serde / macros combinations)
flock /tmp/jetski-build.lock ./ci/check_feature_matrix.sh
```
