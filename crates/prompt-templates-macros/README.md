# prompt-templates-macros

Proc macros for compile-time template validation, pre-parsing, and typed
parameter struct generation for
[prompt-templates](https://github.com/domenukk/prompt-templates).

## Macros

### `include_template!`

Reads, parses, and validates a `.tmpl.md` file at compile time. Emits a
module with the pre-compiled template, typed parameter struct, sub-structs,
constants, and type aliases.

```rust
use prompt_templates_macros::include_template;

include_template!("prompts/simple_greeting.tmpl.md");

let output = simple_greeting::Params { name: "World".into() }
    .render()
    .unwrap();
assert_eq!(output, "\nHello World!\n");
```

Override the module name:

```rust
use prompt_templates_macros::include_template;

include_template!("prompts/simple_greeting.tmpl.md" => my_greet);

let output = my_greet::Params { name: "Alice".into() }
    .render()
    .unwrap();
assert_eq!(output, "\nHello Alice!\n");
```

#### Generated module contents

- **`pub fn template() -> &'static Template`** — pre-compiled template singleton.
- **`pub struct Params { ... }`** — typed parameter struct with:
  - `render()` — render using the embedded template.
  - `render_with(tmpl)` — render with an externally-loaded template (hot-reload).
  - `validate_template(tmpl)` — check template compatibility.
  - `to_context()` — convert to a `Context`.
- Sub-structs for compound types (e.g. `ParamsItemsItem`).
- Constants from the `consts:` block.
- Type aliases / enums from the `types:` block.

### `template!`

Like `include_template!`, but for inline template strings. The
`=> module_name` is required.

```rust
prompt_templates_macros::template!(r#"
---
params:
  - name = str
---
Hello {{ name }}!
"# => greeting);

let output = greeting::Params { name: "World".into() }
    .render()
    .unwrap();
assert_eq!(output, "Hello World!\n");
```

## Hot-Reload with Type Safety

Combine compile-time types with runtime loading:

```rust
use prompt_templates::Template;

prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");

// Load from disk at runtime:
let tmpl = Template::from_file(std::path::Path::new("prompts/greeting.tmpl.md")).unwrap();

// Validate the reloaded file hasn't diverged:
greeting::Params::validate_template(&tmpl).unwrap();

// Render with the disk-loaded template:
let output = greeting::Params {
    name: "Bob".to_string(),
    count: 1,
    items: vec![],
}.render_with(&tmpl).unwrap();
```

## Type Mapping

| Frontmatter Type            | Rust Type                                            |
| :-------------------------- | :--------------------------------------------------- |
| `str`                       | `String`                                             |
| `int`                       | `i64`                                                |
| `float`                     | `f64`                                                |
| `bool`                      | `bool`                                               |
| `list<field = type, ...>`   | `Vec<Params{Field}Item>` (auto-generated sub-struct) |
| `struct<field = type, ...>` | `Params{Field}` (auto-generated sub-struct)          |
| `enum<Variant, ...>`        | `Params{Field}` (auto-generated enum)                |
| `option<T>`                 | `Option<RustType>`                                   |
| `tmpl<field = type, ...>`   | `Params{Field}` (template callable)                  |
