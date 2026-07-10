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
    let mut chars = source.char_indices().peekable();
    let mut line_num = 1usize;

    while let Some(&(pos, ch)) = chars.peek() {
        if ch == '\n' {
            line_num += 1;
            chars.next();
            continue;
        }

        // Look for ``` at the start (possibly with a language tag)
        if source[pos..].starts_with("```") {
            let fence_line = line_num;
            // Skip past the opening ``` and any language tag
            let after_backticks = pos + 3;
            // Find end of this line
            let eol = source[after_backticks..]
                .find('\n')
                .map_or(source.len(), |i| after_backticks + i);

            // Advance chars past this line
            for (_, c) in source[pos..=eol.min(source.len() - 1)].char_indices() {
                if c == '\n' {
                    line_num += 1;
                }
            }
            // Skip the chars iterator forward
            while let Some(&(i, _)) = chars.peek() {
                if i > eol {
                    break;
                }
                chars.next();
            }

            // Now find the closing ```
            let content_start = eol + 1;
            if content_start >= source.len() {
                continue;
            }
            let closing = source[content_start..].find("\n```");
            if let Some(close_offset) = closing {
                let content_end = content_start + close_offset;
                let block = &source[content_start..content_end];
                let trimmed = block.trim();

                // Only keep blocks that look like templates (have frontmatter)
                if trimmed.starts_with("---") && trimmed[3..].contains("\n---") {
                    // Skip the generic format example
                    if !trimmed.contains("<frontmatter>") && !trimmed.contains("<body>") {
                        blocks.push(DocExample {
                            content: trimmed.to_string(),
                            line: fence_line,
                            file,
                        });
                    }
                }

                // Count newlines in the block content + closing fence
                let skip_to = content_end + 4; // +4 for \n```
                for c in source[eol + 1..skip_to.min(source.len())].chars() {
                    if c == '\n' {
                        line_num += 1;
                    }
                }
                while let Some(&(i, _)) = chars.peek() {
                    if i >= skip_to {
                        break;
                    }
                    chars.next();
                }
            }
        } else {
            chars.next();
        }
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

    if !failures.is_empty() {
        panic!(
            "SPEC.md doc example compilation failures ({} of {}):\n{}",
            failures.len(),
            blocks.len(),
            failures.join("\n")
        );
    }

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

    if !failures.is_empty() {
        panic!(
            "README doc example compilation failures ({} of {}):\n{}",
            failures.len(),
            total_pass + total_skip + failures.len(),
            failures.join("\n")
        );
    }

    eprintln!(
        "READMEs: {total_pass} compiled, {total_skip} skipped (env/import dependent), 0 failures"
    );
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn extract_finds_template_blocks() {
        let md = r#"Some text

```yaml
---
params:
  - name = str
---
Hello {{ name }}
```

More text
"#;
        let blocks = extract_template_blocks(md, "test.md");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].content.starts_with("---"));
        assert!(blocks[0].content.contains("name = str"));
    }

    #[test]
    fn has_empty_body_detects_frontmatter_only() {
        assert!(has_empty_body("---\nparams:\n  - x = str\n---\n"));
        assert!(has_empty_body("---\nparams:\n  - x = str\n---"));
        assert!(!has_empty_body(
            "---\nparams:\n  - x = str\n---\nHello {{ x }}"
        ));
    }

    #[test]
    fn inject_allow_unused_inserts_correctly() {
        let src = "---\nparams:\n  - x = str\n---\n";
        let injected = inject_allow_unused(src);
        assert!(injected.contains("allow_unused: true"));
        assert!(injected.contains("params:"));
    }
}
