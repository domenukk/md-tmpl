# Benchmarks

Performance benchmarks for
[prompt-templates](https://github.com/domenukk/prompt-templates) across
all language bindings.

## Scenarios

All benchmarks share four core scenarios (five in Rust):

| Scenario        | Description                                        |
| --------------- | -------------------------------------------------- |
| **Simple**      | Variable substitution (`{{ name }}`)               |
| **Loop**        | Iterating over a list with `{% for %}`             |
| **Conditional** | `if`/`elif`/`else` branching with filters          |
| **Hero**        | Nested loops + conditionals                        |
| **Mega**        | (Rust only) Large-scale template with all features |

Templates are pre-compiled before timing — benchmarks measure
**render throughput only**, not compilation.

## Rust

Criterion, comparing against Tera, MiniJinja, and Handlebars.

```bash
cd benchmarks
cargo bench
```

Results in `benchmarks/target/criterion/` with HTML reports.

> The standalone crate at `crates/prompt-templates` also has Criterion
> benchmarks for internals (compile, render, filters, conditions).

## Python

Compares against Jinja2, Mako, Chevron, and Django templates.

```bash
source crates/prompt-templates-python/.venv/bin/activate
pip install maturin
cd crates/prompt-templates-python && maturin develop && cd ../..
pip install -r benchmarks/python/requirements.txt
python benchmarks/python/bench_templates.py
```

> Chevron (Mustache) only participates in simple/loop (no filter or
> `elif` support).

## Go

Go's `testing.B` framework, comparing against `text/template`.

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
cd crates/prompt-templates-wasm
wasm-pack build --target nodejs --out-dir pkg
node benchmarks/bench.mjs            # table output
node benchmarks/bench.mjs --json     # JSON to stdout
```

## Results

Benchmark tables are in each language binding's README:
[Rust](../crates/prompt-templates/README.md#performance) ·
[Python](../crates/prompt-templates-python/README.md#performance) ·
[Go](../go/prompt_templates/README.md#performance) ·
[TypeScript](../crates/prompt-templates-typescript/README.md#performance) ·
[WASM](../crates/prompt-templates-wasm/README.md#performance)
