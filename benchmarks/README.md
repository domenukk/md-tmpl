# Benchmarks

Performance benchmarks for
[md-tmpl](https://github.com/domenukk/md-tmpl) across
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

Templates are pre-parsed before timing — benchmarks measure
**render throughput only**, not parsing.

## Rust

Criterion, comparing against Tera, MiniJinja, and Handlebars.

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
cd crates/md-tmpl-python && maturin develop && cd ../..
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
wasm-pack build --target nodejs --out-dir pkg
npm run build
node dist/bench.js            # table output
node dist/bench.js --json     # JSON to stdout
```

## Fairness & Methodology

These benchmarks aim for objectivity. Known asymmetries:

- **Rust render-only groups**: MiniJinja re-serializes `&impl Serialize`
  each iteration while the other engines use pre-built contexts. The
  `_e2e` groups include context construction for all engines to level
  this playing field.
- **Handlebars** lacks string equality, numeric comparisons, and filters.
  Data structs include pre-computed boolean flags and formatted strings
  so it can produce equivalent output — Handlebars therefore does
  slightly less per-render work.
- **Python**: `md-tmpl` is a Rust/PyO3 engine; competitors
  (Jinja2, Mako, Django, Chevron) are pure Python. The speed advantage
  is partly due to native code, not template design alone.
- **Go**: md-tmpl uses CGo FFI (~100–200ns overhead per call),
  penalizing it relative to the pure-Go `text/template` on small
  templates. The comparison is fair at the API boundary level.
- **TypeScript comparison**: Mustache lacks `elif` and filters, so its
  benchmark templates do less work in conditional/loop scenarios.

All comparison benchmarks verify output correctness before timing.

## Results

Benchmark tables are in each language binding's README:
[Rust](../crates/md-tmpl/README.md#performance) ·
[Python](../crates/md-tmpl-python/README.md#performance) ·
[Go](../go/md_tmpl/README.md#performance) ·
[TypeScript](../crates/md-tmpl-typescript/README.md#performance) ·
[WASM](../crates/md-tmpl-wasm/README.md#performance)
