"""Tests for md_tmpl template construction, loading, and metadata."""

import textwrap
from pathlib import Path

import pytest

from md_tmpl import (
    Template,
    TemplateCache,
    load_types,
    template,
    TemplateSyntaxError,
    MissingParamsError,
    TypeMismatchError,
)

# ---------------------------------------------------------------------------
# Template.from_source — basic rendering
# ---------------------------------------------------------------------------


class TestFromSource:
    """Tests for Template.from_source()."""

    def test_basic_render(self) -> None:
        tmpl = Template.from_source("""---
params:
  - name = str
---
Hello {{ name }}!""")
        assert tmpl.render(name="world") == "Hello world!"

    def test_int_param(self) -> None:
        tmpl = Template.from_source("""---
params:
  - count = int
---
Count: {{ count }}""")
        assert tmpl.render(count=42) == "Count: 42"

    def test_bool_param(self) -> None:
        tmpl = Template.from_source("""---
params: [flag = bool]
---
> {% if flag %}

yes

> {% /if %}""")
        assert tmpl.render(flag=True) == "yes\n"

    def test_float_param(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [score = float]
            ---
            {{ score }}"""))
        assert tmpl.render(score=3.14) == "3.14"

    def test_syntax_error_raises(self) -> None:
        with pytest.raises(ValueError, match="frontmatter"):
            Template.from_source("no frontmatter at all")

    def test_missing_param_raises(self) -> None:
        tmpl = Template.from_source("""---
params: [name = str, age = int]
---
{{ name }} {{ age }}""")
        with pytest.raises(MissingParamsError) as excinfo:
            tmpl.render(name="Alice")
        assert excinfo.value.kind == "missing_params"
        assert excinfo.value.missing == ["age"]

    def test_type_mismatch_raises(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [flag = bool]
            ---
            {{ flag }}"""))
        with pytest.raises(TypeMismatchError) as excinfo:
            tmpl.render(flag="not a bool")
        assert excinfo.value.kind == "type_mismatch"
        assert excinfo.value.path == "flag"
        assert excinfo.value.expected == "bool"
        assert excinfo.value.actual == "str"


# ---------------------------------------------------------------------------
# Template.from_file
# ---------------------------------------------------------------------------


class TestFromFile:
    """Tests for Template.from_file()."""

    def test_load_and_render(self, simple_template_path: Path) -> None:
        tmpl = Template.from_file(str(simple_template_path))
        assert tmpl.render(name="world") == "Hello world!"

    def test_missing_file_raises(self) -> None:
        with pytest.raises(ValueError, match="failed to load template"):
            Template.from_file("/nonexistent/path.tmpl.md")


# ---------------------------------------------------------------------------
# Template.from_source_allowing_unused
# ---------------------------------------------------------------------------


class TestFromSourceAllowingUnused:
    """Tests for Template.from_source_allowing_unused()."""

    def test_unused_param_accepted(self) -> None:
        """Params declared but not referenced in the body should be accepted."""
        tmpl = Template.from_source_allowing_unused("""---
params: [name = str, unused = int]
---
Hello {{ name }}!""")
        assert tmpl.render(name="world", unused=42) == "Hello world!"

    def test_unused_param_rejected_in_strict_mode(self) -> None:
        """from_source() should reject unused params."""
        with pytest.raises(ValueError):
            Template.from_source("""---
params: [name = str, unused = int]
---
Hello {{ name }}!""")

    def test_undeclared_variable_in_body_rejected(self) -> None:
        """Referencing a variable declared nowhere is a compile-time error.

        Mirrors the Rust core's ``check_undeclared_variables``: the error is
        raised at construction (``from_source``), not at render time.
        """
        with pytest.raises(TemplateSyntaxError, match="undeclared variable"):
            Template.from_source("""---
params:
  - x = str
---
{{ x }} {{ y }}""")

    def test_undeclared_variable_in_condition_rejected(self) -> None:
        """Undeclared variables inside an if condition are compile-time errors."""
        with pytest.raises(TemplateSyntaxError, match="undeclared variable"):
            Template.from_source("""---
params:
  - x = int
---
> {% if y > 0 %}{{ x }}{% /if %}""")


# ---------------------------------------------------------------------------
# Template metadata
# ---------------------------------------------------------------------------


class TestMetadata:
    """Tests for template metadata methods."""

    def test_declarations(self) -> None:
        tmpl = Template.from_source("""---
params: [name = str, count = int]
---
{{ name }} {{ count }}""")
        decls = tmpl.declarations()
        names = [d[0] for d in decls]
        assert "name" in names
        assert "count" in names

    def test_declarations_types(self) -> None:
        tmpl = Template.from_source("""---
params: [name = str, count = int]
---
{{ name }} {{ count }}""")
        decls = tmpl.declarations()
        type_map = {d[0]: d[1] for d in decls}
        assert type_map["name"] == "str"
        assert type_map["count"] == "int"

    def test_source_hash_stable(self) -> None:
        source = """---
params: [x = str]
---
{{ x }}"""
        t1 = Template.from_source(source)
        t2 = Template.from_source(source)
        assert t1.source_hash() == t2.source_hash()

    def test_source_hash_changes_with_content(self) -> None:
        t1 = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            Hello {{ x }}"""))
        t2 = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            Goodbye {{ x }}"""))
        assert t1.source_hash() != t2.source_hash()

    def test_repr(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            {{ name }}"""))
        r = repr(tmpl)
        assert "Template" in r
        assert "name" in r


# ---------------------------------------------------------------------------
# load_template
# ---------------------------------------------------------------------------


class TestLoadTemplate:
    """Tests for the load_template() convenience function."""

    def test_load_and_render(self, simple_template_path: Path) -> None:
        from md_tmpl import load_template

        tmpl = load_template(str(simple_template_path))
        assert tmpl.render(name="world") == "Hello world!"

    def test_load_template_missing_raises(self) -> None:
        from md_tmpl import load_template

        with pytest.raises(ValueError, match="failed to load"):
            load_template("/nonexistent/path.tmpl.md")

    def test_load_template_returns_template(self, simple_template_path: Path) -> None:
        from md_tmpl import load_template

        tmpl = load_template(str(simple_template_path))
        assert isinstance(tmpl, Template)

    def test_load_template_with_load_types(self, enum_template_path: Path) -> None:
        """load_template and load_types work together."""
        from md_tmpl import load_template

        tmpl = load_template(str(enum_template_path))
        types = load_types(str(enum_template_path))
        assert hasattr(types, "Outcome")
        output = tmpl.render(outcome=types.Outcome.Rejected)
        assert output == "NO\n"


# ---------------------------------------------------------------------------
# from_source_with_base_dir
# ---------------------------------------------------------------------------


class TestFromSourceWithBaseDir:
    """Tests for Template.from_source_with_base_dir()."""

    def test_include_resolved_relative_to_base_dir(self, tmp_path: Path) -> None:
        """Include directives should resolve relative to base_dir."""
        child = tmp_path / "parts" / "header.tmpl.md"
        child.parent.mkdir(parents=True, exist_ok=True)
        child.write_text(textwrap.dedent("""\
            ---
            params: [title = str]
            ---
            # {{ title }}"""))

        source = textwrap.dedent("""\
            ---
            params:
              - title = str
            ---
            > {% include [header](./parts/header.tmpl.md) with title=title %}""")

        tmpl = Template.from_source_with_base_dir(source, str(tmp_path))
        output = tmpl.render(title="Hello")
        assert output == "# Hello"

    def test_basic_source_with_base_dir(self, tmp_path: Path) -> None:
        """A template without includes should still work with base_dir."""
        source = """---
params: [name = str]
---
Hello {{ name }}!"""
        tmpl = Template.from_source_with_base_dir(source, str(tmp_path))
        assert tmpl.render(name="world") == "Hello world!"

    def test_invalid_source_raises(self, tmp_path: Path) -> None:
        """Invalid template source should raise ValueError."""
        with pytest.raises(ValueError, match="frontmatter"):
            Template.from_source_with_base_dir("no frontmatter", str(tmp_path))


# ---------------------------------------------------------------------------
# set_max_include_depth
# ---------------------------------------------------------------------------


class TestSetMaxIncludeDepth:
    """Tests for Template.set_max_include_depth()."""

    def test_depth_limit_does_not_affect_flat_template(self) -> None:
        """Templates without includes render normally regardless of depth."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        tmpl.set_max_include_depth(1)
        assert tmpl.render(x="ok") == "ok"

    def test_depth_limit_allows_shallow_include(self, tmp_path: Path) -> None:
        """A single-level include should work with depth >= 1."""
        child = tmp_path / "child.tmpl.md"
        child.write_text(textwrap.dedent("""\
            ---
            params: [msg = str]
            ---
            Child: {{ msg }}"""))
        parent = tmp_path / "parent.tmpl.md"
        parent.write_text(textwrap.dedent("""\
            ---
            params:
              - greeting = str
            ---
            > {% include [child](./child.tmpl.md) with msg=greeting %}"""))
        tmpl = Template.from_file(str(parent))
        tmpl.set_max_include_depth(2)
        output = tmpl.render(greeting="hi")
        assert output == "Child: hi"


# ---------------------------------------------------------------------------
# body()
# ---------------------------------------------------------------------------


class TestBody:
    """Tests for Template.body()."""

    def test_body_returns_stripped_content(self) -> None:
        """body() should return the template text without frontmatter."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        assert tmpl.body() == "Hello {{ name }}!"

    def test_body_preserves_multiline(self) -> None:
        """body() should preserve multiline template content."""
        source = """---
params: [x = str]
---
Line 1
Line 2
{{ x }}"""
        tmpl = Template.from_source(source)
        body = tmpl.body()
        assert "Line 1" in body
        assert "Line 2" in body
        assert "{{ x }}" in body
        assert "params:" not in body

    def test_body_empty_template(self) -> None:
        """body() on a template with no body content should return empty."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: []
            ---
            """))
        assert tmpl.body() == ""


# ---------------------------------------------------------------------------
# PathLike support
# ---------------------------------------------------------------------------


class TestPathLikeSupport:
    """Tests for pathlib.Path acceptance in all path-taking APIs."""

    def test_from_file_with_path(self, simple_template_path: Path) -> None:
        tmpl = Template.from_file(simple_template_path)
        assert tmpl.render(name="pathlib") == "Hello pathlib!"

    def test_load_template_with_path(self, simple_template_path: Path) -> None:
        from md_tmpl import load_template

        tmpl = load_template(simple_template_path)
        assert tmpl.render(name="pathlib") == "Hello pathlib!"

    def test_template_helper_with_path(self, simple_template_path: Path) -> None:
        t = template(simple_template_path)
        assert t.render(name="pathlib") == "Hello pathlib!"

    def test_load_types_with_path(self, enum_template_path: Path) -> None:
        types = load_types(enum_template_path)
        assert hasattr(types, "Outcome")

    def test_cache_load_with_path(self, simple_template_path: Path) -> None:
        cache = TemplateCache()
        tmpl = cache.load(simple_template_path)
        assert tmpl.render(name="cached_pathlib") == "Hello cached_pathlib!"


# ---------------------------------------------------------------------------
# Template.from_source_with_base_dir — missing include
# ---------------------------------------------------------------------------


class TestFromSourceWithBaseDirExtended:
    """Extended tests for from_source_with_base_dir."""

    def test_missing_include_raises_at_render(self, tmp_path: Path) -> None:
        """Missing include should fail at render, not parse."""
        source = textwrap.dedent("""\
            ---
            params: [title = str]
            ---
            > {% include [missing](./does_not_exist.tmpl.md) with title=title %}""")
        tmpl = Template.from_source_with_base_dir(source, str(tmp_path))
        with pytest.raises(ValueError):
            tmpl.render(title="Test")


# ---------------------------------------------------------------------------
# from_source_with_env / from_source_with_options are stub-visible
# ---------------------------------------------------------------------------


class TestFromSourceConstructorsStubVisible:
    """Confirm the env/options constructors exist as callable staticmethods."""

    def test_from_source_with_env_is_stub_visible(self) -> None:
        assert callable(getattr(Template, "from_source_with_env"))

    def test_from_source_with_options_is_stub_visible(self) -> None:
        assert callable(getattr(Template, "from_source_with_options"))

    def test_from_source_with_options_base_dir_and_allow_unused(
        self, tmp_path: Path
    ) -> None:
        """base_dir + allow_unused combine in a single call."""
        source = textwrap.dedent("""\
            ---
            params: [name = str, unused = int]
            ---
            Hello {{ name }}!""")
        tmpl = Template.from_source_with_options(
            source, base_dir=str(tmp_path), allow_unused=True
        )
        assert tmpl.render(name="world", unused=1) == "Hello world!"


# ---------------------------------------------------------------------------
# name / description accessors
# ---------------------------------------------------------------------------


class TestNameAndDescription:
    """Tests for the Template.name() and Template.description() accessors."""

    def test_name_and_description_present(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            name: greeting
            description: A friendly greeting template
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        assert tmpl.name() == "greeting"
        assert tmpl.description() == "A friendly greeting template"

    def test_name_and_description_none_when_omitted(self) -> None:
        """Both return None when the frontmatter omits them."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        assert tmpl.name() is None
        assert tmpl.description() is None

    def test_name_present_description_omitted(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            name: only_name
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        assert tmpl.name() == "only_name"
        assert tmpl.description() is None

    def test_name_and_description_from_file(self, tmp_path: Path) -> None:
        path = tmp_path / "doc.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            name: doc
            description: Documented template
            params: [x = str]
            ---
            {{ x }}"""))
        tmpl = Template.from_file(str(path))
        assert tmpl.name() == "doc"
        assert tmpl.description() == "Documented template"

    def test_accessors_are_stub_visible(self) -> None:
        assert callable(getattr(Template, "name"))
        assert callable(getattr(Template, "description"))
