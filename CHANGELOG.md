# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-06-25

### Added

- TypeScript language binding package (`prompt-templates`) via Node API.
- TypeScript CLI compiler (`prompt-templates-cli`) for emitting strongly typed template interfaces.
- WebAssembly module (`prompt-templates-wasm`) targeting Node.js and the Browser.
- Pre-compiled WASM package.
- `renderJson` capability across language bindings for unified multi-language LLM pipelines.
- Multi-dimensional caching API in Python, Go, and TypeScript bindings to speed up template rendering via LRU caches.
- Native `option<T>` type support mapped correctly to native optionals in all supported languages.
- Complete set of benchmark suites directly comparing `prompt-templates` with Handlebars, Mustache, Mako, Django, text/template, Jinja2, Tera, and MiniJinja.

### Changed

- Total internal engine refactoring to modularize components into separate files (`template`, `frontmatter/params`, `ffi`, `scope`, `serde_support`).
- Revamped FFI binding boundary with `RenderOptions` structure replacing isolated `render_ctx` and `render_ctx_allowing_extra` functions for future extensibility.
- Internal string parsing and parameter evaluation mechanisms strictly decoupled from language bindings.
- JSON Null evaluation explicitly returns a string type to resolve FFI boundary panic scenarios.

### Fixed

- Addressed all outstanding `clippy` pedantic and standard lint violations across the workspace.
- `no_std` builds fixed by handling `spin::Lazy` deprecation warnings, switching to `spin::LazyLock`.
- Deprecated dependency upgrades and replacements applied to `Cargo.toml`.

## [0.1.4] - 2026-06-10

### Changed

- Minor bugfixes and optimizations to Python macro generation.

## [0.1.3] - 2026-06-10

### Added

- Go language bindings integration (`prompt_templates`).
- Support for complex recursive structs.
- Filter functions: `upper`, `lower`, `trim`, `fixed`, `join`, `limit`, `add`, `sub`.

## [0.1.2] - 2026-06-10

### Added

- Initial release of `prompt-templates` core engine.
- Rust macro system (`prompt-templates-macros`).
- Python native extension (`prompt-templates-python`).
- Built-in conditionals, loops, nested fields.
- High-performance zero-allocation template renderer.
