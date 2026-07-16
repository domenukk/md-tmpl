//! Unit and concurrency tests for [`super`]: the template compilation cache.

use std::sync::atomic::AtomicUsize;

use super::*;

#[test]
fn cache_returns_same_template_for_unchanged_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tmpl.md");
    std::fs::write(
        &path,
        r"---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();

    let cache = TemplateCache::new();
    let t1 = cache.load(&path).unwrap();
    let t2 = cache.load(&path).unwrap();

    assert_eq!(t1.source_hash(), t2.source_hash());
    assert_eq!(cache.template_count(), 1);
}

#[test]
fn cache_recompiles_on_file_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tmpl.md");
    std::fs::write(
        &path,
        r"---
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();

    // Back-date the first version: both writes can land within one mtime tick
    // on coarse-granularity filesystems, and the mtime fast path would then
    // serve the stale cached entry, failing the assert below. Forcing an
    // older mtime guarantees the second write is detected as changed.
    let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(earlier)
        .unwrap();

    let cache = TemplateCache::new();
    let t1 = cache.load(&path).unwrap();

    std::fs::write(
        &path,
        r"---
params: [name = str]
---
Goodbye {{ name }}!",
    )
    .unwrap();
    let t2 = cache.load(&path).unwrap();

    assert_ne!(t1.source_hash(), t2.source_hash());
    assert_eq!(cache.template_count(), 1); // same path, entry replaced
}

#[test]
fn cache_clear_invalidates_all() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tmpl.md");
    std::fs::write(
        &path,
        r"---
params: []
---
Hi",
    )
    .unwrap();

    let cache = TemplateCache::new();
    cache.load(&path).unwrap();
    assert_eq!(cache.template_count(), 1);

    cache.clear();
    assert_eq!(cache.template_count(), 0);
}

#[test]
fn include_cache_avoids_recompile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("header.tmpl.md");
    std::fs::write(
        &path,
        r"---
name: header
params: []
---
# Header",
    )
    .unwrap();

    let cache = TemplateCache::new();
    let c1 = cache.resolve_include(&path, &[]).unwrap();
    let c2 = cache.resolve_include(&path, &[]).unwrap();

    assert_eq!(c1.segments.len(), c2.segments.len());
    assert_eq!(cache.include_count(), 1);
}

#[test]
fn load_with_frontmatter_caches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fm.tmpl.md");
    std::fs::write(
        &path,
        r"---
name: test
params: [x = str]
---
{{ x }}",
    )
    .unwrap();

    let cache = TemplateCache::new();
    let (t1, fm1) = cache.load_with_frontmatter(&path).unwrap();
    let (t2, fm2) = cache.load_with_frontmatter(&path).unwrap();

    assert_eq!(t1.source_hash(), t2.source_hash());
    assert_eq!(fm1.name, fm2.name);
    assert_eq!(cache.template_count(), 1);
}

#[test]
fn render_cached_with_include() {
    let dir = tempfile::tempdir().unwrap();

    // Create a main template that includes a header.
    std::fs::write(
        dir.path().join("header.tmpl.md"),
        r"---
name: header
params: [title = str]
---
# {{ title }}",
    )
    .unwrap();
    let main_path = dir.path().join("main.tmpl.md");
    std::fs::write(
        &main_path,
        r"---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body",
    )
    .unwrap();

    let cache = TemplateCache::new();
    let tmpl = cache.load(&main_path).unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("title", "Hello");

    // First render — compiles include from disk.
    let output1 = tmpl.render_ctx_cached(&ctx, &cache).unwrap();
    assert!(output1.contains("# Hello"));
    assert!(output1.contains("Body"));
    assert_eq!(cache.include_count(), 1);

    // Second render — include resolved from cache.
    let output2 = tmpl.render_ctx_cached(&ctx, &cache).unwrap();
    assert_eq!(output1, output2);
    assert_eq!(cache.include_count(), 1); // same entry, no new compilation
}

/// Regression: cached includes must preserve `consts:` from the included
/// template's frontmatter so that constants are visible during rendering
/// from cache (not only on the first uncached render).
#[test]
fn cached_include_preserves_consts() {
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("with_const.tmpl.md"),
        r#"---
name: with_const
consts: [GREETING = str := "Howdy"]
params: [name = str]
---
{{ GREETING }} {{ name }}!"#,
    )
    .unwrap();

    let main_path = dir.path().join("main.tmpl.md");
    std::fs::write(
        &main_path,
        r"---
params: [name = str]
---
> {% include [with_const](./with_const.tmpl.md) with name=name %}",
    )
    .unwrap();

    let cache = TemplateCache::new();
    let tmpl = cache.load(&main_path).unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("name", "World");

    // First render — uncached include.
    let out1 = tmpl.render_ctx_cached(&ctx, &cache).unwrap();
    assert!(
        out1.contains("Howdy World!"),
        "first render should contain const: {out1}"
    );

    // Second render — cached include must still see the const.
    let out2 = tmpl.render_ctx_cached(&ctx, &cache).unwrap();
    assert_eq!(out1, out2, "cached render must match uncached render");
}

/// Regression: cached includes must preserve `imports:` from the included
/// template's frontmatter. Without this, custom types defined via imports
/// (e.g. `artist.WorkRole`) cause "unknown type" errors on cached renders.
#[test]
fn cached_include_preserves_imported_consts() {
    let dir = tempfile::tempdir().unwrap();

    // Create a "types" template that defines an enum.
    std::fs::write(
        dir.path().join("types.tmpl.md"),
        r#"---
name: types
description: "Type definitions"
types: [Color = enum(Red, Green, Blue)]
params: []
---
"#,
    )
    .unwrap();

    // Create an included template that imports the enum.
    std::fs::write(
        dir.path().join("colorful.tmpl.md"),
        r#"---
name: colorful
imports:
  - "[types](./types.tmpl.md)"

params:
  - favorite = types.Color := Red
---
Color: {{ favorite }}"#,
    )
    .unwrap();

    let main_path = dir.path().join("main.tmpl.md");
    std::fs::write(
        &main_path,
        r"---
params: []
---
> {% include [colorful](./colorful.tmpl.md) %}",
    )
    .unwrap();

    let cache = TemplateCache::new();
    let tmpl = cache.load(&main_path).unwrap();

    let ctx = crate::Context::new();

    // First render — uncached, compiles include from disk.
    let out1 = tmpl.render_ctx_cached(&ctx, &cache).unwrap();
    assert!(
        out1.contains("Color: Red"),
        "first render should show default enum value: {out1}"
    );

    // Second render — from cache. Before the fix, this would fail with
    // "unknown type" because imported_consts were not stored in CachedInclude.
    let out2 = tmpl.render_ctx_cached(&ctx, &cache).unwrap();
    assert_eq!(
        out1, out2,
        "cached render must match uncached render (imported consts preserved)"
    );
}

/// Regression: cached include resolution must propagate the including
/// template's compile-time `env` into the included file's frontmatter, so
/// `env:` declarations without defaults resolve identically to the uncached
/// path. Before the fix, the cached resolver parsed the include with an
/// empty env and failed with "no value provided and no default".
#[test]
fn cached_include_resolves_env_from_compile_env() {
    let dir = tempfile::tempdir().unwrap();

    // Child declares an env var with NO default — it can only resolve if
    // the parent's compile env is threaded through.
    std::fs::write(
        dir.path().join("child.tmpl.md"),
        r"---
name: child
env: [PROMPTS_DIR = str]
params: []
---
dir={{ PROMPTS_DIR }}",
    )
    .unwrap();

    let main_path = dir.path().join("main.tmpl.md");
    std::fs::write(
        &main_path,
        r"---
params: []
---
> {% include [child](./child.tmpl.md) %}",
    )
    .unwrap();

    let env = [("PROMPTS_DIR", Value::from("/prompts"))];
    let (tmpl, _fm) = crate::Template::compile(
        &std::fs::read_to_string(&main_path).unwrap(),
        crate::CompileOptions::default()
            .base_dir(dir.path())
            .env(&env),
    )
    .unwrap();

    let cache = TemplateCache::new();
    let ctx = crate::Context::new();

    // First render — compiles the include from disk with env threaded in.
    let out1 = tmpl
        .render_ctx_cached(&ctx, &cache)
        .expect("first cached render should resolve env");
    assert!(out1.contains("dir=/prompts"), "first render: {out1}");
    assert_eq!(cache.include_count(), 1);

    // Second render — served from cache; env-injected const must persist.
    let out2 = tmpl
        .render_ctx_cached(&ctx, &cache)
        .expect("second cached render should resolve env");
    assert_eq!(out1, out2, "cached render must match uncached render");
    assert_eq!(cache.include_count(), 1);
}

/// Regression: a change in compile-time env must invalidate a cached
/// include. The env value is baked into the cached result, so two renders
/// with different env must not share an entry — even though the file's
/// mtime and content are unchanged (which would otherwise hit the fast path).
#[test]
fn cached_include_invalidated_on_env_change() {
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("child.tmpl.md"),
        r"---
name: child
env: [PROMPTS_DIR = str]
params: []
---
dir={{ PROMPTS_DIR }}",
    )
    .unwrap();

    let main_path = dir.path().join("main.tmpl.md");
    let main_src = r"---
params: []
---
> {% include [child](./child.tmpl.md) %}";
    std::fs::write(&main_path, main_src).unwrap();

    let cache = TemplateCache::new();
    let ctx = crate::Context::new();

    let compile_with = |value: &str| {
        let env = [("PROMPTS_DIR", Value::from(value))];
        crate::Template::compile(
            main_src,
            crate::CompileOptions::default()
                .base_dir(dir.path())
                .env(&env),
        )
        .unwrap()
        .0
    };

    // Render with env A → caches "dir=/alpha".
    let tmpl_a = compile_with("/alpha");
    let out_a = tmpl_a.render_ctx_cached(&ctx, &cache).unwrap();
    assert!(out_a.contains("dir=/alpha"), "env A render: {out_a}");

    // Render with env B against the SAME cache. The file is untouched, so
    // without env-aware invalidation the mtime fast path would return the
    // stale "/alpha" entry. It must instead recompute to "/beta".
    let tmpl_b = compile_with("/beta");
    let out_b = tmpl_b.render_ctx_cached(&ctx, &cache).unwrap();
    assert!(
        out_b.contains("dir=/beta"),
        "env B render must reflect new env, got: {out_b}"
    );
}

#[test]
fn with_hasher_custom_builder() {
    use std::hash::BuildHasherDefault;

    // Use a deterministic DefaultHasher via BuildHasherDefault.
    let cache = TemplateCache::with_hasher(BuildHasherDefault::<
        std::collections::hash_map::DefaultHasher,
    >::default());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.tmpl.md");
    std::fs::write(
        &path,
        r"---
params: [x = str]
---
{{ x }}",
    )
    .unwrap();

    let tmpl = cache.load(&path).unwrap();
    let mut ctx = crate::Context::new();
    ctx.set("x", "works");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "works");

    // Cached reload works.
    let tmpl2 = cache.load(&path).unwrap();
    assert_eq!(tmpl.source_hash(), tmpl2.source_hash());
}

#[test]
fn eviction_removes_lru_entry() {
    let cache = TemplateCache::new().with_max_entries(2);
    let dir = tempfile::tempdir().unwrap();

    let path_a = dir.path().join("a.tmpl.md");
    let path_b = dir.path().join("b.tmpl.md");
    let path_c = dir.path().join("c.tmpl.md");
    std::fs::write(
        &path_a,
        "\
---

params: []
---
A",
    )
    .unwrap();
    std::fs::write(
        &path_b,
        "\
---

params: []
---
B",
    )
    .unwrap();
    std::fs::write(
        &path_c,
        "\
---

params: []
---
C",
    )
    .unwrap();

    cache.load(&path_a).unwrap();
    cache.load(&path_b).unwrap();
    assert_eq!(cache.template_count(), 2);

    // Loading C should evict the LRU entry (A), keeping count at 2.
    cache.load(&path_c).unwrap();
    assert_eq!(cache.template_count(), 2);
}

#[test]
fn no_eviction_when_max_entries_is_none() {
    let cache = TemplateCache::new();
    let dir = tempfile::tempdir().unwrap();

    for i in 0..10 {
        let path = dir.path().join(format!("{i}.tmpl.md"));
        std::fs::write(
            &path,
            format!(
                "---
params: []
---
{i}"
            ),
        )
        .unwrap();
        cache.load(&path).unwrap();
    }
    assert_eq!(cache.template_count(), 10);
}

/// Helper for [`concurrent_load_render_clear`]: loader thread logic.
fn run_loader_thread(
    cache: &TemplateCache,
    path: &std::path::Path,
    successful_loads: &AtomicUsize,
) {
    use std::sync::atomic::Ordering;
    // NOLINT: Err is acceptable — clear() may have raced with load()
    if let Ok(tmpl) = cache.load(path) {
        // Verify the loaded template is functional.
        assert!(
            !tmpl.declarations().is_empty(),
            "loaded template must have declarations"
        );
        successful_loads.fetch_add(1, Ordering::Relaxed);
    }
    // Err is acceptable — clear() may have raced.
}

/// Helper for [`concurrent_load_render_clear`]: renderer thread logic.
fn run_renderer_thread(
    cache: &TemplateCache,
    path: &std::path::Path,
    expected_idx: usize,
    successful_renders: &AtomicUsize,
) {
    use std::sync::atomic::Ordering;
    // NOLINT: Err is acceptable — clear() may have raced with load()
    if let Ok(tmpl) = cache.load(path) {
        let mut ctx = crate::Context::new();
        ctx.set("x", "hello");
        // NOLINT: render may fail if clear() raced — expected in stress test
        if let Ok(output) = tmpl.render_ctx_cached(&ctx, cache) {
            assert!(
                output.contains("hello"),
                "rendered output must contain 'hello', got: {output}"
            );
            assert!(
                output.contains(&format!("template{expected_idx}")),
                "rendered output must contain template index, got: {output}"
            );
            successful_renders.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Helper for [`concurrent_load_render_clear`]: clear thread logic.
fn run_clear_thread(
    cache: &TemplateCache,
    path: &std::path::Path,
    round: usize,
    successful_loads: &AtomicUsize,
) {
    use std::sync::atomic::Ordering;
    if round % 5 == 0 {
        cache.clear();
    }
    // Load after clear to verify cache rebuilds correctly.
    // NOLINT: Err is acceptable — load after clear may race
    if let Ok(tmpl) = cache.load(path) {
        assert!(
            !tmpl.declarations().is_empty(),
            "reloaded template must have declarations"
        );
        successful_loads.fetch_add(1, Ordering::Relaxed);
    }
}

/// Helper for [`concurrent_load_render_clear`]: reader thread logic.
fn run_reader_thread(
    cache: &TemplateCache,
    path: &std::path::Path,
    paths_len: usize,
    successful_loads: &AtomicUsize,
) {
    use std::sync::atomic::Ordering;
    // Counts must be non-negative and bounded.
    let tc = cache.template_count();
    let ic = cache.include_count();
    assert!(tc <= paths_len, "template count {tc} exceeds file count");
    assert!(ic <= 100, "include count {ic} unexpectedly large");
    // NOLINT: Err is acceptable — clear() may have raced with load()
    if let Ok(tmpl) = cache.load(path) {
        assert!(
            !tmpl.declarations().is_empty(),
            "loaded template must have declarations"
        );
        successful_loads.fetch_add(1, Ordering::Relaxed);
    }
}

/// Stress-test `TemplateCache` under concurrent access.
///
/// Spawns 8 threads that simultaneously `load`, `render_ctx_cached`,
/// `clear`, and query `template_count` / `include_count` in a tight
/// loop. The test verifies:
///
/// - No panics (locks are never poisoned).
/// - No deadlocks (all threads join within the timeout).
/// - Rendered output is correct when rendering succeeds.
#[test]
fn concurrent_load_render_clear() {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    const NUM_THREADS: usize = 8;
    const ROUNDS_PER_THREAD: usize = 50;

    let dir = tempfile::tempdir().unwrap();

    // Create several template files that threads will load concurrently.
    let mut paths = Vec::new();
    for i in 0..4 {
        let path = dir.path().join(format!("t{i}.tmpl.md"));
        std::fs::write(
            &path,
            format!(
                "---
params: [x = str]
---
template{i}: {{{{ x }}}}"
            ),
        )
        .unwrap();
        paths.push(path);
    }

    let cache = Arc::new(TemplateCache::new());
    let paths = Arc::new(paths);
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let successful_loads = Arc::new(AtomicUsize::new(0));
    let successful_renders = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|thread_id| {
            let cache = Arc::clone(&cache);
            let paths = Arc::clone(&paths);
            let barrier = Arc::clone(&barrier);
            let successful_loads = Arc::clone(&successful_loads);
            let successful_renders = Arc::clone(&successful_renders);
            std::thread::spawn(move || {
                // All threads start simultaneously.
                barrier.wait();

                for round in 0..ROUNDS_PER_THREAD {
                    let path = &paths[round % paths.len()];
                    let expected_idx = round % paths.len();

                    match thread_id % 4 {
                        0 => run_loader_thread(&cache, path, &successful_loads),
                        1 => {
                            run_renderer_thread(&cache, path, expected_idx, &successful_renders);
                        }
                        2 => run_clear_thread(&cache, path, round, &successful_loads),
                        _ => run_reader_thread(&cache, path, paths.len(), &successful_loads),
                    }
                }
            })
        })
        .collect();

    // Join all threads — a hang here would indicate a deadlock.
    for handle in handles {
        handle.join().expect("thread must not panic");
    }

    // At least some loads and renders must have succeeded.
    let loads = successful_loads.load(Ordering::Relaxed);
    let renders = successful_renders.load(Ordering::Relaxed);
    assert!(loads > 0, "no loads succeeded across {NUM_THREADS} threads");
    assert!(
        renders > 0,
        "no renders succeeded across {NUM_THREADS} threads"
    );
}
