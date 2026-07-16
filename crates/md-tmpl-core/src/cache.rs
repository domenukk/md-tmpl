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
    value::Value,
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
    /// Local constants from the included template's frontmatter.
    pub consts: HashMap<String, Value>,
    /// Imported constants (e.g. from `imports:` in frontmatter).
    pub imported_consts: HashMap<String, Value>,
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
    /// Resolve (and cache) an included template file.
    ///
    /// `env` carries the compile-time environment values propagated from the
    /// including template so that the include's `env:` frontmatter resolves to
    /// the same values it would on the uncached render path. Because those
    /// values are baked into the cached result (as injected constants), they
    /// participate in cache invalidation.
    fn resolve_include(
        &self,
        path: &Path,
        env: &[(String, Value)],
    ) -> Result<CachedInclude, TemplateError>;
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
    /// Hash of the compile-time env values baked into `cached`. Two renders
    /// with different env must not share a cached result, so this participates
    /// in invalidation alongside `source_hash`/`last_modified`.
    env_hash: u64,
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

    /// Hash the compile-time env values for include-cache invalidation.
    ///
    /// [`Value`] intentionally does not implement [`Hash`] (it can contain
    /// floats), so we hash a deterministic textual rendering instead. The env
    /// is tiny (a handful of scalar entries) and only hashed on include
    /// resolution, so the formatting cost is negligible.
    fn hash_env(&self, env: &[(String, Value)]) -> u64 {
        use std::fmt::Write as _;
        let mut buf = String::new();
        for (name, value) in env {
            // Writing to a `String` via `fmt::Write` cannot fail; surface the
            // impossible error rather than silently discarding the Result.
            write!(buf, "{name}={value:?}\u{1f}").expect("writing to a String is infallible");
        }
        self.hash_content(&buf)
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
    fn resolve_include_impl(
        &self,
        include_path: &Path,
        env: &[(String, Value)],
    ) -> Result<CachedInclude, TemplateError> {
        let canonical = std::fs::canonicalize(include_path).map_err(|err| {
            TemplateError::IncludeNotFound(format!("{}: {err}", include_path.display()))
        })?;

        let file_mtime = std::fs::metadata(include_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        // Env values are baked into the cached result, so a change in env must
        // invalidate the entry even when the file itself is untouched.
        let env_hash = self.hash_env(env);

        // Fast path: mtime + env match → skip reading the file.
        {
            let mut cache = self
                .includes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = cache.get_mut(&canonical)
                && entry.last_modified == file_mtime
                && entry.env_hash == env_hash
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

        // Content hash still matches (and same env) despite mtime change?
        {
            let mut cache = self
                .includes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = cache.get_mut(&canonical)
                && entry.source_hash == source_hash
                && entry.env_hash == env_hash
            {
                entry.last_modified = file_mtime;
                entry.last_accessed = Instant::now();
                return Ok(entry.cached.clone());
            }
        }

        // Cache miss — compile.
        let base_dir = include_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        // Propagate compile-time env so the include's `env:` frontmatter
        // resolves to the same values as on the uncached render path.
        let env_pairs: Vec<(&str, Value)> =
            env.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        let (fm, body) =
            frontmatter::parse_frontmatter_with_base_dir(&source, &base_dir, &env_pairs)?;
        let (segments, _inline_templates) = compiled::compile(body, &fm.type_aliases)?;

        let mut include_consts = HashMap::new();
        for d in &fm.consts {
            if let Some(v) = d.default_value.clone() {
                include_consts.insert(d.name.clone(), v);
            }
        }
        // Inject resolved env values as constants.
        for d in &fm.env {
            if let Some(ref v) = d.default_value {
                include_consts
                    .entry(d.name.clone())
                    .or_insert_with(|| v.clone());
            }
        }

        let cached = CachedInclude {
            segments: Arc::from(segments),
            declarations: Arc::from(fm.declarations),
            base_dir,
            consts: include_consts,
            imported_consts: fm.imported_consts,
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
                    env_hash,
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
    fn resolve_include(
        &self,
        path: &Path,
        env: &[(String, Value)],
    ) -> Result<CachedInclude, TemplateError> {
        self.resolve_include_impl(path, env)
    }
}

#[cfg(test)]
mod tests;
