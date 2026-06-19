# Extract version from the main Cargo.toml

version := `grep '^version' crates/prompt-templates/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'`

# Python venv for the bindings crate

pyvenv := "crates/prompt-templates-python/.venv/bin/"

# Default recipe: format, lint, and test
default: fmt lint test

# ── Format ────────────────────────────────────────────────────────────

# Format all code (Rust, TOML, Markdown, Justfile, Go, Python, TypeScript)
fmt: fmt-rust fmt-toml fmt-markdown fmt-just fmt-python fmt-go fmt-ts

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

# Format Go files
fmt-go:
    cd go/prompt_templates && gofmt -w .

# Format TypeScript files with prettier
fmt-ts:
    cd crates/prompt-templates-typescript && npx -y prettier@latest --write 'src/**/*.ts'

# ── Lint ──────────────────────────────────────────────────────────────

# Lint all code (Rust clippy, TOML, Markdown, Justfile, Go, Python, TypeScript)
lint: lint-rust lint-toml lint-markdown lint-just lint-python lint-go lint-ts

# Lint Rust with clippy (pedantic + all, deny warnings)
lint-rust:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Check Rust formatting without modifying files
lint-rust-fmt:
    cargo +nightly fmt -- --check

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

# Lint Go files (vet)
lint-go: build-go-ffi
    cd go/prompt_templates && go vet ./...

# Lint TypeScript (strict type-check with tsc)
lint-ts:
    cd crates/prompt-templates-typescript && npx tsc --noEmit --strict

# Run all tests
test: test-rust test-no-std test-python test-go test-ts test-wasm

# Run Rust tests (lib + doctests + integration + macros, zero ignored)
test-rust:
    cargo test --workspace --all-features 2>&1 | tee /tmp/cargo-test-output.txt
    @echo "Verifying zero ignored tests..."
    @if grep 'test result:' /tmp/cargo-test-output.txt | grep -v '0 ignored'; then echo "ERROR: ignored tests found!" && exit 1; fi
    @echo "All tests pass, none ignored ✓"

# Verify no_std compatibility (integration tests + true no_std target build)
test-no-std:
    @echo "── no_std integration tests ──"
    cargo test -p prompt-templates --no-default-features --test no_std_compat
    @echo ""
    @echo "── no_std target build (thumbv7em-none-eabihf) ──"
    cargo build -p prompt-templates --no-default-features --target thumbv7em-none-eabihf
    cargo build -p prompt-templates --no-default-features --features serde --target thumbv7em-none-eabihf
    cargo build -p prompt-templates --no-default-features --features typed-builder --target thumbv7em-none-eabihf
    cargo build -p prompt-templates --no-default-features --features serde,typed-builder --target thumbv7em-none-eabihf
    @echo "All no_std checks pass ✓"

# Build and test Python bindings
test-python:
    cd crates/prompt-templates-python && .venv/bin/maturin develop && .venv/bin/pytest python/tests/ -v

# Build and test Go bindings
test-go: build-go-ffi
    cd go/prompt_templates && go test -v -count=1 ./...

# Build and test TypeScript bindings
test-ts: build-ts
    cd crates/prompt-templates-typescript && node --test dist/tests/template.test.js

# Run WASM tests (parity + comprehensive unit tests)
test-wasm: build-wasm
    cd crates/prompt-templates-wasm && npx tsc && node dist/correctness.js && node --test dist/wasm.test.js

# ── Docs ──────────────────────────────────────────────────────────────

# Build documentation (checks for broken intra-doc links)
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# ── Benchmark ─────────────────────────────────────────────────────────

# Run all benchmarks (Rust + Go + TypeScript + WASM + Python)
bench: bench-rust bench-go bench-ts bench-wasm bench-python

# Run Rust benchmarks (via criterion)
bench-rust:
    cargo bench -p prompt-templates

# Run Go benchmarks
bench-go: build-go-ffi
    cd go/prompt_templates && go test -bench=. -benchmem -count=1 ./...

# Run TypeScript benchmarks
bench-ts: build-ts
    cd crates/prompt-templates-typescript && node dist/benchmarks/bench.js

# Run TypeScript comparison benchmarks (vs Handlebars, Mustache)
bench-ts-compare: build-ts
    cd crates/prompt-templates-typescript && node dist/benchmarks/comparison.js

# Run WASM vs TypeScript comparative benchmarks
bench-wasm: build-wasm
    cd crates/prompt-templates-wasm && node benchmarks/bench.mjs

# Run Python benchmarks (vs Jinja2, Mako, Chevron, Django)
bench-python:
    python benchmarks/python/bench_templates.py

# Run all benchmarks and update README.md + RESULTS.md tables
bench-update:
    python3 benchmarks/scripts/run_and_update.py

# Run Rust benchmarks and update tables
bench-update-rust:
    python3 benchmarks/scripts/run_and_update.py --lang rust

# Run Python benchmarks and update tables
bench-update-python:
    python3 benchmarks/scripts/run_and_update.py --lang python

# Run Go benchmarks and update tables
bench-update-go:
    python3 benchmarks/scripts/run_and_update.py --lang go

# Run TypeScript benchmarks and update tables
bench-update-ts:
    python3 benchmarks/scripts/run_and_update.py --lang ts

# Run WASM benchmarks and update tables
bench-update-wasm:
    python3 benchmarks/scripts/run_and_update.py --lang wasm

# ── Build ─────────────────────────────────────────────────────────────

# Build the FFI static library (required by Go bindings)
build-go-ffi:
    cargo build -p prompt-templates-ffi --release

# Build the TypeScript bindings
build-ts:
    cd crates/prompt-templates-typescript && npx tsc

# Build the WASM package (via wasm-pack)
build-wasm:
    cd crates/prompt-templates-wasm && wasm-pack build --target nodejs --out-dir pkg --release

# ── Other ─────────────────────────────────────────────────────────────

# Run all checks (fmt check + lint + test + doc)
check: lint-rust-fmt lint test doc

# ── Publish ───────────────────────────────────────────────────────────

# Publish everything (lint + test first, then all packages)
publish: lint test publish-rust publish-python publish-ts

# Publish Rust crates to crates.io (prompt-templates first, then macros)
publish-rust:
    cargo publish -p prompt-templates
    @echo "Waiting for crates.io index..."
    sleep 30
    cargo publish -p prompt-templates-macros

# Publish Python package to PyPI via maturin
publish-python:
    cd crates/prompt-templates-python && maturin publish

# Publish TypeScript package to npm
publish-ts: build-ts
    cd crates/prompt-templates-typescript && npm publish --access public

# Tag a Go module release (Go modules are released via git tags)

# Version is read from crates/prompt-templates/Cargo.toml
publish-go:
    @echo "Tagging Go module release: go/prompt_templates/v{{ version }}"
    git tag "go/prompt_templates/v{{ version }}"
    @echo "Tagged. Push with: git push origin go/prompt_templates/v{{ version }}"

# ── Setup ─────────────────────────────────────────────────────────────

# Set up Python development environment
setup-python:
    python3 -m venv crates/prompt-templates-python/.venv
    crates/prompt-templates-python/.venv/bin/pip install maturin pytest black mypy

# Set up TypeScript development environment
setup-ts:
    cd crates/prompt-templates-typescript && npm install
