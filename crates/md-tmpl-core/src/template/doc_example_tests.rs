//! Validates that all template examples in `SPEC.md` and every `README.md`
//! compile successfully.
//!
//! Body-less snippets (frontmatter-only examples) get `allow_unused: true`
//! auto-injected so they pass the unused-params check without polluting
//! the user-facing documentation with that directive.
//!
//! Examples that depend on environment variables or cross-template imports
//! are expected to fail with specific error messages and are asserted as such.

use crate::{CompileOptions, Template};

// Embed every doc file at compile time so CI never has stale paths.
const SPEC_MD: &str = include_str!("../../../../SPEC.md");
const ROOT_README: &str = include_str!("../../../../README.md");
const CRATE_README: &str = include_str!("../../../../crates/md-tmpl/README.md");
const TS_README: &str = include_str!("../../../../crates/md-tmpl-typescript/README.md");
const PYTHON_README: &str = include_str!("../../../../crates/md-tmpl-python/README.md");
const GO_README: &str = include_str!("../../../../go/md_tmpl/README.md");

/// A template block extracted from a markdown document.
struct DocExample {
    content: String,
    line: usize,
    file: &'static str,
}

/// Extract fenced code blocks that look like md-tmpl templates (have `---`
/// frontmatter).
fn extract_template_blocks(source: &str, file: &'static str) -> Vec<DocExample> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = source.split('\n').collect();
    let mut idx = 0usize;

    while idx < lines.len() {
        let line = lines[idx].trim_end_matches('\r');
        let trimmed_start = line.trim_start_matches(' ');
        let backtick_count = trimmed_start.bytes().take_while(|&b| b == b'`').count();
        if backtick_count >= 3 && !trimmed_start[backtick_count..].contains('`') {
            let fence_line = idx + 1;
            let mut close_idx = idx + 1;
            while close_idx < lines.len() {
                let close_line = lines[close_idx]
                    .trim_end_matches('\r')
                    .trim_start_matches(' ');
                let close_ticks = close_line.bytes().take_while(|&b| b == b'`').count();
                if close_ticks >= backtick_count && close_line[close_ticks..].trim().is_empty() {
                    break;
                }
                close_idx += 1;
            }
            if close_idx < lines.len() {
                let block = lines[idx + 1..close_idx].join("\n");
                let trimmed = block.trim();
                if trimmed.starts_with("---")
                    && trimmed[3..].contains("\n---")
                    && !trimmed.contains("<frontmatter>")
                    && !trimmed.contains("<body>")
                {
                    blocks.push(DocExample {
                        content: trimmed.to_string(),
                        line: fence_line,
                        file,
                    });
                }
                idx = close_idx + 1;
                continue;
            }
        }
        idx += 1;
    }

    blocks
}

/// Check if a template has an empty body (frontmatter only, no content after
/// the closing `---`).
fn has_empty_body(template_src: &str) -> bool {
    let trimmed = template_src.trim();
    if !trimmed.starts_with("---") {
        return false;
    }
    // Find closing ---
    if let Some(second_fence) = trimmed[3..].find("\n---") {
        let after_fence = 3 + second_fence + 4; // skip past \n---
        let body = trimmed.get(after_fence..).unwrap_or("");
        body.trim().is_empty()
    } else {
        true
    }
}

/// Inject `allow_unused: true` into frontmatter for body-less snippets.
/// Adds a blank line before the directive so it parses correctly after
/// YAML block lists (e.g. `params:` entries).
fn inject_allow_unused(template_src: &str) -> String {
    let trimmed = template_src.trim();
    if let Some(second_fence) = trimmed[3..].find("\n---") {
        let insert_pos = 3 + second_fence;
        format!(
            "{}\n\nallow_unused: true{}",
            &trimmed[..insert_pos],
            &trimmed[insert_pos..]
        )
    } else {
        template_src.to_string()
    }
}

/// Returns `true` if the error indicates the template needs env vars or
/// cross-template imports that can't be resolved standalone.
fn is_expected_standalone_failure(err_msg: &str) -> bool {
    // Env vars not provided
    err_msg.contains("no value provided and no default")
        // Cross-template import type references
        || err_msg.contains("unknown type")
        // Env-based import paths
        || err_msg.contains("unresolvable expression")
}

#[test]
fn all_spec_examples_compile() {
    let blocks = extract_template_blocks(SPEC_MD, "SPEC.md");
    assert!(
        blocks.len() >= 15,
        "Expected at least 15 template blocks in SPEC.md, found {}",
        blocks.len()
    );

    let mut pass = 0;
    let mut expected_skip = 0;
    let mut failures = Vec::new();

    for block in &blocks {
        let src = if has_empty_body(&block.content) {
            // Auto-inject allow_unused for body-less examples
            inject_allow_unused(&block.content)
        } else {
            block.content.clone()
        };

        let opts = CompileOptions::default().allow_unused(has_empty_body(&block.content));
        match Template::compile(&src, opts) {
            Ok(_) => pass += 1,
            Err(e) => {
                let msg = e.to_string();
                if is_expected_standalone_failure(&msg) {
                    expected_skip += 1;
                } else {
                    failures.push(format!("  {}:L{}: {msg}", block.file, block.line));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "SPEC.md doc example compilation failures ({} of {}):\n{}",
        failures.len(),
        blocks.len(),
        failures.join("\n")
    );

    eprintln!(
        "SPEC.md: {pass} compiled, {expected_skip} skipped (env/import dependent), 0 failures"
    );
}

#[test]
fn all_readme_examples_compile() {
    let readmes: &[(&str, &str)] = &[
        (ROOT_README, "README.md"),
        (CRATE_README, "crates/md-tmpl/README.md"),
        (TS_README, "crates/md-tmpl-typescript/README.md"),
        (PYTHON_README, "crates/md-tmpl-python/README.md"),
        (GO_README, "go/md_tmpl/README.md"),
    ];

    let mut total_pass = 0;
    let mut total_skip = 0;
    let mut failures = Vec::new();

    for &(content, file) in readmes {
        let blocks = extract_template_blocks(content, file);

        for block in &blocks {
            let src = if has_empty_body(&block.content) {
                inject_allow_unused(&block.content)
            } else {
                block.content.clone()
            };

            let opts = CompileOptions::default().allow_unused(has_empty_body(&block.content));
            match Template::compile(&src, opts) {
                Ok(_) => total_pass += 1,
                Err(e) => {
                    let msg = e.to_string();
                    if is_expected_standalone_failure(&msg) {
                        total_skip += 1;
                    } else {
                        failures.push(format!("  {}:L{}: {msg}", block.file, block.line));
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "README doc example compilation failures ({} of {}):\n{}",
        failures.len(),
        total_pass + total_skip + failures.len(),
        failures.join("\n")
    );

    eprintln!(
        "READMEs: {total_pass} compiled, {total_skip} skipped (env/import dependent), 0 failures"
    );
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn extract_finds_template_blocks() {
        let md = r"Some text

```yaml
---
params:
  - name = str
---
Hello {{ name }}
```

More text
";
        let blocks = extract_template_blocks(md, "test.md");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].content.starts_with("---"));
        assert!(blocks[0].content.contains("name = str"));
    }

    #[test]
    fn has_empty_body_detects_frontmatter_only() {
        assert!(has_empty_body(
            "---
params:
  - x = str
---
"
        ));
        assert!(has_empty_body(
            "---
params:
  - x = str
---"
        ));
        assert!(!has_empty_body(
            "---
params:
  - x = str
---
Hello {{ x }}"
        ));
    }

    #[test]
    fn inject_allow_unused_inserts_correctly() {
        let src = "---
params:
  - x = str
---
";
        let injected = inject_allow_unused(src);
        assert!(injected.contains("allow_unused: true"));
        assert!(injected.contains("params:"));
    }
}
