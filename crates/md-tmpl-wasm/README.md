# md-tmpl-wasm

WebAssembly bindings for the
[md-tmpl](https://github.com/domenukk/md-tmpl) engine.
Wraps the full Rust engine via `wasm-bindgen` for use in Node.js and
browser environments.

Implements the same `ITemplate` interface as the pure-TypeScript
`md-tmpl` package — swap backends without changing application
code.

## Building

```bash
wasm-pack build --target nodejs --out-dir pkg

# For browser:
wasm-pack build --target web --out-dir pkg
```

## Quick Start

```ts
import { Template } from "./pkg/md_tmpl_wasm";

const tmpl = Template.fromSource(`---
params:
  - name = str
  - role = str
---
You are {{ role }}. Hello {{ name }}!`);

console.log(tmpl.render({ name: "Alice", role: "an AI assistant" }));
// → "You are an AI assistant. Hello Alice!"
```

## API

### Constructors

```ts
Template.fromSource(source: string): Template
Template.fromSourceAllowingUnused(source: string): Template
Template.fromSourceWithBaseDir(source: string, baseDir: string): Template
```

### Rendering

```ts
tmpl.render(params: object): string              // type-validated
tmpl.renderUnchecked(params: object): string      // allows extra params
tmpl.renderJson(jsonStr: string): string          // avoids per-field WASM crossings
tmpl.renderUncheckedJson(jsonStr: string): string
tmpl.renderFlexbuffers(buffer: Uint8Array): string        // zero-copy binary
tmpl.renderUncheckedFlexbuffers(buffer: Uint8Array): string
```

### Metadata

```ts
tmpl.body(): string
tmpl.defaults(): object
tmpl.consts(): object
tmpl.importedConsts(): object
tmpl.declarations(): [string, string][]
tmpl.sourceHash(): number
```

## Serialization Tiers

| Method                | Input        | Overhead                         |
| --------------------- | ------------ | -------------------------------- |
| `render()`            | JS object    | serde-wasm-bindgen per field     |
| `renderJson()`        | JSON string  | One string copy, JSON parse      |
| `renderFlexbuffers()` | `Uint8Array` | Zero-copy binary deserialization |

For small templates, `render()` is simplest. For high-throughput
scenarios, `renderFlexbuffers()` eliminates serialization overhead.

## ITemplate Compatibility

```ts
import type { ITemplate } from "md-tmpl";

function renderGreeting(tmpl: ITemplate, name: string): string {
  return tmpl.render({ name });
}

import { Template as WasmTemplate } from "md-tmpl-wasm";
import { Template as TsTemplate } from "md-tmpl";

renderGreeting(WasmTemplate.fromSource(src), "Alice");
renderGreeting(TsTemplate.fromSource(src), "Alice");
```

## Performance

### WASM vs Pure-TypeScript

([source](benchmarks/bench.ts))

| Scenario                                |    WASM (Rust) |      TypeScript | speedup |
| --------------------------------------- | -------------: | --------------: | ------: |
| parse simple                            |        5.74 µs |  **3.76 µs** 🏆 | 1.5× TS |
| render simple (1 param)                 |        2.31 µs |   **433 ns** 🏆 | 5.3× TS |
| render multi-param (4 params)           |        4.11 µs |  **1.12 µs** 🏆 | 3.7× TS |
| render list/for (2 items)               |        6.43 µs |  **3.07 µs** 🏆 | 2.1× TS |
| render list/for (20 items)              |       40.64 µs | **21.79 µs** 🏆 | 1.9× TS |
| render conditional (if/elif)            |        2.99 µs |  **1.21 µs** 🏆 | 2.5× TS |
| render enum dispatch                    |        4.23 µs |  **1.39 µs** 🏆 | 3.0× TS |
| render with defaults                    |        2.94 µs |  **1.10 µs** 🏆 | 2.7× TS |
| render complex (nested+list+filter)     |       11.82 µs |  **6.60 µs** 🏆 | 1.8× TS |
| sourceHash()                            |           9 ns |     **6 ns** 🏆 | 1.6× TS |
| declarations()                          |   **42 ns** 🏆 |          578 ns |   13.9× |
| consts()                                |   **41 ns** 🏆 |           59 ns |    1.4× |
| renderJson simple (1 param)             |        1.24 µs |   **501 ns** 🏆 | 2.5× TS |
| renderJson multi-param (4 params)       |        2.84 µs |  **1.38 µs** 🏆 | 2.1× TS |
| renderJson complex (nested+list+filter) | **6.17 µs** 🏆 |         6.50 µs |    1.1× |

Pure-TS is faster for rendering due to JS↔WASM serialization overhead.
WASM wins on metadata access. Use WASM when you need exact Rust-engine
parity.

## License

Apache-2.0 OR MIT
