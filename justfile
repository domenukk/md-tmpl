# Python venv for the bindings crate

pyvenv := "crates/prompt-templates-python/.venv/bin/"

# Default recipe: format, lint, and test
default: fmt lint test

# ── Format ────────────────────────────────────────────────────────────

# Format all code (Rust, TOML, Markdown, Justfile)
fmt: fmt-rust fmt-toml fmt-markdown fmt-just fmt-python

# Format Rust code (nightly required for import grouping)
fmt-rust:
    cargo +nightly fmt

# Format TOML files
fmt-toml:
    taplo fmt

# Format Markdown files with prettier
fmt-markdown:
    npx -y prettier@latest --write '**/*.md'

# Format the justfile itself
fmt-just:
    just --fmt --unstable

# Format Python files with black
fmt-python:
    {{ pyvenv }}black crates/prompt-templates-python/python/

# ── Lint ──────────────────────────────────────────────────────────────

# Lint all code (Rust clippy, TOML, Markdown, Justfile)
lint: lint-rust lint-toml lint-markdown lint-just lint-python

# Lint Rust with clippy (pedantic + all, deny warnings)
lint-rust:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Lint TOML files
lint-toml:
    taplo check

# Lint Markdown files
lint-markdown:
    npx -y markdownlint-cli2@latest '**/*.md'

# Lint the justfile (check formatting)
lint-just:
    just --fmt --unstable --check

# Lint Python files with black (check) and mypy
lint-python:
    {{ pyvenv }}black --check crates/prompt-templates-python/python/
    {{ pyvenv }}mypy crates/prompt-templates-python/python/prompt_templates/ --ignore-missing-imports

# Run all tests
test: test-rust test-python

# Run Rust tests (lib + doctests + integration + macros, zero ignored)
test-rust:
    cargo test --workspace --all-features
    @echo "Verifying zero ignored tests..."
    @if cargo test --workspace --all-features 2>&1 | grep 'test result:' | grep -v '0 ignored'; then echo "ERROR: ignored tests found!" && exit 1; fi
    @echo "All tests pass, none ignored ✓"

# Build and test Python bindings
test-python:
    cd crates/prompt-templates-python && .venv/bin/maturin develop && .venv/bin/pytest python/tests/ -v

# ── Docs ──────────────────────────────────────────────────────────────

# Build documentation (checks for broken intra-doc links)
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# ── Other ─────────────────────────────────────────────────────────────

# Run all checks (lint + test + doc)
check: lint test doc

# ── Publish ───────────────────────────────────────────────────────────

# Publish everything (lint + test first, then Rust crates, then Python)
publish: lint test publish-rust publish-python

# Publish Rust crates to crates.io (prompt-templates first, then macros)
publish-rust:
    cargo publish -p prompt-templates
    @echo "Waiting for crates.io index..."
    sleep 30
    cargo publish -p prompt-templates-macros

# Publish Python package to PyPI via maturin
publish-python:
    cd crates/prompt-templates-python && maturin publish
