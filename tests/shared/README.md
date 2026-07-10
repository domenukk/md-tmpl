# E2E Test Infra: prompt-templates

This document defines the authoritative end-to-end (E2E) test infrastructure, architecture, feature inventory, and verification methodology for the cross-language `prompt-templates` template engine. It serves as the specification for ensuring behavioral and diagnostic parity across all language bindings (Rust, TypeScript, Python, and WASM).

---

## Test Philosophy

The testing infrastructure for `prompt-templates` is built upon two core foundational pillars:

1. **Opaque-Box, Requirement-Driven Testing**: All E2E tests evaluate the template engine from the perspective of an external consumer using the public rendering interface. Tests do not rely on internal AST structures, language-specific memory layouts, or private engine APIs. By defining tests strictly against functional requirements and public behaviors, the suite guarantees absolute parity across Rust, TypeScript, Python, and WebAssembly implementations.
2. **Systematic Test Methodology**:
   - **Category-Partition**: Input domains, frontmatter parameter definitions, and template control structures are systematically partitioned into distinct equivalence classes (e.g., default list parameters vs. explicit parameter overrides; valid string membership vs. type mismatch errors).
   - **Boundary Value Analysis (BVA)**: Exhaustive verification of boundary conditions, including empty lists (`[]`), empty strings (`""`), missing optional parameters, boundary line/column numbers in error diagnostics, and maximum nesting depths for control tags and include statements.
   - **Pairwise Testing**: Combinatorial test generation designed to exercise all pairs of parameter types, operators (`in` / `not in`), default override states, and error handling branches without combinatorial explosion.
   - **Workload Testing**: Evaluating end-to-end system reliability, execution safety, and error recovery under realistic multi-template rendering workflows, large parameter dictionaries, and deeply nested evaluation graphs.

---

## Feature Inventory

The following table catalogs the core feature areas covered by the E2E test infrastructure, tracing each to its originating requirement and specifying test density commitments across all four testing tiers.

| #   | Feature                                                        | Source (requirement) | Tier 1 | Tier 2 | Tier 3 | Tier 4 |
| --- | -------------------------------------------------------------- | -------------------- | ------ | ------ | ------ | ------ |
| 1   | List parameter default values (`[...]`)                        | ORIGINAL_REQUEST §R1 | 5      | 5      | ✓      | ✓      |
| 2   | `in` / `not in` operators (list membership & string substring) | ORIGINAL_REQUEST §R2 | 5      | 5      | ✓      | ✓      |
| 3   | Syntax error diagnostics (`line`, `column`, `snippet`)         | ORIGINAL_REQUEST §R1 | 5      | 5      | ✓      | ✓      |
| 4   | `{% panic(...) %}` statement                                   | ORIGINAL_REQUEST §R3 | 5      | 5      | ✓      | ✓      |
| 5   | Boolean expressions (`&&`, `\|\|`, `!`, grouping)              | SPEC §Conditions     | 10     | 10     | ✓      | —      |
| 6   | String interpolation in conditions (`"{{ expr }}"`)            | SPEC §Conditions     | 5      | 5      | —      | —      |
| 7   | Option types (`option(T)`, `has()`, `None`/`Some`, for+match)  | SPEC §Types          | 10     | 5      | ✓      | —      |
| 8   | Filters (`upper`, `lower`, `trim`, `fixed`, `join`, `limit`)   | SPEC §Filters        | 7      | 4      | ✓      | —      |
| 9   | Built-in functions (`kind()`, `kinds()`)                       | SPEC §Functions      | 4      | 3      | —      | —      |
| 10  | For loop edge cases (`idx()` nested, empty list)               | SPEC §ForLoops       | 2      | —      | —      | —      |
| 11  | Raw blocks and comments (`{% raw %}`, `{# ... #}`)             | SPEC §Raw/Comments   | 2      | —      | —      | —      |
| 12  | Imports in included files (enum types, consts, `match`/`case`) | SPEC §Includes       | 9      | —      | ✓      | —      |

### Tier Definitions

- **Tier 1 (Basic Functionality)**: Positive and negative unit tests verifying core syntax parsing, default parameter evaluation, membership operator evaluation, and basic error reporting.
- **Tier 2 (Edge Cases & BVA)**: Stress testing boundary conditions, empty containers, malformed syntax, type mismatches, and nested expression evaluation.
- **Tier 3 (Pairwise Coverage)**: Multi-parameter combinations, complex boolean conditionals combining `in`/`not in` with logical operators, and parameter override interactions.
- **Tier 4 (Real-World Scenarios)**: Complex application-level templates simulating production workflows (e.g., access control, configuration generation, automated prompting).

---

## Test Architecture

The E2E test suite utilizes a unified, language-agnostic TOML fixture architecture located in `tests/shared/`. A single TOML definition acts as the definitive source of truth across all backend implementations.

### Runner Locations & Language Integration

- **Rust (`crates/md-tmpl/src/template/shared_tests.rs`)**:
  - **Integration**: Attached via `mod shared_tests;` in `crates/md-tmpl/src/template/mod.rs` and executed via `cargo test -p md-tmpl --lib shared_tests`.
  - **Mechanism**: Uses `include_str!("../../../../tests/shared/*.toml")` at compile time with `toml::from_str`. It evaluates inline `template` strings via `Template::from_source_with_base_dir` and external references via `Template::from_file`, converting `"params"` via `params.to_context()` and rendering via `tmpl.render_ctx(&ctx)`.
- **TypeScript (`crates/md-tmpl-typescript/src/tests/shared_tests.test.ts`)**:
  - **Integration**: Executed via `npm test` / `node --test` using standard `node:test` (`describe`, `it`) blocks.
  - **Mechanism**: Reads fixtures from `tests/shared/` using `fs.readFileSync` and `smol-toml` (TOML v1.0). It resolves templates via `Template.fromSourceWithBaseDir` or `Template.fromFile`, casting parameters and verifying rendering results or caught error substrings.
- **Python & WASM Bindings**:
  - **Current State**: Python unit tests reside in `crates/md-tmpl-python/python/tests/test_md_tmpl.py` and WASM tests in `crates/md-tmpl-wasm/tests/wasm.test.ts`.
  - **E2E Integration Plan**: Both Python and WASM bindings will execute these exact same shared TOML fixtures once shared test harnesses (`test_shared.py` loading `tomllib` via `pytest`, and `shared.test.ts` using `node:test`) are added to their test suites. Because the TOML schema requires only standard dictionary passing and substring error matching, no changes to the fixtures will be required when these harnesses come online.

### TOML Fixture Format

Each shared test fixture file consists of a sequence of `[[tests]]` table entries:

```toml
[[tests]]
name = "unique_test_identifier"
description = "Clear explanation of the expected behavior or verification goal"
template = '''
---
params: [param_name = str, items = list(str) := ["default"]]
---
> {{ param_name }}: {% for item in items %}{{ item }} {% /for %}'''
expected_output = "runtime_value: rust ts "

[tests.params]
param_name = "runtime_value"
items = ["rust", "ts"]
```

#### Schema Specification

- **`name`** _(string, required)_: A unique identifier for the test case across the test suite.
- **`description`** _(string, required)_: Explains what specific requirement, operator, or boundary condition is being verified.
- **`params`** _(table, optional)_: Key-value dictionary representing runtime variables passed into the template evaluation scope. When testing frontmatter parameter defaults (`:=`), omitting a key from this table forces the engine to evaluate the declared default.
  - **Option Value Convention**: TOML has no null literal, so test runners interpret string values as follows:
    - `"None"` → null / `Value::None` — represents an absent option value.
    - `"Some(x)"` → the literal string `x` — escape hatch when you need the string `"None"` as an actual value.
    - All other strings → passed through unchanged.
    - Non-string TOML values (integers, booleans, arrays, tables) → unaffected.
    - This transformation is applied recursively to arrays and nested tables.
- **Template Definition** _(exactly one required)_:
  - **`template`** _(string)_: Inline template source code defined as a multiline literal TOML string (`'''`), or a relative file path from `tests/shared/` to an external `.tmpl.md` template file (e.g., `"templates/inline_tmpl/basic.tmpl.md"`).
  - **`parent_template`** _(optional)_: Used in inheritance and include tests to specify calling templates alongside child templates.
  - **`files`** _(table, optional)_: In include tests, maps relative filenames to their template contents or file paths.
- **Outcome Assertion** _(mutually exclusive, exactly one required)_:
  - **`expected_output`** _(string)_: The exact expected string produced after successful compilation and rendering. Multiline expected outputs use multiline literal strings (`'''`).
  - **`expected_error`** _(string)_: A substring expected to match against the stringified compilation error or runtime exception. This verifies syntax diagnostics (`"line 5"`, `"--> snippet"`) and runtime panic messages (`"template panic: fatal error"`).

### Directory Layout

```text
tests/shared/
├── inline_tmpl_tests.toml        # Core syntax, variables, filters, and control flow
├── inline_control_tests.toml     # Advanced conditionals, loops, and block scoping
├── tmpl_param_tests.toml         # Frontmatter parameter declarations and typing
├── include_tests.toml            # Template inclusion and inheritance structures
└── templates/                    # External template file assets referenced by "template" path
    ├── include/
    ├── inline_control/
    ├── inline_tmpl/
    └── tmpl_param/
```

---

## Real-World Application Scenarios (Tier 4)

To ensure robust performance under complex operational workloads, the E2E infrastructure includes high-complexity real-world application scenarios that exercise multiple core features concurrently.

| #   | Scenario                                                            | Features Exercised                                                             | Complexity |
| --- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------ | ---------- |
| 1   | RBAC access control template with list defaults and `in` operator   | List defaults (`[...]`), `in` operator                                         | Medium     |
| 2   | Dynamic SQL/config generator with safety `panic`                    | `{% panic(...) %}`, `not in` operator                                          | High       |
| 3   | Template compilation diagnostic reporter                            | Syntax error diagnostics (`line`, `column`, `snippet`), `{% panic(...) %}`     | Medium     |
| 4   | Multi-recipient email template with conditional inclusion           | List defaults (`[...]`), `in` / `not in` operators                             | Medium     |
| 5   | Automated prompt builder with default fallback lists and validation | List defaults (`[...]`), `in` operator, `{% panic(...) %}`, syntax diagnostics | High       |

### Scenario Descriptions & Verification Goals

1. **RBAC Access Control Template**: Simulates generating security policy documents where user roles are checked against authorized lists using `{% if user_role in allowed_roles %}`. Utilizes default list parameter values (`allowed_roles = list(str) := ["admin", "auditor"]`) when explicit role configurations are omitted.
2. **Dynamic SQL/Config Generator**: Builds complex database query strings or system configuration files from user-defined dictionaries. Uses `{% if ";" in input_val or "--" in input_val %}{% panic("SQL injection pattern detected") %}{% /if %}` alongside `not in` validation to safeguard against malformed or malicious configurations.
3. **Template Compilation Diagnostic Reporter**: Exercises IDE and CI integration workflows by deliberately submitting malformed templates (unclosed tags, invalid syntax, missing panic arguments). Verifies that the engine emits precise line numbers, column approximations, and source snippets (`-->`) across all language runners.
4. **Multi-Recipient Email Notification Template**: Renders customized email bodies for user groups. Employs frontmatter default lists for standard notification tiers (`list(str) := ["general", "updates"]`) and uses `in` / `not in` operators to conditionally include unsubscribe links or confidential executive summaries.
5. **Automated Prompt Builder with Validation**: Simulates an LLM agent prompt construction pipeline. Declares default fallback tool lists (`list(str) := ["code_search", "view_file"]`), validates selected models using `in`, evaluates string substring constraints on system instructions, and executes `{% panic(...) %}` if mandatory context blocks or required safety constraints are violated.

---

## Coverage Thresholds

All code changes, feature additions, and language binding implementations must strictly satisfy the following minimum test coverage thresholds before merge:

- **Tier 1 (Basic Functionality)**: Minimum **≥5 test cases per feature** (totaling at least 20 test cases across the 4 core features) validating standard execution paths and positive assertions.
- **Tier 2 (Edge Cases & BVA)**: Minimum **≥5 test cases per feature** (totaling at least 20 test cases) targeting boundary values, empty lists/strings, type coercion failures, and syntax error triggers.
- **Tier 3 (Pairwise & Combinatorial)**: **100% pairwise coverage** of parameter data types (`str`, `int`, `bool`, `list(str)`), membership operators (`in`, `not in`), default parameter override states, and error handling paths.
- **Tier 4 (Real-World Application Scenarios)**: Minimum **≥5 realistic end-to-end application scenarios** simulating multi-feature operational workloads with complete cross-language execution verification.
