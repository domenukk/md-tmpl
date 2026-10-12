# Benchmarks

Performance benchmarks for
[md-tmpl](https://github.com/domenukk/md-tmpl) across
all language bindings.

## Scenarios

All benchmarks share four core scenarios (five in Rust and TypeScript):

| Scenario        | Description                                                            |
| --------------- | ---------------------------------------------------------------------- |
| **Simple**      | Variable substitution (`{{ name }}`)                                   |
| **Loop**        | Iterating over a list with `{% for %}`                                 |
| **Conditional** | `if`/`elif`/`else` branching with filters                              |
| **Hero**        | Nested loops + conditionals                                            |
| **Mega**        | (Rust, TypeScript, WASM) Large-scale template with nested loops/scores |

Benchmarks measure both **pre-parsed render throughput** and **compile + render (end-to-end / round-trip)** latency.

## Rust

Criterion, comparing `md-tmpl (macro)` (`include_template!` / `template!` native codegen) and `md-tmpl` (`render_ctx` / `render`) against Tera, MiniJinja, and Handlebars across both pre-built context and `_e2e` (struct construction/serialization + validation + render) modes.

```bash
cd benchmarks
cargo bench
```

Results in `benchmarks/target/criterion/` with HTML reports.

> The standalone crate at `crates/md-tmpl` also has Criterion
> benchmarks for internals (parse, render, filters, conditions).

## Python

Compares against Jinja2, Mako, Chevron, and Django templates.

```bash
source crates/md-tmpl-python/.venv/bin/activate
pip install maturin
cd crates/md-tmpl-python && maturin develop --release && cd ../..
pip install -r benchmarks/python/requirements.txt
python benchmarks/python/bench_templates.py
```

> Chevron (Mustache) only participates in simple/loop (no filter or
> `elif` support).

## Go

Go's `testing.B` framework, comparing md-tmpl (Rust/CGo FFI)
against Go's `text/template` across parse, render, and round-trip
scenarios.

```bash
just bench-go
```

## TypeScript

```bash
just bench-ts           # internal benchmarks
just bench-ts-compare   # vs Handlebars & Mustache
```

## WASM

Compares WASM bindings against the pure-TypeScript implementation.

```bash
cd crates/md-tmpl-wasm
wasm-pack build --target nodejs --out-dir pkg --release
npm run build
node dist/bench.js            # table output
node dist/bench.js --json     # JSON to stdout
```

## Fairness & Methodology

These benchmarks aim for objectivity. Known asymmetries:

- **Rust**: `md-tmpl (macro)` (`include_template!` / `template!`) compiles the template into native Rust statements on the generated `Params` struct (zero `Context` or `HashMap`). `md-tmpl` uses `render_ctx` (full runtime frontmatter schema validation on every call) in the pre-built context groups and `Template::render(&struct)` (validating on first use and caching `TypeId::of::<T>()`) in the `_e2e` groups. In the render-only groups, MiniJinja re-serializes `&impl Serialize` each iteration while the other runtime engines use pre-built contexts; the `_e2e` groups include struct/context construction inside the timed loop for all engines (including `build_*_macro_data` for `md-tmpl (macro)`).
- **Handlebars** lacks string equality, numeric comparisons, and filters. Data structs include pre-computed boolean flags and formatted strings so it can produce equivalent output — Handlebars therefore does slightly less per-render work.
- **Python**: `md-tmpl` is a Rust/PyO3 engine; competitors (Jinja2, Mako, Django, Chevron) are pure Python. Chevron has no pre-compilation API (`compile()` only wraps the source string in a closure and re-tokenizes on every `render()` call).
- **Go**: `md-tmpl` uses CGo FFI (~100–200 ns overhead per call), penalizing it relative to pure-Go `text/template` on tiny templates (`small`) and single-use round-trips while winning by 5–6× on medium and large templates.
- **TypeScript**: Every `md-tmpl` `Template.fromSource()` call performs a full cold parse, YAML frontmatter compilation, static type/displayability/collision checking, and AST compilation with zero cross-template memoization caches; every `render()` call performs full runtime schema validation (`MissingParamsError` / `TypeMismatchError` / `ExtraParamsError`). Mustache has no `elif` or filters (and clears its internal token cache via `Mustache.clearCache()` on cold-compile/round-trip benchmarks) and performs zero schema validation, so Mustache is faster on single-use cold round-trips while `md-tmpl` is faster on pre-compiled rendering.

All comparison benchmarks verify output correctness before timing.

## Results

Benchmark tables are in each language binding's README:
[Rust](../crates/md-tmpl/README.md#performance) ·
[Python](../crates/md-tmpl-python/README.md#performance) ·
[Go](../go/md_tmpl/README.md#performance) ·
[TypeScript](../crates/md-tmpl-typescript/README.md#performance) ·
[WASM](../crates/md-tmpl-wasm/README.md#performance)
