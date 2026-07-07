//! Template compilation cache for fast hot-reload.
//!
//! [`TemplateCache`] stores compiled templates keyed by `(path, content_hash)`.
//! On reload it:
//!
//! 1. Stats the file to check its modification time (cheap syscall).
//! 2. If the mtime matches the cached entry, returns the cached template —
//!    **zero file I/O beyond a stat**.
//! 3. If the mtime changed, reads the file and hashes the source.
//! 4. If the hash still matches (e.g. whitespace-only save), returns cached.
//! 5. Otherwise, compiles the new source, stores it, and returns it.
//!
//! The cache also stores compiled **include** segments so that included
//! templates are not re-read and re-compiled on every render.
//!
//! An optional LRU eviction limit ([`TemplateCache::with_max_entries`])
//! prevents unbounded memory growth in long-running processes.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Instant, SystemTime},
};

use crate::{
    compat::HashMap,
    compiled::{self, CompiledInlineTemplate, Segment},
    error::TemplateError,
    frontmatter::{self, Frontmatter},
    types::VarDecl,
};

/// A compiled include entry, ready for rendering without re-parsing.
#[derive(Debug, Clone)]
pub(crate) struct CachedInclude {
    /// Pre-compiled segment instructions.
    pub segments: Arc<[Segment]>,
    /// Declared variables from the included template's frontmatter.
    pub declarations: Arc<[VarDecl]>,
    /// Base directory for resolving nested includes.
    pub base_dir: PathBuf,
}

/// Content-hash of a source string.
///
/// Uses the shared FNV-1a implementation for deterministic, cross-version
/// stable hashing.  Same source → same hash, different source → (very
/// likely) different hash.
pub(crate) fn hash_source(source: &str) -> u64 {
    crate::__private::fnv1a_hash(source.as_bytes())
}

/// A cache entry for a compiled template.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Hash of the raw source (including frontmatter).
    source_hash: u64,
    /// File modification time at the point the source was read.
    last_modified: SystemTime,
    /// Last time this entry was accessed (for LRU eviction).
    last_accessed: Instant,
    /// Pre-compiled segment tree.
    segments: Arc<[Segment]>,
    /// Frontmatter declarations.
    declarations: Arc<[VarDecl]>,
    /// Inline template definitions.
    inline_templates: Arc<HashMap<String, CompiledInlineTemplate>>,
    /// Local constants.
    consts: Arc<HashMap<String, crate::value::Value>>,
    /// Imported constants.
    imported_consts: Arc<HashMap<String, crate::value::Value>>,
    /// Full parsed frontmatter.
    frontmatter: Frontmatter,
}

/// Internal trait for generic LRU eviction across different cache entry types.
trait HasLastAccessed {
    fn last_accessed(&self) -> Instant;
}

impl HasLastAccessed for CacheEntry {
    fn last_accessed(&self) -> Instant {
        self.last_accessed
    }
}

/// Thread-safe template compilation cache.
///
/// Caches compiled templates and includes by path + content hash to avoid
/// redundant parsing during hot-reload and rendering.
///
/// # Usage
///
/// ```rust
/// use md_tmpl_core::TemplateCache;
///
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("greeting.tmpl.md");
/// std::fs::write(
///     &path,
///     r#"---
/// params:
///   - name = str
/// ---
/// Hi {{ name }}!"#,
/// )
/// .unwrap();
///
/// let cache = TemplateCache::new();
///
/// // First load — compiles from disk.
/// let tmpl = cache.load(&path).unwrap();
///
/// // Second load of same unchanged file — returns cached, zero re-parsing.
/// let tmpl2 = cache.load(&path).unwrap();
/// assert_eq!(tmpl.source_hash(), tmpl2.source_hash());
/// ```
/// Internal trait for include resolution — erases the `BuildHasher`
/// generic so that [`Scope`](crate::scope::Scope) doesn't need to carry it.
pub(crate) trait IncludeResolver: Send + Sync {
    fn resolve_include(&self, path: &Path) -> Result<CachedInclude, TemplateError>;
}

/// A template compilation cache parameterised over a [`BuildHasher`](std::hash::BuildHasher)
/// for content-addressed invalidation.
///
/// The default hasher is [`RandomState`](std::collections::hash_map::RandomState) (SipHash-1-3). Supply a
/// different `BuildHasher` via [`with_hasher`](Self::with_hasher) if
/// you need a faster or more collision-resistant hash.
///
/// # Examples
///
/// ```
/// use md_tmpl_core::TemplateCache;
///
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("greeting.tmpl.md");
/// std::fs::write(
///     &path,
///     r#"---
/// params:
///   - name = str
/// ---
/// Hi {{ name }}!"#,
/// )
/// .unwrap();
///
/// let cache = TemplateCache::new();
/// let tmpl = cache.load(&path).unwrap();
///
/// // Second load of same unchanged file — returns cached, zero re-parsing.
/// let tmpl2 = cache.load(&path).unwrap();
/// assert_eq!(tmpl.source_hash(), tmpl2.source_hash());
/// ```
#[derive(Clone)]
pub struct TemplateCache<S: std::hash::BuildHasher = std::collections::hash_map::RandomState> {
    /// Main template cache: canonical path → entry.
    templates: Arc<RwLock<HashMap<PathBuf, CacheEntry>>>,
    /// Include cache: canonical path → compiled include.
    includes: Arc<RwLock<HashMap<PathBuf, IncludeCacheEntry>>>,
    /// Hasher builder for content-addressed cache invalidation.
    hasher: S,
    /// Optional maximum number of entries per cache map. When set and
    /// exceeded on insert, the least-recently-used entry is evicted.
    max_entries: Option<usize>,
}

impl<S: std::hash::BuildHasher> std::fmt::Debug for TemplateCache<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateCache")
            .field("template_count", &self.template_count())
            .field("include_count", &self.include_count())
            .finish()
    }
}

/// Cache entry for an included template file.
#[derive(Debug, Clone)]
struct IncludeCacheEntry {
    source_hash: u64,
    /// File modification time at the point the source was read.
    last_modified: SystemTime,
    /// Last time this entry was accessed (for LRU eviction).
    last_accessed: Instant,
    cached: CachedInclude,
}

impl HasLastAccessed for IncludeCacheEntry {
    fn last_accessed(&self) -> Instant {
        self.last_accessed
    }
}

impl Default for TemplateCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateCache {
    /// Create a new empty cache using the default content hasher (`SipHash-1-3`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            templates: Arc::new(RwLock::new(HashMap::new())),
            includes: Arc::new(RwLock::new(HashMap::new())),
            hasher: std::collections::hash_map::RandomState::new(),
            max_entries: None,
        }
    }
}

impl<S: std::hash::BuildHasher> TemplateCache<S> {
    /// Create a new empty cache with a custom [`BuildHasher`](std::hash::BuildHasher).
    ///
    /// The default uses `RandomState` (SipHash-1-3). Supply a
    /// different `BuildHasher` if you need stronger collision resistance
    /// or faster hashing (e.g. `xxHash`, `FxHash`, `AHasher`).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::{collections::hash_map::DefaultHasher, hash::BuildHasherDefault};
    ///
    /// use md_tmpl_core::TemplateCache;
    ///
    /// let cache = TemplateCache::with_hasher(BuildHasherDefault::<DefaultHasher>::default());
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let path = dir.path().join("test.tmpl.md");
    /// std::fs::write(
    ///     &path,
    ///     r#"---
    /// params: [x = str]
    /// ---
    /// {{ x }}"#,
    /// )
    /// .unwrap();
    ///
    /// let tmpl = cache.load(&path).unwrap();
    /// let mut ctx = md_tmpl_core::Context::new();
    /// ctx.set("x", "works");
    /// assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "works");
    /// ```
    #[must_use]
    pub fn with_hasher(hasher: S) -> Self {
        Self {
            templates: Arc::new(RwLock::new(HashMap::new())),
            includes: Arc::new(RwLock::new(HashMap::new())),
            hasher,
            max_entries: None,
        }
    }

    /// Set the maximum number of entries per cache map.
    ///
    /// When a new entry is inserted and the cache exceeds this limit,
    /// the least-recently-used entry is evicted. `None` (the default)
    /// disables eviction.
    ///
    /// # Examples
    ///
    /// ```
    /// use md_tmpl_core::TemplateCache;
    ///
    /// let cache = TemplateCache::new().with_max_entries(128);
    /// ```
    #[must_use]
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = Some(max);
        self
    }

    /// Hash a source string using this cache's hasher.
    fn hash_content(&self, source: &str) -> u64 {
        self.hasher.hash_one(source)
    }

    /// Load a template from file, using the cache if the source is unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Io`] if the file cannot be read, or
    /// [`TemplateError::Syntax`] if compilation fails.
    pub fn load(&self, path: &Path) -> Result<crate::Template, TemplateError> {
        self.load_inner(path, false).map(|(tmpl, _fm)| tmpl)
    }

    /// Load a template and return frontmatter too, using the cache.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] on I/O or syntax errors.
    pub fn load_with_frontmatter(
        &self,
        path: &Path,
    ) -> Result<(crate::Template, Frontmatter), TemplateError> {
        let (tmpl, fm) = self.load_inner(path, true)?;
        // `load_inner(_, true)` always returns `Some(fm)`.
        let fm = fm.ok_or_else(|| {
            TemplateError::syntax("internal error: frontmatter not returned by load_inner")
        })?;
        Ok((tmpl, fm))
    }

    fn build_template_from_entry(
        entry: &mut CacheEntry,
        base_dir: Option<PathBuf>,
        need_frontmatter: bool,
    ) -> (crate::Template, Option<Frontmatter>) {
        entry.last_accessed = Instant::now();
        let tmpl = crate::Template::from_cached(crate::template::CachedTemplateData {
            segments: entry.segments.clone(),
            declared_variables: entry.declarations.clone(),
            base_dir,
            inline_templates: entry.inline_templates.clone(),
            source_hash: entry.source_hash,
            consts: entry.consts.clone(),
            imported_consts: entry.imported_consts.clone(),
            name: entry.frontmatter.name.clone(),
            description: entry.frontmatter.description.clone(),
        });
        let fm = if need_frontmatter {
            Some(entry.frontmatter.clone())
        } else {
            None
        };
        (tmpl, fm)
    }

    /// Shared implementation: stat → mtime check → (read → hash) → cache → compile → store.
    ///
    /// When `need_frontmatter` is false, avoids cloning the cached frontmatter.
    fn load_inner(
        &self,
        path: &Path,
        need_frontmatter: bool,
    ) -> Result<(crate::Template, Option<Frontmatter>), TemplateError> {
        let canonical = std::fs::canonicalize(path)?;
        let file_mtime = std::fs::metadata(path)?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let base_dir = path.parent().map(Path::to_path_buf);

        // Fast path: if mtime matches the cached entry, skip reading the file entirely.
        {
            let mut cache = self
                .templates
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = cache.get_mut(&canonical)
                && entry.last_modified == file_mtime
            {
                return Ok(Self::build_template_from_entry(
                    entry,
                    base_dir,
                    need_frontmatter,
                ));
            }
        }

        // Mtime changed (or first load) — read file and hash.
        let source = std::fs::read_to_string(path)?;
        let source_hash = self.hash_content(&source);

        // Check if content hash still matches despite mtime change (e.g. whitespace-only save).
        {
            let mut cache = self
                .templates
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = cache.get_mut(&canonical)
                && entry.source_hash == source_hash
            {
                entry.last_modified = file_mtime;
                return Ok(Self::build_template_from_entry(
                    entry,
                    base_dir,
                    need_frontmatter,
                ));
            }
        }

        // Cache miss — compile.
        let (fm, body) = frontmatter::parse_frontmatter(&source)?;
        let body_str = body.to_string();
        let (segments, inline_templates) = compiled::compile(&body_str, &fm.type_aliases)?;

        let consts: HashMap<String, crate::value::Value> = fm
            .consts
            .iter()
            .filter_map(|d| d.default_value.clone().map(|v| (d.name.clone(), v)))
            .collect();
        let consts = Arc::new(consts);
        let imported_consts = Arc::new(fm.imported_consts.clone());

        let entry = CacheEntry {
            source_hash,
            last_modified: file_mtime,
            last_accessed: Instant::now(),
            segments: Arc::from(segments),
            declarations: Arc::from(fm.declarations.clone()),
            inline_templates: Arc::new(inline_templates),
            consts: consts.clone(),
            imported_consts: imported_consts.clone(),
            frontmatter: fm.clone(),
        };

        {
            let mut cache = self
                .templates
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Self::evict_lru(&mut cache, self.max_entries);
            cache.insert(canonical, entry.clone());
        }

        let tmpl = crate::Template::from_cached(crate::template::CachedTemplateData {
            segments: entry.segments,
            declared_variables: entry.declarations,
            base_dir,
            inline_templates: entry.inline_templates,
            source_hash,
            consts: entry.consts,
            imported_consts: entry.imported_consts,
            name: entry.frontmatter.name.clone(),
            description: entry.frontmatter.description.clone(),
        });
        Ok((tmpl, Some(fm)))
    }

    /// Resolve an include from cache or compile it from disk.
    ///
    /// Called during rendering — avoids re-reading and re-compiling
    /// included template files that haven't changed.
    fn resolve_include_impl(&self, include_path: &Path) -> Result<CachedInclude, TemplateError> {
        let canonical = std::fs::canonicalize(include_path).map_err(|err| {
            TemplateError::IncludeNotFound(format!("{}: {err}", include_path.display()))
        })?;

        let file_mtime = std::fs::metadata(include_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        // Fast path: mtime match → skip reading the file.
        {
            let mut cache = self
                .includes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = cache.get_mut(&canonical)
                && entry.last_modified == file_mtime
            {
                entry.last_accessed = Instant::now();
                return Ok(entry.cached.clone());
            }
        }

        // Mtime changed — read and hash.
        let source = std::fs::read_to_string(include_path).map_err(|err| {
            TemplateError::IncludeNotFound(format!("{}: {err}", include_path.display()))
        })?;
        let source_hash = self.hash_content(&source);

        // Content hash still matches despite mtime change?
        {
            let mut cache = self
                .includes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = cache.get_mut(&canonical)
                && entry.source_hash == source_hash
            {
                entry.last_modified = file_mtime;
                entry.last_accessed = Instant::now();
                return Ok(entry.cached.clone());
            }
        }

        // Cache miss — compile.
        let (fm, body) = frontmatter::parse_frontmatter(&source)?;
        let (segments, _inline_templates) = compiled::compile(body, &fm.type_aliases)?;
        let base_dir = include_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let cached = CachedInclude {
            segments: Arc::from(segments),
            declarations: Arc::from(fm.declarations),
            base_dir,
        };

        {
            let mut cache = self
                .includes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Self::evict_lru(&mut cache, self.max_entries);
            cache.insert(
                canonical,
                IncludeCacheEntry {
                    source_hash,
                    last_modified: file_mtime,
                    last_accessed: Instant::now(),
                    cached: cached.clone(),
                },
            );
        }

        Ok(cached)
    }

    /// Invalidate all cached entries (e.g. after a bulk file update).
    pub fn clear(&self) {
        self.templates
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.includes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    /// Number of cached main templates.
    #[must_use]
    pub fn template_count(&self) -> usize {
        self.templates
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Number of cached include templates.
    #[must_use]
    pub fn include_count(&self) -> usize {
        self.includes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Evict the oldest entries when the cache exceeds `max_entries`.
    ///
    /// Amortised: evicts down to 75% capacity in a single pass, so the
    /// O(n·log n) sort runs infrequently rather than on every insert.
    fn evict_lru<V: HasLastAccessed>(cache: &mut HashMap<PathBuf, V>, max_entries: Option<usize>) {
        let Some(max) = max_entries else { return };
        if cache.len() < max {
            return;
        }
        // Target: keep 75% of max (at least 1).
        let keep = (max * 3 / 4).max(1);
        let evict_count = cache.len().saturating_sub(keep);
        if evict_count == 0 {
            return;
        }
        // Collect and sort by last_accessed (oldest first).
        let mut entries: Vec<_> = cache
            .iter()
            .map(|(k, v)| (k.clone(), v.last_accessed()))
            .collect();
        entries.sort_unstable_by_key(|(_, t)| *t);
        // Remove the oldest `evict_count` entries.
        for (key, _) in entries.into_iter().take(evict_count) {
            cache.remove(&key);
        }
    }
}

impl<S: std::hash::BuildHasher + Send + Sync> IncludeResolver for TemplateCache<S> {
    fn resolve_include(&self, path: &Path) -> Result<CachedInclude, TemplateError> {
        self.resolve_include_impl(path)
    }
}

#[cfg(test)]
mod tests {
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
        let c1 = cache.resolve_include(&path).unwrap();
        let c2 = cache.resolve_include(&path).unwrap();

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
        if let Ok(tmpl) = cache.load(path) {
            let mut ctx = crate::Context::new();
            ctx.set("x", "hello");
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
                                run_renderer_thread(
                                    &cache,
                                    path,
                                    expected_idx,
                                    &successful_renders,
                                );
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
}
