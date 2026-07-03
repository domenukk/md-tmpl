//! Validate that every YAML/markdown code block in SPEC.md and every README.md
//! compiles (or at least parses) correctly.
//!
//! This prevents documentation drift: if a SPEC example doesn't compile with the
//! current parser, this test fails — catching bugs where the spec says one thing
//! but the implementation does another.
//!
//! **Strategy:**
//!
//! 1. Extract all fenced `yaml` and `markdown` code blocks that contain `---`
//!    frontmatter delimiters.
//! 2. **Standalone blocks** (no `imports:` referencing external files, no
//!    `{% tmpl %}` wrappers) must compile via `Template::compile`.
//! 3. **Import-dependent blocks** (containing `imports:` or inline `{% tmpl %}`)
//!    cannot resolve external files in isolation, but their YAML frontmatter must
//!    still be valid — verified via `serde_yaml`.
//! 4. All frontmatter YAML is cross-validated against `serde_yaml` to ensure it's
//!    standards-compliant YAML.

use md_tmpl::{CompileOptions, Template};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Describes what kind of block we found.
#[derive(Debug, PartialEq)]
enum BlockKind {
    /// Can be compiled standalone (no external imports or inline templates).
    Standalone,
    /// References external files via `imports:` — can only validate frontmatter.
    HasImports,
    /// Uses `{% tmpl %}` wrapper — inline template block, not a top-level template.
    InlineTemplate,
}

/// A code block extracted from a documentation file.
#[derive(Debug)]
struct DocBlock {
    file: &'static str,
    start_line: usize,
    end_line: usize,
    source: String,
    kind: BlockKind,
}

/// Extract all compilable code blocks from a markdown file's contents.
fn extract_blocks(content: &str, file: &'static str) -> Vec<DocBlock> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("```yaml") || line.starts_with("```markdown") {
            let start_line = i + 1; // 1-indexed
            let mut block_lines = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].trim() != "```" {
                block_lines.push(lines[i]);
                i += 1;
            }
            let end_line = i + 1; // 1-indexed

            let block_content = block_lines.join("\n");

            // Only process blocks that have --- frontmatter delimiters
            let dash_count = block_lines.iter().filter(|l| l.trim() == "---").count();
            if dash_count < 2 {
                i += 1;
                continue;
            }

            // Determine kind
            let kind = if block_content.contains("{% tmpl ") {
                BlockKind::InlineTemplate
            } else if block_content.contains("imports:") {
                BlockKind::HasImports
            } else {
                BlockKind::Standalone
            };

            // Strip leading comments (e.g., "# base.tmpl.md") before first ---
            let mut source = block_content.clone();
            if source.starts_with('#') {
                if let Some(pos) = source.find("\n---") {
                    source = source[pos + 1..].to_string();
                }
            }

            blocks.push(DocBlock {
                file,
                start_line,
                end_line,
                source,
                kind,
            });
        }
        i += 1;
    }

    blocks
}

/// Extract the raw YAML content between `---` delimiters from a template source.
fn extract_yaml_frontmatter(source: &str) -> String {
    let mut in_frontmatter = false;
    let mut yaml_lines = Vec::new();
    for line in source.lines() {
        if line.trim() == "---" {
            if in_frontmatter {
                break;
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            yaml_lines.push(line);
        }
    }
    yaml_lines.join("\n")
}

// ---------------------------------------------------------------------------
// SPEC.md tests
// ---------------------------------------------------------------------------

#[test]
fn spec_standalone_blocks_compile() {
    let content = include_str!("../../../SPEC.md");
    let blocks = extract_blocks(content, "SPEC.md");

    let standalone: Vec<&DocBlock> = blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Standalone)
        .collect();

    assert!(
        !standalone.is_empty(),
        "should find at least one standalone block in SPEC.md"
    );

    let mut failures = Vec::new();
    for block in &standalone {
        if let Err(e) =
            Template::compile(&block.source, CompileOptions::default().allow_unused(true))
        {
            failures.push(format!(
                "  {}:{}-{}: {}\n    source: {:?}",
                block.file,
                block.start_line,
                block.end_line,
                e,
                &block.source[..block.source.len().min(100)]
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "standalone SPEC.md blocks failed to compile:\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn spec_import_blocks_have_valid_yaml() {
    let content = include_str!("../../../SPEC.md");
    let blocks = extract_blocks(content, "SPEC.md");

    let import_blocks: Vec<&DocBlock> = blocks
        .iter()
        .filter(|b| b.kind == BlockKind::HasImports)
        .collect();

    assert!(
        !import_blocks.is_empty(),
        "should find at least one import-dependent block in SPEC.md"
    );

    let mut failures = Vec::new();
    for block in &import_blocks {
        // These can't fully compile (unresolvable imports), but their
        // YAML frontmatter must be valid.
        let yaml = extract_yaml_frontmatter(&block.source);
        if yaml.is_empty() {
            continue;
        }
        if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(&yaml) {
            failures.push(format!(
                "  {}:{}-{}: {}\n    yaml: {:?}",
                block.file,
                block.start_line,
                block.end_line,
                e,
                &yaml[..yaml.len().min(100)]
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "import-dependent SPEC.md blocks have invalid YAML:\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn spec_all_yaml_is_serde_valid() {
    let content = include_str!("../../../SPEC.md");
    let blocks = extract_blocks(content, "SPEC.md");

    let mut failures = Vec::new();
    for block in &blocks {
        if block.kind == BlockKind::InlineTemplate {
            continue; // Inline templates don't start with ---
        }

        // Extract the YAML between --- delimiters
        let lines: Vec<&str> = block.source.lines().collect();
        let mut in_frontmatter = false;
        let mut yaml_lines = Vec::new();
        for line in &lines {
            if line.trim() == "---" {
                if in_frontmatter {
                    break; // End of frontmatter
                }
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter {
                yaml_lines.push(*line);
            }
        }

        if yaml_lines.is_empty() {
            continue;
        }

        let yaml_block = yaml_lines.join("\n");
        if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(&yaml_block) {
            failures.push(format!(
                "  {}:{}-{}: {}\n    yaml: {:?}",
                block.file,
                block.start_line,
                block.end_line,
                e,
                &yaml_block[..yaml_block.len().min(100)]
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "SPEC.md YAML blocks are not valid YAML:\n{}",
        failures.join("\n\n")
    );
}

// ---------------------------------------------------------------------------
// README.md tests (all READMEs in the project)
// ---------------------------------------------------------------------------

macro_rules! readme_test {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let content = include_str!($path);
            let blocks = extract_blocks(content, $path);

            let standalone: Vec<&DocBlock> = blocks
                .iter()
                .filter(|b| b.kind == BlockKind::Standalone)
                .collect();

            let mut failures = Vec::new();
            for block in &standalone {
                if let Err(e) =
                    Template::compile(&block.source, CompileOptions::default().allow_unused(true))
                {
                    failures.push(format!(
                        "  {}:{}-{}: {}\n    source: {:?}",
                        block.file,
                        block.start_line,
                        block.end_line,
                        e,
                        &block.source[..block.source.len().min(100)]
                    ));
                }
            }

            assert!(
                failures.is_empty(),
                "standalone README blocks failed to compile:\n{}",
                failures.join("\n\n")
            );

            // Import-dependent blocks: validate YAML
            let import_blocks: Vec<&DocBlock> = blocks
                .iter()
                .filter(|b| b.kind == BlockKind::HasImports)
                .collect();

            let mut fm_failures = Vec::new();
            for block in &import_blocks {
                let yaml = extract_yaml_frontmatter(&block.source);
                if yaml.is_empty() {
                    continue;
                }
                if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(&yaml) {
                    fm_failures.push(format!(
                        "  {}:{}-{}: {}\n    yaml: {:?}",
                        block.file,
                        block.start_line,
                        block.end_line,
                        e,
                        &yaml[..yaml.len().min(100)]
                    ));
                }
            }

            assert!(
                fm_failures.is_empty(),
                "import-dependent README blocks have invalid frontmatter:\n{}",
                fm_failures.join("\n\n")
            );
        }
    };
}

readme_test!(readme_root_blocks_compile, "../../../README.md");
readme_test!(
    readme_macros_blocks_compile,
    "../../md-tmpl-macros/README.md"
);
readme_test!(
    readme_python_blocks_compile,
    "../../md-tmpl-python/README.md"
);
readme_test!(
    readme_typescript_blocks_compile,
    "../../md-tmpl-typescript/README.md"
);
readme_test!(readme_wasm_blocks_compile, "../../md-tmpl-wasm/README.md");
readme_test!(readme_go_blocks_compile, "../../../go/md_tmpl/README.md");
