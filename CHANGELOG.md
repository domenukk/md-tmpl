# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2026-06-24

### Added

- **`option<T>` type** — first-class "maybe a value" type with `has()` builtin, auto-unwrap,
  and `{% match %}` support. Desugars to `enum<Some(val = T), None>`. Supports `null`/`None`
  in JSON/Python/TypeScript and `Option<T>` in Rust codegen.
- **Type aliases** (`types:` block) — named type definitions reusable across `params:` and `consts:`.
- **Cross-template imports** (`imports:` block) — share type aliases, constants, and implicit
  param types across templates via `stem.Name` dotted paths.
- **Constants** (`consts:` block) — file-scoped constant values, including imported constants.
- **Enum literal expressions** — `kind(Stage.Design)` syntax for accessing enum variants
  as namespace constants, with compile-time validation.
- **Const-reference defaults** — `param = int := MAX_RETRIES` references a constant by name.
- **Scalar lists** — `list<str>`, `list<int>`, `list<float>`, `list<bool>`.
- **`for...else`** — `{% else %}` block inside `{% for %}` renders when the list is empty.
- **`render_unchecked()`** — skip parameter validation for maximum render speed.
- **FlexBuffers serialization** — binary FFI path for Go and Python, avoiding JSON overhead.
- **`serde-wasm-bindgen`** — direct JS object deserialization in the WASM binding.
- **`TemplateCache`** — content-hashed caching with LRU eviction across all language bindings.
- **`defaults_context()`** — pre-filled context with all default parameter values.
- **Filters**: `add(N)`, `sub(N)` arithmetic filters.
- **Functions**: `has(expr)` for option presence checking.
- **Displayability validation** — compile-time rejection of `{{ list }}`, `{{ struct }}`, etc.
- **Collision rules** — comprehensive naming conflict detection at parse time.
- **Go codegen** (`pt-gen-go`) — generates typed Go structs with direct FFI setters.
- **Python import hook** — `from prompt_templates import load_types`.
- **TypeScript `TypedTemplate<P>`** — compile-time type-safe template wrapper.
- **CI**: Full test matrix (Rust stable/MSRV, clippy, fmt, docs, no_std, Python, Go, TS, WASM).

### Changed

- **Blockquote prefix requirement** — standalone `{% %}` tags must use `> {% %}` prefix.
  Mandatory blank lines around standalone tags for clean CommonMark rendering.
- **Performance**: ~2× faster than Tera on simple/loop/conditional/hero benchmarks.
- **Go**: Pure Go FlexBuffers encoder with `sync.Pool` builder reuse.
- **Python**: Migrated to FlexBuffers-based context population.

### Fixed

- FFI `imported_consts` stub now returns actual data from the core library.
- CI WASM test paths corrected to compile TypeScript before running.
- CI clippy job aligned with justfile (excludes macros crate from `--all-features`).

## [0.1.3] — 2026-06-21

### Added

- Enum support with `{% match %}` / `{% case %}` dispatch.
- Unit and struct enum variants with field narrowing.

## [0.1.2] — 2026-06-20

### Added

- Initial release.
- Core template engine with YAML frontmatter, typed parameters.
- `include_template!` and `template!` proc macros.
- TypeScript pure implementation.
- Python PyO3 bindings.
- Go CGo bindings.
- WASM bindings.
