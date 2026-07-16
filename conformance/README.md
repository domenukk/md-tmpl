# md-tmpl — Shared Conformance Corpus

One **language-neutral** corpus of test cases, meant to be executed by BOTH the
Rust core (`md-tmpl-core`) and the TypeScript parser (`md-tmpl-typescript`) so
that the two independent implementations are proven to agree, case-for-case.
This is the single source of truth for cross-backend parity.

> [!NOTE]
> **Phase 1 (this directory): the corpus + this README only.** The Rust and
> TypeScript test harnesses that load these files are intentionally **not** in
> the tree yet (Phase 2), and the `escapes` / `comments` matrices are omitted
> because that behavior is being changed elsewhere right now. See
> [Scope & sequencing](#scope--sequencing).

## Files

| File                 | What it asserts                                        | Cases |
| -------------------- | ------------------------------------------------------ | ----- |
| `render.json`        | `source` + `params` → exact rendered `output`          | 37    |
| `frontmatter.json`   | `source` → `defaults()` object                         | 12    |
| `errors.json`        | `source` (+`params`) → an error at a given phase       | 7     |
| `interpolation.json` | `{{ }}` interpolation inside statement string contexts | 2     |

Total: **58** cases, every one of which is currently satisfied by **both**
backends (verified — see [Provenance](#provenance-how-these-were-derived)).

JSON was chosen so `serde_json` (Rust) and `JSON.parse` (TS) both load it
trivially; `\n` inside `source` strings encodes multi-line templates.

## Case schema

Each file is a JSON array of case objects:

```json
{
  "name": "interp_single",
  "note": "single {{ }} interpolation",
  "source": "---\nparams:\n  - name = str\n---\nHello {{ name }}!",
  "params": { "name": "World" },
  "expect": { "kind": "render", "output": "Hello World!" }
}
```

Common fields:

- `name` — unique, stable identifier. **Harnesses MUST report this name on
  failure** so drift is diagnosable.
- `note` — one-line human description / executable documentation.
- `source` — the full template text (frontmatter + body).
- `params` — the render context (present for `render` and some `error` cases).
- `env` — _(optional, `render` only)_ compile-time `env:` values, e.g.
  `{ "REGION": "eu" }`. Maps to `Template.fromSourceWithEnv` (TS) /
  `CompileOptions::env` (Rust).

`expect.kind` is one of:

| kind      | extra fields                                             | meaning                                                           |
| --------- | -------------------------------------------------------- | ----------------------------------------------------------------- |
| `render`  | `output` (string)                                        | exact rendered output                                             |
| `default` | `defaults` (object)                                      | `Template.defaults()` projected to canonical JSON                 |
| `error`   | `phase` (`compile`\|`render`), `error_contains` (string) | the named stage MUST fail with a message containing the substring |

## Value encoding (canonical JSON)

Expected values (`params`, `defaults`, `output`) use the engines' **native JSON
projection** — the convention explicitly permitted by the design doc, chosen
here over an abstract `{variant, fields}` form because it is lossless, requires
no re-encoding, and both backends already emit it identically:

| Type                    | Encoding                                                                               |
| ----------------------- | -------------------------------------------------------------------------------------- |
| str/int/float/bool      | JSON scalar                                                                            |
| list                    | JSON array                                                                             |
| struct                  | JSON object                                                                            |
| option `None`           | `null`                                                                                 |
| option `Some x`         | transparent — just `x`                                                                 |
| enum **unit** variant   | the bare variant name as a string, e.g. `"Active"`                                     |
| enum **struct** variant | object tagged with `__kind__`, e.g. `{ "__kind__": "Confirmed", "evidence": "found" }` |

> [!NOTE]
> A unit enum value is indistinguishable from a plain string at the value layer.
> This is intentional and true of **both** engines, so a Phase-2 harness can
> compare with a plain deep-equal. Enum _inputs_ in `params` follow the same
> convention (bare string for unit variants, `__kind__` object for struct
> variants).

## Authoring rules (learned from the reference implementation)

These are not arbitrary style choices — the engine enforces them, so cases must
follow them:

- **Blank line before a new frontmatter section.** A block list (`consts:`,
  `types:`, `env:`) must be followed by a blank line before the next key
  (e.g. `params:`).
- **Blockquote statement tags need breathing room.** A standalone
  `> {% ... %}` tag line must be followed by a blank line _or_ another
  `> {%...%}` tag line. So loop/if/match bodies are separated from their tags by
  blank lines (which are consumed, not emitted).
- **Params require an explicit type.** `- x := "hi"` (no type) is rejected;
  write `- x = str := "hi"`.
- **`list(struct(...))` is rejected** in favour of the field shorthand
  `list(name = str, ...)`.
- Available **filters**: `upper`, `lower`, `trim`, `fixed(n)`, `join(sep)`,
  `limit(n)`, `add(n)`, `sub(n)`.
- Available **functions**: `len(...)`, `kind(Enum.Variant)`, `kinds(Enum)`,
  `has(option)`.

## Provenance (how these were derived)

Per the "derive, don't hand-guess" rule, **every** expectation was produced by
actually executing the current builds — nothing here is hand-written by
guessing outputs.

1. **Ground truth = the TypeScript reference build.** A generator authored the
   `source`/`params` for each case, ran it through `md-tmpl-typescript/dist`,
   and recorded the real `output` / `defaults` / error. Any case that did not
   behave as intended (e.g. an "error" case that did not throw at the declared
   phase) was reported as an authoring failure rather than silently encoded.
2. **Parity gate = the Rust core.** A second tool replayed the entire generated
   corpus through `md-tmpl-core` and asserted Rust produces the identical
   output / defaults / error-phase. Cases where the two backends disagreed were
   **excluded** from the shared corpus and recorded as
   [known divergences](#known-cross-backend-divergences) instead of being baked
   in as passing.

The derivation scripts are kept as scratch tooling (not committed in Phase 1):

- `gen.mjs` — Node ESM generator (imports the TS `dist`, writes these 4 files).
- `mdtmpl_verify/` — a standalone Rust crate that path-depends on
  `md-tmpl-core`, loads this corpus and checks Rust parity.

To regenerate + re-verify:

```bash
node gen.mjs                                   # rewrite the JSON from the TS build
cargo run --manifest-path mdtmpl_verify/Cargo.toml   # assert Rust agrees
```

## Known cross-backend divergences

Found while building this corpus (current builds, `2026-07-16`). These are
**excluded** from the shared corpus because the two backends do not agree — they
are candidate bugs for the maintainer to triage, not settled behavior:

1. **`has()` on a `None` option.** For `option(str)` with value `null`:
   TypeScript takes the `else` branch (has → false); Rust takes the `then`
   branch (has → true, rendering the body with an empty value). Observable
   output differs — a genuine behavioral bug in one backend.
2. **Default written as `Alias.Variant`** (e.g. `s = Stage := Stage.Build`):
   TypeScript accepts it and stores the literal string `"Stage.Build"`; Rust
   rejects it at compile (`invalid default value ... strings must be quoted`).
3. **Undeclared variable** (`{{ missing }}` with no such param): both error, but
   TypeScript fails at **render** while Rust fails at **compile**.
4. **Implicit-typed param** (`- x := "hello"`): both reject at **compile**, but
   with different diagnostics/parsing (TS: "must have explicit type"; Rust:
   "unknown type 'hello'").

(Two further mismatches — the exact wording of the _unused param_ and _unclosed
`{{`_ compile errors — were purely cosmetic and are covered by matching on a
shared substring, `"unused"` / `"unclosed"`, rather than the full message.)

## Scope & sequencing

- **Not yet here:** `escapes.json` (S-matrix), `comments.json` (C-matrix), and
  the full I-matrix — that behavior is mid-change and will migrate in once it
  lands.
- **Phase 2:** the Rust harness (`crates/md-tmpl-core/tests/conformance.rs`) and
  TS harness (`crates/md-tmpl-typescript/src/tests/conformance.test.ts`) that
  load these files, plus a `just test-conformance` target wiring both into CI.
  Both harnesses must fail with the offending case `name`.
