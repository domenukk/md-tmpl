"""Tests for md_tmpl TemplateCache, cached rendering, Env, and context managers."""

import textwrap
from pathlib import Path

import pytest

from md_tmpl import (
    Template,
    TemplateCache,
)

# ---------------------------------------------------------------------------
# TemplateCache
# ---------------------------------------------------------------------------


class TestCache:
    """Tests for TemplateCache."""

    def test_cache_load(self, simple_template_path: Path) -> None:
        cache = TemplateCache()
        tmpl = cache.load(str(simple_template_path))
        assert tmpl.render(name="cached") == "Hello cached!"

    def test_cache_returns_same_hash(self, simple_template_path: Path) -> None:
        cache = TemplateCache()
        t1 = cache.load(str(simple_template_path))
        t2 = cache.load(str(simple_template_path))
        assert t1.source_hash() == t2.source_hash()


# ---------------------------------------------------------------------------
# TemplateCache management methods
# ---------------------------------------------------------------------------


class TestCacheManagement:
    """Tests for TemplateCache.clear(), template_count(), include_count()."""

    def test_template_count(self, simple_template_path: Path) -> None:
        cache = TemplateCache()
        assert cache.template_count() == 0
        cache.load(str(simple_template_path))
        assert cache.template_count() == 1

    def test_include_count_starts_at_zero(self) -> None:
        cache = TemplateCache()
        assert cache.include_count() == 0

    def test_clear_resets_counts(self, simple_template_path: Path) -> None:
        cache = TemplateCache()
        cache.load(str(simple_template_path))
        assert cache.template_count() == 1
        cache.clear()
        assert cache.template_count() == 0


# ---------------------------------------------------------------------------
# render_cached()
# ---------------------------------------------------------------------------


class TestRenderCached:
    """Tests for cache-aware rendering."""

    def test_render_cached_basic(self, tmp_path: Path) -> None:
        path = tmp_path / "hello.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            Hello {{ name }}!"""))
        cache = TemplateCache()
        tmpl = cache.load(path)
        result = tmpl.render_cached(cache, name="cached")
        assert result == "Hello cached!"

    def test_render_cached_with_includes(self, tmp_path: Path) -> None:
        inc = tmp_path / "_header.tmpl.md"
        inc.write_text(textwrap.dedent("""\
            ---
            params: [title = str]
            ---
            HEADER: {{ title }}"""))
        main = tmp_path / "page.tmpl.md"
        main.write_text("""---
params:
  - title = str
---
> {% include [header](./_header.tmpl.md) with title=title %}

Body here.""")
        cache = TemplateCache()
        tmpl = cache.load(main)
        result = tmpl.render_cached(cache, title="Test")
        assert "HEADER: Test" in result
        assert "Body here." in result

    def test_render_cached_include_resolves_compile_env(self, tmp_path: Path) -> None:
        """Regression: a cached include whose ``env:`` var has no default must
        resolve from the *parent's* compile-time env — identical to the uncached
        path. Before the core fix the cached resolver parsed the include with an
        empty env and raised "no value provided and no default". Python exercises
        the same ``render_ctx_cached`` core path via the PyO3 binding.
        """
        (tmp_path / "child.tmpl.md").write_text(textwrap.dedent("""\
            ---
            name: child
            env: [PROMPTS_DIR = str]
            params: []
            ---
            dir={{ PROMPTS_DIR }}"""))
        main = tmp_path / "main.tmpl.md"
        main.write_text(textwrap.dedent("""\
            ---
            params: []
            ---
            > {% include [child](./child.tmpl.md) %}"""))
        cache = TemplateCache()
        tmpl = Template.from_source_with_options(
            main.read_text(),
            base_dir=str(tmp_path),
            env={"PROMPTS_DIR": "/prompts"},
        )
        # First render compiles the include from disk with env threaded in.
        out1 = tmpl.render_cached(cache)
        assert "dir=/prompts" in out1
        # Second render is served from cache; the env-injected const persists.
        assert tmpl.render_cached(cache) == out1

    def test_render_cached_include_invalidated_on_env_change(
        self, tmp_path: Path
    ) -> None:
        """Regression: changing the compile-time env must invalidate a cached
        include even though the file's mtime and content are unchanged (which
        would otherwise hit the cache fast path)."""
        (tmp_path / "child.tmpl.md").write_text(textwrap.dedent("""\
            ---
            name: child
            env: [PROMPTS_DIR = str]
            params: []
            ---
            dir={{ PROMPTS_DIR }}"""))
        main = tmp_path / "main.tmpl.md"
        main.write_text(textwrap.dedent("""\
            ---
            params: []
            ---
            > {% include [child](./child.tmpl.md) %}"""))
        main_src = main.read_text()
        cache = TemplateCache()

        def compile_with(value: str) -> Template:
            return Template.from_source_with_options(
                main_src, base_dir=str(tmp_path), env={"PROMPTS_DIR": value}
            )

        assert "dir=/alpha" in compile_with("/alpha").render_cached(cache)
        # Same file, same cache — must recompute for the new env, not serve /alpha.
        assert "dir=/beta" in compile_with("/beta").render_cached(cache)

    def test_render_cached_dict(self, tmp_path: Path) -> None:
        path = tmp_path / "hello.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            Hello {{ name }}!"""))
        cache = TemplateCache()
        tmpl = cache.load(path)
        result = tmpl.render_cached_dict({"name": "dict_cached"}, cache)
        assert result == "Hello dict_cached!"


# ---------------------------------------------------------------------------
# TemplateCache max_entries and __len__
# ---------------------------------------------------------------------------


class TestCacheMaxEntries:
    """Tests for TemplateCache max_entries and __len__."""

    def test_default_no_limit(self) -> None:
        cache = TemplateCache()
        assert len(cache) == 0

    def test_cache_len_tracks_entries(self, tmp_path: Path) -> None:
        path1 = tmp_path / "a.tmpl.md"
        path1.write_text(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        path2 = tmp_path / "b.tmpl.md"
        path2.write_text(textwrap.dedent("""\
            ---
            params: [y = str]
            ---
            {{ y }}"""))
        cache = TemplateCache()
        assert len(cache) == 0
        cache.load(path1)
        assert len(cache) >= 1
        cache.load(path2)
        assert len(cache) >= 2

    def test_max_entries_constructor(self) -> None:
        cache = TemplateCache(max_entries=10)
        assert len(cache) == 0

    def test_max_entries_eviction(self, tmp_path: Path) -> None:
        cache = TemplateCache(max_entries=2)
        paths = []
        for i in range(4):
            p = tmp_path / f"t{i}.tmpl.md"
            p.write_text(textwrap.dedent(f"""\
                ---
                params: [x = str]
                ---
                Template {i}: {{{{ x }}}}"""))
            paths.append(p)
        for p in paths:
            cache.load(p)
        assert cache.template_count() <= 2


# ---------------------------------------------------------------------------
# Concurrent rendering (ported from Go)
# ---------------------------------------------------------------------------


class TestConcurrentRendering:
    """Tests for thread-safety of template rendering."""

    def test_concurrent_render(self) -> None:
        """Multiple threads rendering the same template concurrently."""
        import concurrent.futures

        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [id = int]
            ---
            Result: {{ id }}"""))

        def render_id(i: int) -> str:
            return tmpl.render(id=i)

        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            futures = [pool.submit(render_id, i) for i in range(20)]
            results = [f.result() for f in futures]

        for i in range(20):
            assert f"Result: {i}" in results

    def test_concurrent_render_different_templates(self, tmp_path: Path) -> None:
        """Different templates rendered concurrently."""
        import concurrent.futures

        paths = []
        for i in range(5):
            p = tmp_path / f"t{i}.tmpl.md"
            p.write_text(textwrap.dedent(f"""\
                ---
                params: [x = str]
                ---
                Template {i}: {{{{ x }}}}"""))
            paths.append(p)

        templates = [Template.from_file(str(p)) for p in paths]

        def render(idx: int) -> str:
            return templates[idx].render(x="val")

        with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
            futures = [pool.submit(render, i) for i in range(5)]
            results = [f.result() for f in futures]

        for i in range(5):
            assert f"Template {i}: val" in results


# ---------------------------------------------------------------------------
# render_cached allow_extra code paths
# ---------------------------------------------------------------------------


class TestRenderCachedAllowExtra:
    """Tests for the allow_extra=True path in render_cached and render_cached_dict."""

    def test_render_cached_allow_extra(self, tmp_path: Path) -> None:
        path = tmp_path / "hello.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            Hello {{ name }}!"""))
        cache = TemplateCache()
        tmpl = cache.load(path)
        result = tmpl.render_cached(
            cache, name="test", extra="ignored", allow_extra=True
        )
        assert result == "Hello test!"

    def test_render_cached_dict_allow_extra(self, tmp_path: Path) -> None:
        path = tmp_path / "hello.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            Hello {{ name }}!"""))
        cache = TemplateCache()
        tmpl = cache.load(path)
        result = tmpl.render_cached_dict(
            {"name": "dict_test", "extra": "ignored"}, cache, allow_extra=True
        )
        assert result == "Hello dict_test!"

    def test_render_cached_rejects_extra_by_default(self, tmp_path: Path) -> None:
        path = tmp_path / "hello.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            Hello {{ name }}!"""))
        cache = TemplateCache()
        tmpl = cache.load(path)
        with pytest.raises(ValueError, match="extra|undeclared|not declared"):
            tmpl.render_cached(cache, name="test", bogus="bad")


# ---------------------------------------------------------------------------
# Cache with includes — include count tracking
# ---------------------------------------------------------------------------


class TestCacheIncludeCounting:
    """Tests for TemplateCache include counting."""

    def test_include_count_after_load(self, tmp_path: Path) -> None:
        inc = tmp_path / "header.tmpl.md"
        inc.write_text(textwrap.dedent("""\
            ---
            params: [title = str]
            ---
            # {{ title }}"""))
        main = tmp_path / "page.tmpl.md"
        main.write_text(textwrap.dedent("""\
            ---
            params:
              - title = str
            ---
            > {% include [header](./header.tmpl.md) with title=title %}

            Body."""))
        cache = TemplateCache()
        tmpl = cache.load(main)
        # Render to trigger include resolution
        tmpl.render_cached(cache, title="Test")
        assert cache.template_count() >= 1
        # include_count should be non-negative (implementation may vary)
        assert cache.include_count() >= 0


# ---------------------------------------------------------------------------
# Context-manager protocol
# ---------------------------------------------------------------------------


class TestContextManager:
    """Tests for __enter__ / __exit__ context-manager support."""

    def test_template_context_manager(self) -> None:
        with Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}""")) as tmpl:
            result = tmpl.render(x="hello")
            assert result == "hello"

    def test_template_context_manager_exception(self) -> None:
        """__exit__ should not suppress exceptions."""
        with pytest.raises(ValueError):
            with Template.from_source(textwrap.dedent("""\
                ---
                params: [x = str]
                ---
                {{ x }}""")) as tmpl:
                raise ValueError("test error")

    def test_cache_context_manager(self) -> None:
        with TemplateCache() as cache:
            assert len(cache) == 0


# ---------------------------------------------------------------------------
# Env: compile-time environment variables
# ---------------------------------------------------------------------------


class TestEnv:
    """Tests for compile-time env: declarations via from_source_with_env / from_source_with_options."""

    def test_basic_env_str_substitution(self) -> None:
        """Env var of type str is rendered in the template body."""
        source = textwrap.dedent("""\
            ---
            env: [MODEL = str]

            params: []
            ---
            Model: {{ MODEL }}""")
        tmpl = Template.from_source_with_env(source, {"MODEL": "gpt-4"})
        assert tmpl.render_empty() == "Model: gpt-4"

    def test_env_default_used(self) -> None:
        """Env var with default uses default when not provided."""
        source = textwrap.dedent("""\
            ---
            env:
              - MODEL = str := "gpt-3.5"

            params: []
            ---
            Model: {{ MODEL }}""")
        tmpl = Template.from_source_with_env(source, {})
        assert tmpl.render_empty() == "Model: gpt-3.5"

    def test_env_default_overridden(self) -> None:
        """Env var with default is overridden when provided."""
        source = textwrap.dedent("""\
            ---
            env:
              - MODEL = str := "gpt-3.5"

            params: []
            ---
            Model: {{ MODEL }}""")
        tmpl = Template.from_source_with_env(source, {"MODEL": "gpt-4o"})
        assert tmpl.render_empty() == "Model: gpt-4o"

    def test_missing_required_env_raises(self) -> None:
        """Missing required env var (no default) raises ValueError at compile time."""
        source = textwrap.dedent("""\
            ---
            env: [REQUIRED_VAR = str]

            params: []
            ---
            {{ REQUIRED_VAR }}""")
        with pytest.raises(ValueError, match="no value provided and no default"):
            Template.from_source_with_env(source, {})

    def test_env_coexists_with_params(self) -> None:
        """Env vars and params coexist; both are accessible in the template body."""
        source = textwrap.dedent("""\
            ---
            env: [PREFIX = str]

            params: [name = str]
            ---
            {{ PREFIX }}/{{ name }}""")
        tmpl = Template.from_source_with_env(source, {"PREFIX": "/opt/prompts"})
        assert tmpl.render(name="agent_x") == "/opt/prompts/agent_x"

    def test_env_int_type(self) -> None:
        """Env var of type int is parsed and rendered correctly."""
        source = textwrap.dedent("""\
            ---
            env: [MAX_RETRIES = int]

            params: []
            ---
            Retries: {{ MAX_RETRIES }}""")
        tmpl = Template.from_source_with_env(source, {"MAX_RETRIES": "5"})
        assert tmpl.render_empty() == "Retries: 5"

    def test_env_via_from_source_with_options(self) -> None:
        """from_source_with_options(env=...) works the same as from_source_with_env."""
        source = textwrap.dedent("""\
            ---
            env: [GREETING = str]

            params: []
            ---
            {{ GREETING }}""")
        tmpl = Template.from_source_with_options(source, env={"GREETING": "Hi"})
        assert tmpl.render_empty() == "Hi"


# ---------------------------------------------------------------------------
# render_cached_json (renamed from render_json_cached)
# ---------------------------------------------------------------------------


class TestRenderCachedJson:
    """Tests for the renamed render_cached_json method."""

    def _cached_template(self, tmp_path: Path) -> tuple[Template, TemplateCache]:
        path = tmp_path / "greeting.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        cache = TemplateCache()
        tmpl = cache.load(str(path))
        return tmpl, cache

    def test_render_cached_json_basic(self, tmp_path: Path) -> None:
        tmpl, cache = self._cached_template(tmp_path)
        assert tmpl.render_cached_json('{"name": "json"}', cache) == "Hello json!"

    def test_render_cached_json_allow_extra(self, tmp_path: Path) -> None:
        tmpl, cache = self._cached_template(tmp_path)
        result = tmpl.render_cached_json(
            '{"name": "json", "extra": true}', cache, allow_extra=True
        )
        assert result == "Hello json!"

    def test_render_cached_json_rejects_extra_by_default(self, tmp_path: Path) -> None:
        tmpl, cache = self._cached_template(tmp_path)
        with pytest.raises(ValueError, match="extra|undeclared|not declared"):
            tmpl.render_cached_json('{"name": "json", "extra": true}', cache)

    def test_render_cached_json_matches_render_json(self, tmp_path: Path) -> None:
        tmpl, cache = self._cached_template(tmp_path)
        payload = '{"name": "Alice"}'
        assert tmpl.render_cached_json(payload, cache) == tmpl.render_json(payload)

    def test_old_render_json_cached_name_is_gone(self, tmp_path: Path) -> None:
        """The pre-rename name must no longer exist (no backward-compat alias)."""
        tmpl, _cache = self._cached_template(tmp_path)
        assert not hasattr(tmpl, "render_json_cached")
        assert not hasattr(Template, "render_json_cached")

    def test_render_cached_json_is_stub_visible(self) -> None:
        assert callable(getattr(Template, "render_cached_json"))


# ---------------------------------------------------------------------------
# Context managers are documented no-ops (still functional)
# ---------------------------------------------------------------------------


class TestContextManagerNoOps:
    """Confirm the documented no-op context managers keep `with` working."""

    def test_template_enter_returns_self(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        with tmpl as entered:
            assert entered is tmpl

    def test_template_exit_does_not_suppress(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        with pytest.raises(RuntimeError, match="boom"):
            with tmpl:
                raise RuntimeError("boom")

    def test_cache_enter_returns_self(self) -> None:
        cache = TemplateCache()
        with cache as entered:
            assert entered is cache

    def test_cache_exit_does_not_clear_entries(self, tmp_path: Path) -> None:
        """A no-op __exit__ must NOT drop cached entries."""
        path = tmp_path / "greeting.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        cache = TemplateCache()
        with cache:
            cache.load(str(path))
            assert len(cache) == 1
        # Entries remain after exiting the `with` block (no-op __exit__).
        assert len(cache) == 1
