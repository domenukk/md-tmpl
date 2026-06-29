# Contributing to prompt-templates

First off, thank you for considering contributing to `prompt-templates`! We welcome contributions from everyone.

## Development Setup

`prompt-templates` is a cross-language project built around a core Rust engine.

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable and nightly)
- [Python 3.10+](https://www.python.org/) + [`maturin`](https://github.com/PyO3/maturin)
- [Go 1.24+](https://golang.org/)
- [Node.js 22+](https://nodejs.org/) + `npm`
- [just](https://github.com/casey/just) command runner

### Getting Started

Clone the repository and run the test suite to ensure everything is set up correctly:

```bash
git clone https://github.com/domenukk/prompt-templates.git
cd prompt-templates
cargo test --workspace --all-features
```

## Project Structure

- `crates/prompt-templates`: The core Rust template engine.
- `crates/prompt-templates-macros`: Rust procedural macros for build-time validation and codegen.
- `crates/prompt-templates-ffi`: C ABI for binding to other languages.
- `crates/prompt-templates-python`: Python bindings (PyO3).
- `crates/prompt-templates-wasm`: WASM bindings.
- `crates/prompt-templates-typescript`: TypeScript/Node integration.
- `go/prompt_templates`: Go bindings (cgo).
- `benchmarks`: Cross-language benchmark suite.

## Pull Requests

1. Fork the repo and create your branch from `main`.
2. If you've added code that should be tested, add tests.
3. If you've changed APIs, update the documentation and READMEs.
4. Ensure the test suite passes (`cargo test --workspace --all-features`).
5. Run the linters: `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
6. Run the formatters: `just fmt`.
7. Issue a pull request!

## Benchmarking

Performance is a key feature of this library. If you are making changes to the core engine, please run the benchmarks to ensure no regressions:

```bash
cd benchmarks
cargo bench
```

There are also benchmarks for the other languages, which can be run with `just bench-go`, `just bench-ts`, etc.

## Code of Conduct

Please be respectful and considerate of others when participating in this project.
