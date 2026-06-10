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
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Instant, SystemTime},
};

use crate::{
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
/// Used for change-detection in hot-reload scenarios: same source →
/// same hash, different source → (very likely) different hash.
pub(crate) fn hash_source(source: &str) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault};
    // Deterministic: BuildHasherDefault<DefaultHasher> always uses the same
    // seed, unlike RandomState which re-seeds per instance.
    BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default().hash_one(source)
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
/// use prompt_templates::TemplateCache;
///
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("greeting.tmpl.md");
/// std::fs::write(&path, "---\nparams:\n  - name = str\n---\nHi {{ name }}!").unwrap();
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
/// use prompt_templates::TemplateCache;
///
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("greeting.tmpl.md");
/// std::fs::write(&path, "---\nparams:\n  - name = str\n---\nHi {{ name }}!").unwrap();
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
    /// use prompt_templates::TemplateCache;
    ///
    /// let cache = TemplateCache::with_hasher(BuildHasherDefault::<DefaultHasher>::default());
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let path = dir.path().join("test.tmpl.md");
    /// std::fs::write(&path, "---\nparams: [x = str]\n---\n{{ x }}").unwrap();
    ///
    /// let tmpl = cache.load(&path).unwrap();
    /// let mut ctx = prompt_templates::Context::new();
    /// ctx.set("x", "works");
    /// assert_eq!(tmpl.render(&ctx).unwrap(), "works");
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
    /// use prompt_templates::TemplateCache;
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
        // SAFETY: `load_inner(_, true)` always populates `fm`.
        debug_assert!(fm.is_some(), "load_inner(_, true) must return Some(fm)");
        Ok((tmpl, fm.unwrap_or_default()))
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
                .expect("template cache lock poisoned");
            if let Some(entry) = cache.get_mut(&canonical)
                && entry.last_modified == file_mtime
            {
                entry.last_accessed = Instant::now();
                let tmpl = crate::Template::from_cached(
                    entry.segments.clone(),
                    entry.declarations.clone(),
                    base_dir,
                    entry.inline_templates.clone(),
                    entry.source_hash,
                );
                let fm = if need_frontmatter {
                    Some(entry.frontmatter.clone())
                } else {
                    None
                };
                return Ok((tmpl, fm));
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
                .expect("template cache lock poisoned");
            if let Some(entry) = cache.get_mut(&canonical)
                && entry.source_hash == source_hash
            {
                entry.last_modified = file_mtime;
                entry.last_accessed = Instant::now();
                let tmpl = crate::Template::from_cached(
                    entry.segments.clone(),
                    entry.declarations.clone(),
                    base_dir,
                    entry.inline_templates.clone(),
                    source_hash,
                );
                let fm = if need_frontmatter {
                    Some(entry.frontmatter.clone())
                } else {
                    None
                };
                return Ok((tmpl, fm));
            }
        }

        // Cache miss — compile.
        let (fm, body) = frontmatter::parse_frontmatter(&source)?;
        let body_str = body.to_string();
        let (segments, inline_templates) = compiled::compile(&body_str)?;

        let entry = CacheEntry {
            source_hash,
            last_modified: file_mtime,
            last_accessed: Instant::now(),
            segments: Arc::from(segments),
            declarations: Arc::from(fm.declarations.clone()),
            inline_templates: Arc::new(inline_templates),
            frontmatter: fm.clone(),
        };

        {
            let mut cache = self
                .templates
                .write()
                .expect("template cache lock poisoned");
            Self::evict_lru(&mut cache, self.max_entries);
            cache.insert(canonical, entry.clone());
        }

        let tmpl = crate::Template::from_cached(
            entry.segments,
            entry.declarations,
            base_dir,
            entry.inline_templates,
            source_hash,
        );
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
            let mut cache = self.includes.write().expect("include cache lock poisoned");
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
            let mut cache = self.includes.write().expect("include cache lock poisoned");
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
        let (segments, _inline_templates) = compiled::compile(body)?;
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
            let mut cache = self.includes.write().expect("include cache lock poisoned");
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
    ///
    /// # Panics
    ///
    /// Panics if a cache lock is poisoned.
    pub fn clear(&self) {
        self.templates
            .write()
            .expect("template cache lock poisoned")
            .clear();
        self.includes
            .write()
            .expect("include cache lock poisoned")
            .clear();
    }

    /// Number of cached main templates.
    ///
    /// # Panics
    ///
    /// Panics if the cache lock is poisoned.
    #[must_use]
    pub fn template_count(&self) -> usize {
        self.templates
            .read()
            .expect("template cache lock poisoned")
            .len()
    }

    /// Number of cached include templates.
    ///
    /// # Panics
    ///
    /// Panics if the cache lock is poisoned.
    #[must_use]
    pub fn include_count(&self) -> usize {
        self.includes
            .read()
            .expect("include cache lock poisoned")
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
    use super::*;

    #[test]
    fn cache_returns_same_template_for_unchanged_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.tmpl.md");
        std::fs::write(&path, "---\nparams: [name = str]\n---\nHello {{ name }}!").unwrap();

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
        std::fs::write(&path, "---\nparams: [name = str]\n---\nHello {{ name }}!").unwrap();

        let cache = TemplateCache::new();
        let t1 = cache.load(&path).unwrap();

        std::fs::write(&path, "---\nparams: [name = str]\n---\nGoodbye {{ name }}!").unwrap();
        let t2 = cache.load(&path).unwrap();

        assert_ne!(t1.source_hash(), t2.source_hash());
        assert_eq!(cache.template_count(), 1); // same path, entry replaced
    }

    #[test]
    fn cache_clear_invalidates_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.tmpl.md");
        std::fs::write(&path, "---\nparams: []\n---\nHi").unwrap();

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
        std::fs::write(&path, "---\nname: header\nparams: []\n---\n# Header").unwrap();

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
        std::fs::write(&path, "---\nname: test\nparams: [x = str]\n---\n{{ x }}").unwrap();

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
            "---\nname: header\nparams: [title = str]\n---\n# {{ title }}",
        )
        .unwrap();
        let main_path = dir.path().join("main.tmpl.md");
        std::fs::write(
            &main_path,
            "---\nparams: [title = str]\n---\n> {% include [header](header.tmpl.md) with title=title %}\nBody",
        )
        .unwrap();

        let cache = TemplateCache::new();
        let tmpl = cache.load(&main_path).unwrap();

        let mut ctx = crate::Context::new();
        ctx.set("title", "Hello");

        // First render — compiles include from disk.
        let output1 = tmpl.render_cached(&ctx, &cache).unwrap();
        assert!(output1.contains("# Hello"));
        assert!(output1.contains("Body"));
        assert_eq!(cache.include_count(), 1);

        // Second render — include resolved from cache.
        let output2 = tmpl.render_cached(&ctx, &cache).unwrap();
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
        std::fs::write(&path, "---\nparams: [x = str]\n---\n{{ x }}").unwrap();

        let tmpl = cache.load(&path).unwrap();
        let mut ctx = crate::Context::new();
        ctx.set("x", "works");
        assert_eq!(tmpl.render(&ctx).unwrap(), "works");

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
        std::fs::write(&path_a, "---\nparams: []\n---\nA").unwrap();
        std::fs::write(&path_b, "---\nparams: []\n---\nB").unwrap();
        std::fs::write(&path_c, "---\nparams: []\n---\nC").unwrap();

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
            std::fs::write(&path, format!("---\nparams: []\n---\n{i}")).unwrap();
            cache.load(&path).unwrap();
        }
        assert_eq!(cache.template_count(), 10);
    }
}
