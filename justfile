# Use built-in Windows PowerShell on Windows so recipes work without Git Bash / MSYS2.

set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# Extract version from the main Cargo.toml without spawning shell utilities.

version := replace_regex(read("crates/md-tmpl/Cargo.toml"), '(?s).*?\nversion = "([^"]+)".*', "$1")

# Cross-platform Python & venv paths.

python := if os_family() == "windows" { "python" } else { "python3" }
pyvenv := if os_family() == "windows" { "crates\\md-tmpl-python\\.venv\\Scripts\\" } else { "crates/md-tmpl-python/.venv/bin/" }
pyvenv_local := if os_family() == "windows" { ".venv\\Scripts\\" } else { ".venv/bin/" }

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
    {{ pyvenv }}black crates/md-tmpl-python/python/

# Format Go files
fmt-go:
    gofmt -w go/md_tmpl
    gofmt -w go/cmd/pt-gen-go

# Format TypeScript files with prettier
[working-directory('crates/md-tmpl-typescript')]
fmt-ts:
    npx -y prettier@latest --write '**/*.ts'

# Check formatting across all languages without modifying files
fmt-check: lint-rust-fmt lint-toml lint-markdown lint-just _fmt-check-python _fmt-check-go _fmt-check-ts

_fmt-check-python:
    {{ pyvenv }}black --check crates/md-tmpl-python/python/

[unix]
_fmt-check-go:
    @cd go/md_tmpl && if [ -n "$(gofmt -l .)" ]; then echo "Go code is not formatted. Run 'just fmt'"; exit 1; fi
    @cd go/cmd/pt-gen-go && if [ -n "$(gofmt -l .)" ]; then echo "Go code is not formatted. Run 'just fmt'"; exit 1; fi

[windows]
_fmt-check-go:
    @if ((gofmt -l go/md_tmpl) -or (gofmt -l go/cmd/pt-gen-go)) { Write-Error "Go code is not formatted. Run 'just fmt'"; exit 1 }

[working-directory('crates/md-tmpl-typescript')]
_fmt-check-ts:
    npx -y prettier@latest --check '**/*.ts'

# ── Lint ──────────────────────────────────────────────────────────────

# Lint all code (Rust clippy, TOML, Markdown, Justfile, Go, Python, TypeScript, codegen, hygiene)
lint: lint-rust lint-toml lint-markdown lint-just lint-python lint-go lint-ts lint-codegen lint-hygiene

# Lint Rust with clippy (pedantic + all, deny warnings)
lint-rust:
    cargo clippy --workspace --all-targets --exclude md-tmpl-macros --all-features -- -D warnings
    cargo clippy -p md-tmpl-macros --all-targets -- -D warnings
    # Reduced feature combos: catch lints that only surface without `std`/all-features
    # (e.g. an unused generic in a `no_std` render path). See the codec features.
    cargo clippy -p md-tmpl-core --no-default-features -- -D warnings
    cargo clippy -p md-tmpl-core --no-default-features --features std -- -D warnings
    cargo clippy -p md-tmpl-core --no-default-features --features serde -- -D warnings
    cargo clippy -p md-tmpl-core --no-default-features --features cbor -- -D warnings
    cargo clippy -p md-tmpl-core --no-default-features --features flexbuffers -- -D warnings

# Check Rust formatting without modifying files
lint-rust-fmt:
    cargo +nightly fmt -- --check

# Lint TOML files
lint-toml:
    taplo check

# Lint Markdown files
lint-markdown:
    npx -y markdownlint-cli2@latest '**/*.md'
    npx -y prettier@latest --check '**/*.md'

# Lint the justfile (check formatting)
lint-just:
    just --fmt --unstable --check

# Lint Python files with black (check) and mypy
lint-python:
    {{ pyvenv }}black --check crates/md-tmpl-python/python/
    {{ pyvenv }}mypy crates/md-tmpl-python/python/md_tmpl/ --ignore-missing-imports

# Lint Go files (vet + gofmt check)
[unix]
lint-go: build-go-ffi
    cd go/md_tmpl && go vet ./...
    cd go/cmd/pt-gen-go && go vet ./...
    @cd go/md_tmpl && if [ -n "$(gofmt -l .)" ]; then echo "Go code is not formatted. Run 'just fmt'"; exit 1; fi
    @cd go/cmd/pt-gen-go && if [ -n "$(gofmt -l .)" ]; then echo "Go code is not formatted. Run 'just fmt'"; exit 1; fi

# Lint Go files (vet + gofmt check)
[windows]
lint-go: build-go-ffi
    Push-Location go/md_tmpl; go vet ./...; $rc = $LASTEXITCODE; Pop-Location; if ($rc -ne 0) { exit $rc }
    Push-Location go/cmd/pt-gen-go; go vet ./...; $rc = $LASTEXITCODE; Pop-Location; if ($rc -ne 0) { exit $rc }
    @if ((gofmt -l go/md_tmpl) -or (gofmt -l go/cmd/pt-gen-go)) { Write-Error "Go code is not formatted. Run 'just fmt'"; exit 1 }

# Lint TypeScript (strict type-check with tsc, ESLint, prettier)
[working-directory('crates/md-tmpl-typescript')]
lint-ts:
    npx tsc --noEmit --strict
    npx eslint .
    npx -y prettier@latest --check '**/*.ts'

# Verify zero-diff of generated static type-generator stubs across TS, Go, and Python
lint-codegen: build-ts build-go-ffi _maturin-develop
    {{ python }} scripts/lint_codegen.py --check

# Regenerate golden static type-generator stubs across TS, Go, and Python
gen-stubs: build-ts build-go-ffi _maturin-develop
    {{ python }} scripts/lint_codegen.py --write

[working-directory('crates/md-tmpl-python')]
_maturin-develop:
    {{ pyvenv_local }}maturin develop

# Run hygiene linter (suppression patterns, error handling, file length)
lint-hygiene:
    {{ python }} scripts/lint_hygiene.py

# Run all tests
test: test-rust test-no-std test-python test-go test-ts test-wasm

# Run Rust tests (lib + doctests + integration + macros, zero ignored)
[unix]
test-rust:
    #!/usr/bin/env bash
    set -euo pipefail
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    cargo test --workspace --exclude md-tmpl-macros --all-features 2>&1 | tee "$tmp"
    cargo test -p md-tmpl-macros 2>&1 | tee -a "$tmp"
    echo "Verifying zero ignored tests..."
    if grep 'test result:' "$tmp" | grep -v '0 ignored'; then
        echo "ERROR: ignored tests found!"
        exit 1
    fi
    echo "All tests pass, none ignored"

# Run Rust tests (lib + doctests + integration + macros, zero ignored)
[windows]
test-rust:
    $tmp = Join-Path $env:TEMP "md-tmpl-cargo-test-$PID.txt"; try { cargo test --workspace --exclude md-tmpl-macros --all-features 2>&1 | ForEach-Object { "$_" } | Tee-Object -FilePath $tmp; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; cargo test -p md-tmpl-macros 2>&1 | ForEach-Object { "$_" } | Tee-Object -FilePath $tmp -Append; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; Write-Output 'Verifying zero ignored tests...'; $bad = Select-String -Path $tmp -Pattern 'test result:' | Where-Object { $_.Line -notmatch '0 ignored' }; if ($bad) { $bad | ForEach-Object { Write-Output $_.Line }; Write-Error 'ERROR: ignored tests found!'; exit 1 }; Write-Output 'All tests pass, none ignored' } finally { Remove-Item -Force -ErrorAction SilentlyContinue $tmp }

# Verify no_std compatibility (integration tests + true no_std target build)
test-no-std:
    @echo "-- no_std integration tests --"
    cargo test -p md-tmpl --no-default-features --test no_std_compat
    @echo ""
    @echo "-- no_std target build (thumbv7em-none-eabihf) --"
    cargo build -p md-tmpl --no-default-features --target thumbv7em-none-eabihf
    cargo build -p md-tmpl --no-default-features --features serde --target thumbv7em-none-eabihf
    cargo build -p md-tmpl --no-default-features --features macros --target thumbv7em-none-eabihf
    cargo build -p md-tmpl --no-default-features --features serde,macros --target thumbv7em-none-eabihf
    cargo build -p md-tmpl --no-default-features --features cbor --target thumbv7em-none-eabihf
    cargo build -p md-tmpl --no-default-features --features serde,cbor,macros --target thumbv7em-none-eabihf
    @echo "All no_std checks pass"

# Build and test Python bindings
[working-directory('crates/md-tmpl-python')]
test-python:
    {{ pyvenv_local }}maturin develop
    {{ pyvenv_local }}pytest python/tests/ -v

# Build and test Go bindings
[working-directory('go/md_tmpl')]
test-go: build-go-ffi
    go test -v -count=1 ./...

# Build and test TypeScript bindings
[working-directory('crates/md-tmpl-typescript')]
test-ts: build-ts
    node --test dist/tests

# Run WASM tests (parity + comprehensive unit tests)
[working-directory('crates/md-tmpl-wasm')]
test-wasm: build-wasm
    npm test

# Replay the shared cross-language conformance corpus against ALL FIVE backends
test-conformance: build-ts build-go-ffi build-wasm
    @echo "-- Rust conformance harness --"
    cargo test -p md-tmpl-core --test conformance --all-features
    @echo "-- TypeScript conformance harness --"
    node --test crates/md-tmpl-typescript/dist/tests/conformance.test.js
    @echo "-- Go conformance harness --"
    just _test-conformance-go
    @echo "-- Python conformance harness --"
    just _test-conformance-python
    @echo "-- WASM conformance harness --"
    node --test crates/md-tmpl-wasm/dist/conformance.test.js

[working-directory('go/md_tmpl')]
_test-conformance-go:
    go test -run TestConformance -count=1 ./...

[working-directory('crates/md-tmpl-python')]
_test-conformance-python:
    {{ pyvenv_local }}maturin develop
    {{ pyvenv_local }}pytest python/tests/test_conformance.py -q

# ── Docs ──────────────────────────────────────────────────────────────

# Build documentation (checks for broken intra-doc links)
[unix]
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# Build documentation (checks for broken intra-doc links)
[windows]
doc:
    $env:RUSTDOCFLAGS = '-D warnings'; cargo doc --all-features --no-deps; exit $LASTEXITCODE

# ── Benchmark ─────────────────────────────────────────────────────────

# Run all benchmarks (Rust + Go + TypeScript + WASM + Python)
bench: bench-rust bench-go bench-ts bench-wasm bench-python

# Run Rust benchmarks (via criterion)
bench-rust:
    cargo bench -p md-tmpl

# Run Go benchmarks
[working-directory('go/md_tmpl')]
bench-go: build-go-ffi
    go test -bench=. -benchmem -count=1 ./...

# Run TypeScript benchmarks
[working-directory('crates/md-tmpl-typescript')]
bench-ts: build-ts
    node dist/benchmarks/bench.js

# Run TypeScript comparison benchmarks (vs Handlebars, Mustache)
[working-directory('crates/md-tmpl-typescript')]
bench-ts-compare: build-ts
    node dist/benchmarks/comparison.js

# Run WASM vs TypeScript comparative benchmarks
[working-directory('crates/md-tmpl-wasm')]
bench-wasm: build-wasm
    npm run bench

# Run Python benchmarks (vs Jinja2, Mako, Chevron, Django)
bench-python:
    {{ pyvenv }}python benchmarks/python/bench_templates.py

# Micro-benchmark: render_ctx vs render_ctx_into buffer reuse
[working-directory('benchmarks')]
bench-perf:
    cargo run --release --example perf_test

# Run all benchmarks and update README.md + RESULTS.md tables
bench-update:
    {{ python }} benchmarks/scripts/run_and_update.py

# Run Rust benchmarks and update tables
bench-update-rust:
    {{ python }} benchmarks/scripts/run_and_update.py --lang rust

# Run Python benchmarks and update tables
bench-update-python:
    {{ python }} benchmarks/scripts/run_and_update.py --lang python

# Run Go benchmarks and update tables
bench-update-go:
    {{ python }} benchmarks/scripts/run_and_update.py --lang go

# Run TypeScript benchmarks and update tables
bench-update-ts:
    {{ python }} benchmarks/scripts/run_and_update.py --lang ts

# Run WASM benchmarks and update tables
bench-update-wasm:
    {{ python }} benchmarks/scripts/run_and_update.py --lang wasm

# ── Build ─────────────────────────────────────────────────────────────

# Build the FFI static library (required by Go bindings)
build-go-ffi:
    cargo build -p md-tmpl-ffi --release

# Build the TypeScript bindings
[unix]
[working-directory('crates/md-tmpl-typescript')]
build-ts:
    rm -rf dist
    npx tsc

# Build the TypeScript bindings
[windows]
[working-directory('crates/md-tmpl-typescript')]
build-ts:
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue dist
    npx tsc

# Build the WASM package (via wasm-pack)
[unix]
[working-directory('crates/md-tmpl-wasm')]
build-wasm:
    rm -rf pkg dist
    wasm-pack build --target nodejs --out-dir pkg --release
    npm run build

# Build the WASM package (via wasm-pack)
[windows]
[working-directory('crates/md-tmpl-wasm')]
build-wasm:
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue pkg, dist
    wasm-pack build --target nodejs --out-dir pkg --release
    npm run build

# ── Other ─────────────────────────────────────────────────────────────

# Run all Rust checks that require no external Go/Node/venv runtimes
check-rust: lint-rust lint-toml lint-just lint-hygiene test-rust doc

# Run all checks (fmt check + lint + test + doc)
check: lint-rust-fmt lint test doc

# ── Publish ───────────────────────────────────────────────────────────

[private]
_bump_py := 'import glob, pathlib, re, sys
old, new = sys.argv[1], sys.argv[2]
q = chr(34)
def rw(path, fn):
    p = pathlib.Path(path)
    p.write_text(fn(p.read_text(encoding="utf-8")), encoding="utf-8", newline="\n")
for p in glob.glob("crates/*/Cargo.toml") + ["crates/md-tmpl-python/pyproject.toml", "benchmarks/Cargo.toml"]:
    rw(p, lambda s: re.sub(rf"(?m)^version = {q}{re.escape(old)}{q}", lambda _: f"version = {q}{new}{q}", s))
for p in glob.glob("crates/*/Cargo.toml"):
    rw(p, lambda s: re.sub(rf"(md-tmpl(?:-core|-macros)? = \{{ version = ){q}{re.escape(old)}{q}", lambda m: f"{m.group(1)}{q}{new}{q}", s))
for p in ("crates/md-tmpl-typescript/package.json", "crates/md-tmpl-typescript/package-lock.json", "crates/md-tmpl-wasm/package.json", "crates/md-tmpl-wasm/package-lock.json"):
    rw(p, lambda s: s.replace(f"{q}version{q}: {q}{old}{q}", f"{q}version{q}: {q}{new}{q}"))'

# Bump version across all workspace crates, Python pyproject.toml, and npm package.json files
bump new_version: (_bump-files version new_version)
    cargo update -w
    cargo update --manifest-path benchmarks/Cargo.toml -w

[unix]
_bump-files old new:
    @printf '%s\n' '{{ _bump_py }}' | {{ python }} - "{{ old }}" "{{ new }}"

[windows]
_bump-files old new:
    @'{{ _bump_py }}' | {{ python }} - "{{ old }}" "{{ new }}"

# Publish everything (lint + test first, then all packages)
publish: lint test publish-rust publish-python publish-ts

# Publish Rust crates to crates.io in dependency order (core → macros → md-tmpl)
[unix]
publish-rust:
    cargo publish -p md-tmpl-core
    @echo "Waiting for crates.io index..."
    sleep 30
    cargo publish -p md-tmpl-macros
    @echo "Waiting for crates.io index..."
    sleep 30
    cargo publish -p md-tmpl

# Publish Rust crates to crates.io in dependency order (core → macros → md-tmpl)
[windows]
publish-rust:
    cargo publish -p md-tmpl-core
    @Write-Output "Waiting for crates.io index..."
    Start-Sleep -Seconds 30
    cargo publish -p md-tmpl-macros
    @Write-Output "Waiting for crates.io index..."
    Start-Sleep -Seconds 30
    cargo publish -p md-tmpl

# Publish Python package to PyPI via maturin
[working-directory('crates/md-tmpl-python')]
publish-python:
    maturin publish

# Publish TypeScript package to npm
[working-directory('crates/md-tmpl-typescript')]
publish-ts: build-ts
    npm publish --access public

# Tag a Go module release (Go modules are released via git tags)

# Version is read from crates/md-tmpl/Cargo.toml
publish-go:
    @echo "Tagging Go module release: go/md_tmpl/v{{ version }}"
    git tag "go/md_tmpl/v{{ version }}"
    @echo "Tagged. Push with: git push origin go/md_tmpl/v{{ version }}"

# ── Setup ─────────────────────────────────────────────────────────────

# Set up Python development environment
setup-python:
    {{ python }} -m venv crates/md-tmpl-python/.venv
    {{ pyvenv }}pip install maturin pytest black mypy tomli -r benchmarks/python/requirements.txt

# Set up TypeScript development environment
[working-directory('crates/md-tmpl-typescript')]
setup-ts:
    npm install
