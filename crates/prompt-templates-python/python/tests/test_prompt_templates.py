"""Tests for the prompt_templates Python bindings.

Tests cover:
- Basic template loading and rendering (from_source, from_file)
- Type validation (str, int, float, bool, list, dict, enum)
- Enum variants: unit and struct (with payload)
- Type mismatch detection and clear error messages
- The template() helper with generated types
- The import hook (prompt_template_import_hook)
- TemplateCache
- Default values
- Edge cases (empty params, missing params, nested types)
- Strict validation: extra params rejected by default
- allow_extra flag
- render_dict API
- @variant decorator
- Variants metaclass
- load_types() helper
- Template metadata (declarations, source_hash, defaults)
- Generated type __repr__, __eq__, __hash__
- Generated type __match_args__ for pattern matching
"""

import sys
import textwrap
from pathlib import Path
from typing import Any

import pytest

from prompt_templates import (
    Template,
    TemplateCache,
    Variants,
    load_types,
    template,
    variant,
)

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture()
def simple_template_path(tmp_path: Path) -> Path:
    """Create a simple greeting template file."""
    path = tmp_path / "greeting.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - name = str
        ---
        Hello {{ name }}!"""))
    return path


@pytest.fixture()
def list_template_path(tmp_path: Path) -> Path:
    """Create a template with typed list params."""
    path = tmp_path / "bugs.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - bugs = list<title = str, severity = str>
        ---
        > {% for bug in bugs %}
        - **{{ bug.title }}** ({{ bug.severity }})
        > {% /for %}"""))
    return path


@pytest.fixture()
def enum_template_path(tmp_path: Path) -> Path:
    """Create a template with enum params including a struct variant."""
    path = tmp_path / "status.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - outcome = enum<Confirmed(evidence = str), Rejected, NeedsWork>
        ---
        > {% match outcome %}
        > {% case Confirmed %}
        YES: {{ outcome.evidence }}
        > {% case Rejected %}
        NO
        > {% case NeedsWork %}
        MAYBE
        > {% /match %}"""))
    return path


@pytest.fixture()
def default_template_path(tmp_path: Path) -> Path:
    """Create a template with default values."""
    path = tmp_path / "defaults.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - name = str := "World"
          - count = int := 1
        ---
        Hello {{ name }}, count={{ count }}!"""))
    return path


@pytest.fixture()
def dict_template_path(tmp_path: Path) -> Path:
    """Create a template with dict params."""
    path = tmp_path / "config.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - config = dict<host = str, port = int>
        ---
        {{ config.host }}:{{ config.port }}"""))
    return path


@pytest.fixture()
def multi_param_path(tmp_path: Path) -> Path:
    """Create a template with multiple param types."""
    path = tmp_path / "multi.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - name = str
          - count = int
          - score = float
          - enabled = bool
        ---
        {{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}"""))
    return path


# ---------------------------------------------------------------------------
# Template.from_source — basic rendering
# ---------------------------------------------------------------------------


class TestFromSource:
    """Tests for Template.from_source()."""

    def test_basic_render(self) -> None:
        tmpl = Template.from_source(
            "---\nparams:\n  - name = str\n---\nHello {{ name }}!"
        )
        assert tmpl.render(name="world") == "Hello world!"

    def test_int_param(self) -> None:
        tmpl = Template.from_source(
            "---\nparams:\n  - count = int\n---\nCount: {{ count }}"
        )
        assert tmpl.render(count=42) == "Count: 42"

    def test_bool_param(self) -> None:
        tmpl = Template.from_source(
            "---\nparams: [flag = bool]\n---\n{% if flag %}yes{% /if %}"
        )
        assert tmpl.render(flag=True) == "yes"

    def test_float_param(self) -> None:
        tmpl = Template.from_source("---\nparams: [score = float]\n---\n{{ score }}")
        assert tmpl.render(score=3.14) == "3.14"

    def test_syntax_error_raises(self) -> None:
        with pytest.raises(ValueError, match="frontmatter"):
            Template.from_source("no frontmatter at all")

    def test_missing_param_raises(self) -> None:
        tmpl = Template.from_source(
            "---\nparams: [name = str, age = int]\n---\n{{ name }} {{ age }}"
        )
        with pytest.raises(ValueError, match="missing"):
            tmpl.render(name="Alice")

    def test_type_mismatch_raises(self) -> None:
        tmpl = Template.from_source("---\nparams: [flag = bool]\n---\n{{ flag }}")
        with pytest.raises(ValueError, match="type mismatch"):
            tmpl.render(flag="not a bool")


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
        tmpl = Template.from_source_allowing_unused(
            "---\nparams: [name = str, unused = int]\n---\nHello {{ name }}!"
        )
        assert tmpl.render(name="world", unused=42) == "Hello world!"

    def test_unused_param_rejected_in_strict_mode(self) -> None:
        """from_source() should reject unused params."""
        with pytest.raises(ValueError):
            Template.from_source(
                "---\nparams: [name = str, unused = int]\n---\nHello {{ name }}!"
            )


# ---------------------------------------------------------------------------
# Strict validation — extra params
# ---------------------------------------------------------------------------


class TestStrictValidation:
    """Tests for strict extra-param rejection."""

    def test_extra_param_rejected_by_default(self) -> None:
        tmpl = Template.from_source("---\nparams: [name = str]\n---\nHello {{ name }}!")
        with pytest.raises(ValueError, match="extra|undeclared|not declared"):
            tmpl.render(name="world", bogus="unexpected")

    def test_allow_extra_ignores_extra_params(self) -> None:
        tmpl = Template.from_source("---\nparams: [name = str]\n---\nHello {{ name }}!")
        result = tmpl.render(name="world", bogus="ignored", allow_extra=True)
        assert result == "Hello world!"

    def test_render_dict_extra_param_rejected(self) -> None:
        tmpl = Template.from_source("---\nparams: [name = str]\n---\nHello {{ name }}!")
        with pytest.raises(ValueError, match="extra|undeclared|not declared"):
            tmpl.render_dict({"name": "world", "bogus": "unexpected"})

    def test_render_dict_allow_extra(self) -> None:
        tmpl = Template.from_source("---\nparams: [name = str]\n---\nHello {{ name }}!")
        result = tmpl.render_dict(
            {"name": "world", "bogus": "ignored"}, allow_extra=True
        )
        assert result == "Hello world!"


# ---------------------------------------------------------------------------
# render_dict
# ---------------------------------------------------------------------------


class TestRenderDict:
    """Tests for Template.render_dict()."""

    def test_basic_render_dict(self) -> None:
        tmpl = Template.from_source("---\nparams: [name = str]\n---\nHello {{ name }}!")
        assert tmpl.render_dict({"name": "dict"}) == "Hello dict!"

    def test_render_dict_type_validation(self) -> None:
        tmpl = Template.from_source("---\nparams: [count = int]\n---\n{{ count }}")
        with pytest.raises(ValueError, match="type mismatch"):
            tmpl.render_dict({"count": "not an int"})


# ---------------------------------------------------------------------------
# Typed lists
# ---------------------------------------------------------------------------


class TestTypedLists:
    """Tests for list<...> parameters."""

    def test_render_list_of_dicts(self, list_template_path: Path) -> None:
        tmpl = Template.from_file(str(list_template_path))
        output = tmpl.render(
            bugs=[
                {"title": "Buffer overflow", "severity": "Critical"},
                {"title": "Race condition", "severity": "High"},
            ]
        )
        assert "Buffer overflow" in output
        assert "Race condition" in output
        assert "Critical" in output
        assert "High" in output

    def test_empty_list(self, list_template_path: Path) -> None:
        tmpl = Template.from_file(str(list_template_path))
        output = tmpl.render(bugs=[])
        assert output.strip() == ""

    def test_wrong_item_type_raises(self, list_template_path: Path) -> None:
        tmpl = Template.from_file(str(list_template_path))
        with pytest.raises((ValueError, TypeError)):
            tmpl.render(bugs=["not a dict"])


# ---------------------------------------------------------------------------
# Dict parameters
# ---------------------------------------------------------------------------


class TestDictParams:
    """Tests for dict<...> parameters."""

    def test_render_dict_param(self, dict_template_path: Path) -> None:
        tmpl = Template.from_file(str(dict_template_path))
        output = tmpl.render(config={"host": "localhost", "port": 8080})
        assert "localhost" in output
        assert "8080" in output

    def test_dict_missing_field_raises(self, dict_template_path: Path) -> None:
        tmpl = Template.from_file(str(dict_template_path))
        with pytest.raises(ValueError, match="missing"):
            tmpl.render(config={"host": "localhost"})  # missing port


# ---------------------------------------------------------------------------
# Multiple param types
# ---------------------------------------------------------------------------


class TestMultipleParamTypes:
    """Tests for templates with multiple param types."""

    def test_all_types(self, multi_param_path: Path) -> None:
        tmpl = Template.from_file(str(multi_param_path))
        output = tmpl.render(name="Alice", count=42, score=9.5, enabled=True)
        assert "Alice" in output
        assert "42" in output
        assert "9.5" in output
        assert "true" in output or "True" in output


# ---------------------------------------------------------------------------
# Enum dispatch
# ---------------------------------------------------------------------------


class TestEnumDispatch:
    """Tests for enum<...> parameters with match/case."""

    def test_unit_variant(self, enum_template_path: Path) -> None:
        tmpl = Template.from_file(str(enum_template_path))
        output = tmpl.render(outcome="Rejected")
        assert "NO" in output

    def test_struct_variant_as_dict(self, enum_template_path: Path) -> None:
        tmpl = Template.from_file(str(enum_template_path))
        output = tmpl.render(
            outcome={
                "tag": "Confirmed",
                "evidence": "found it",
            }
        )
        assert "YES" in output
        assert "found it" in output

    def test_invalid_variant_raises(self, enum_template_path: Path) -> None:
        tmpl = Template.from_file(str(enum_template_path))
        with pytest.raises(ValueError, match="type mismatch|enum"):
            tmpl.render(outcome="Unknown")


# ---------------------------------------------------------------------------
# Default values
# ---------------------------------------------------------------------------


class TestDefaults:
    """Tests for parameters with default values."""

    def test_defaults_used_when_omitted(self, default_template_path: Path) -> None:
        tmpl = Template.from_file(str(default_template_path))
        output = tmpl.render()
        assert "Hello World" in output
        assert "count=1" in output

    def test_defaults_overridden(self, default_template_path: Path) -> None:
        tmpl = Template.from_file(str(default_template_path))
        output = tmpl.render(name="Alice", count=99)
        assert "Hello Alice" in output
        assert "count=99" in output

    def test_defaults_dict(self, default_template_path: Path) -> None:
        tmpl = Template.from_file(str(default_template_path))
        defaults = tmpl.defaults()
        assert "name" in defaults
        assert "count" in defaults


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
# Template metadata
# ---------------------------------------------------------------------------


class TestMetadata:
    """Tests for template metadata methods."""

    def test_declarations(self) -> None:
        tmpl = Template.from_source(
            "---\nparams: [name = str, count = int]\n---\n{{ name }} {{ count }}"
        )
        decls = tmpl.declarations()
        names = [d[0] for d in decls]
        assert "name" in names
        assert "count" in names

    def test_declarations_types(self) -> None:
        tmpl = Template.from_source(
            "---\nparams: [name = str, count = int]\n---\n{{ name }} {{ count }}"
        )
        decls = tmpl.declarations()
        type_map = {d[0]: d[1] for d in decls}
        assert type_map["name"] == "str"
        assert type_map["count"] == "int"

    def test_source_hash_stable(self) -> None:
        source = "---\nparams: [x = str]\n---\n{{ x }}"
        t1 = Template.from_source(source)
        t2 = Template.from_source(source)
        assert t1.source_hash() == t2.source_hash()

    def test_source_hash_changes_with_content(self) -> None:
        t1 = Template.from_source("---\nparams: [x = str]\n---\nHello {{ x }}")
        t2 = Template.from_source("---\nparams: [x = str]\n---\nGoodbye {{ x }}")
        assert t1.source_hash() != t2.source_hash()

    def test_repr(self) -> None:
        tmpl = Template.from_source("---\nparams: [name = str]\n---\n{{ name }}")
        r = repr(tmpl)
        assert "Template" in r
        assert "name" in r


# ---------------------------------------------------------------------------
# template() helper
# ---------------------------------------------------------------------------


class TestTemplateHelper:
    """Tests for the template() convenience function."""

    def test_render_with_kwargs(self, simple_template_path: Path) -> None:
        t = template(str(simple_template_path))
        assert t.render(name="helper") == "Hello helper!"

    def test_generated_enum_types(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        # Should have a generated Outcome enum.
        assert hasattr(t, "Outcome")

        # Unit variant — sentinel, no parens needed.
        rejected = t.Outcome.Rejected
        assert rejected._prompt_template_tag == "Rejected"

        # Struct variant — callable constructor.
        confirmed = t.Outcome.Confirmed(evidence="proof")
        assert confirmed._prompt_template_tag == "Confirmed"
        assert confirmed._prompt_template_fields["evidence"] == "proof"

    def test_render_with_generated_enum(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        output = t.render(outcome=t.Outcome.Rejected)
        assert "NO" in output

    def test_render_with_struct_variant(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        output = t.render(outcome=t.Outcome.Confirmed(evidence="found it"))
        assert "YES" in output
        assert "found it" in output

    def test_generated_list_item_type(self, list_template_path: Path) -> None:
        t = template(str(list_template_path))
        # Should have a generated Item type.
        type_names = list(t._types.keys())
        item_classes = [n for n in type_names if "Item" in n]
        assert len(item_classes) > 0, f"Expected item classes, got {type_names}"

    def test_render_dict_via_helper(self, simple_template_path: Path) -> None:
        t = template(str(simple_template_path))
        assert t.render_dict({"name": "dict_helper"}) == "Hello dict_helper!"

    def test_declarations_via_helper(self, simple_template_path: Path) -> None:
        t = template(str(simple_template_path))
        decls = t.declarations()
        assert any(d[0] == "name" for d in decls)

    def test_repr(self, simple_template_path: Path) -> None:
        t = template(str(simple_template_path))
        assert "template(" in repr(t)


# ---------------------------------------------------------------------------
# Import hook
# ---------------------------------------------------------------------------


class TestImportHook:
    """Tests for prompt_template_import_hook()."""

    def test_import_hook_installs(self) -> None:
        from prompt_templates import prompt_template_import_hook

        # Should not raise.
        prompt_template_import_hook(search_paths=["/nonexistent"])

        # Check it's in sys.meta_path.
        from prompt_templates._import_hook import _TemplateFinder

        finders = [f for f in sys.meta_path if isinstance(f, _TemplateFinder)]
        assert len(finders) == 1

    def test_import_hook_idempotent(self) -> None:
        from prompt_templates import prompt_template_import_hook

        prompt_template_import_hook(search_paths=["/tmp"])
        prompt_template_import_hook(search_paths=["/tmp"])

        from prompt_templates._import_hook import _TemplateFinder

        finders = [f for f in sys.meta_path if isinstance(f, _TemplateFinder)]
        assert len(finders) == 1, "Should not duplicate the hook"

    def test_import_template_file(self, simple_template_path: Path) -> None:
        from prompt_templates import prompt_template_import_hook
        import importlib

        # Install hook with the temp directory.
        prompt_template_import_hook(search_paths=[str(simple_template_path.parent)])

        # The module name comes from the file stem: "greeting"
        module_name = "greeting"

        # Remove from sys.modules if cached from a previous test.
        sys.modules.pop(module_name, None)

        # Import should work.
        mod = importlib.import_module(module_name)

        # Module should have generated types.
        assert hasattr(mod, "GreetingParams") or hasattr(
            mod, "Greeting"
        ), f"Module should have a params class, got: {dir(mod)}"

    def test_normal_imports_unaffected(self) -> None:
        """The hook should not break normal Python imports."""
        from prompt_templates import prompt_template_import_hook
        import os as os_mod

        prompt_template_import_hook()

        # These should still work fine.
        assert os_mod.path is not None
        assert sys.version is not None


# ---------------------------------------------------------------------------
# @variant decorator
# ---------------------------------------------------------------------------


class TestVariantDecorator:
    """Tests for the @variant decorator."""

    def test_basic_variant(self) -> None:
        @variant
        class NeedsChanges:
            reason: str

        v = NeedsChanges(reason="fix tests")
        assert v._prompt_template_tag == "NeedsChanges"
        assert v.reason == "fix tests"

    def test_variant_fields_property(self) -> None:
        @variant
        class Error:
            code: int
            message: str

        v = Error(code=404, message="not found")
        fields = v._prompt_template_fields
        assert fields == {"code": 404, "message": "not found"}

    def test_variant_repr(self) -> None:
        @variant
        class Item:
            name: str

        v = Item(name="test")
        assert repr(v) == "Item(name='test')"

    def test_variant_equality(self) -> None:
        @variant
        class Pair:
            x: int
            y: int

        assert Pair(x=1, y=2) == Pair(x=1, y=2)
        assert Pair(x=1, y=2) != Pair(x=1, y=3)

    def test_variant_hash(self) -> None:
        @variant
        class Tag:
            label: str

        a = Tag(label="one")
        b = Tag(label="one")
        assert hash(a) == hash(b)
        assert {a, b} == {a}  # deduplication

    def test_variant_match_args(self) -> None:
        @variant
        class Point:
            x: float
            y: float

        assert Point.__match_args__ == ("x", "y")

    def test_variant_missing_field_raises(self) -> None:
        @variant
        class Required:
            value: str

        with pytest.raises(TypeError, match="missing"):
            Required()  # type: ignore[call-arg]

    def test_variant_unexpected_field_raises(self) -> None:
        @variant
        class Simple:
            x: int

        with pytest.raises(TypeError, match="unexpected"):
            Simple(x=1, y=2)  # type: ignore[call-arg]

    def test_variant_no_fields_raises(self) -> None:
        """@variant requires at least one annotated field."""
        with pytest.raises(TypeError, match="annotated field"):

            @variant
            class Empty:
                pass


# ---------------------------------------------------------------------------
# Variants metaclass
# ---------------------------------------------------------------------------


class TestVariantsMetaclass:
    """Tests for the Variants base class."""

    def test_unit_variants(self) -> None:
        class Color(Variants):
            Red = ()
            Green = ()
            Blue = ()

        assert repr(Color.Red) == "Red"
        assert repr(Color.Green) == "Green"
        assert repr(Color.Blue) == "Blue"

    def test_unit_variant_tag(self) -> None:
        class Status(Variants):
            Active = ()
            Inactive = ()

        assert Status.Active._prompt_template_tag == "Active"
        assert Status.Active._prompt_template_fields == {}

    def test_unit_variant_equality(self) -> None:
        class Side(Variants):
            Left = ()
            Right = ()

        assert Side.Left == Side.Left
        assert Side.Left != Side.Right

    def test_unit_variant_hash(self) -> None:
        class Dir(Variants):
            Up = ()
            Down = ()

        s = {Dir.Up, Dir.Down, Dir.Up}
        assert len(s) == 2

    def test_struct_variant(self) -> None:
        class Result(Variants):
            Ok = {"value": str}
            Err = {"code": int, "message": str}

        ok = Result.Ok(value="done")
        assert ok._prompt_template_tag == "Ok"
        assert ok.value == "done"
        assert ok._prompt_template_fields == {"value": "done"}

        err = Result.Err(code=500, message="fail")
        assert err._prompt_template_tag == "Err"
        assert err.code == 500
        assert err.message == "fail"

    def test_mixed_variants(self) -> None:
        class Status(Variants):
            Approved = ()
            Rejected = ()
            NeedsChanges = {"reason": str}

        assert repr(Status.Approved) == "Approved"
        nc = Status.NeedsChanges(reason="fix tests")
        assert nc.reason == "fix tests"

    def test_struct_variant_repr(self) -> None:
        class Wrap(Variants):
            Inner = {"x": int}

        v = Wrap.Inner(x=42)
        assert "Inner" in repr(v)
        assert "42" in repr(v)

    def test_struct_variant_equality(self) -> None:
        class Op(Variants):
            Add = {"n": int}

        assert Op.Add(n=1) == Op.Add(n=1)
        assert Op.Add(n=1) != Op.Add(n=2)


# ---------------------------------------------------------------------------
# load_types
# ---------------------------------------------------------------------------


class TestLoadTypes:
    """Tests for the load_types() function."""

    def test_load_all_types(self, enum_template_path: Path) -> None:
        types = load_types(str(enum_template_path))
        assert hasattr(types, "Outcome")
        assert hasattr(types, "Status")

    def test_load_types_pick(self, enum_template_path: Path) -> None:
        types = load_types(str(enum_template_path), pick=["Outcome"])
        assert hasattr(types, "Outcome")
        # Should NOT have Status since we didn't pick it.
        assert not hasattr(types, "Status")

    def test_load_types_pick_missing_raises(self, enum_template_path: Path) -> None:
        with pytest.raises(KeyError, match="not found"):
            load_types(str(enum_template_path), pick=["NonExistent"])

    def test_load_types_invalid_path_raises(self) -> None:
        with pytest.raises(ValueError):
            load_types("/nonexistent/template.tmpl.md")


# ---------------------------------------------------------------------------
# Enum ergonomics
# ---------------------------------------------------------------------------


class TestEnumErgonomics:
    """Detailed tests for generated enum types."""

    def test_unit_variant_repr(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        assert repr(t.Outcome.Rejected) == "Rejected"

    def test_struct_variant_repr(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        v = t.Outcome.Confirmed(evidence="proof")
        assert "Confirmed" in repr(v)
        assert "proof" in repr(v)

    def test_variant_equality(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        assert t.Outcome.Rejected == t.Outcome.Rejected
        assert t.Outcome.Confirmed(evidence="a") == t.Outcome.Confirmed(evidence="a")
        assert t.Outcome.Confirmed(evidence="a") != t.Outcome.Confirmed(evidence="b")
        assert t.Outcome.Rejected != t.Outcome.NeedsWork

    def test_variant_hash(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        # Hashable — can be used in sets/dicts.
        s = {t.Outcome.Rejected, t.Outcome.NeedsWork}
        assert len(s) == 2

    def test_struct_variant_match_args(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        confirmed_cls = t.Outcome.Confirmed
        # Struct variants should have __match_args__ for pattern matching.
        assert hasattr(confirmed_cls, "__match_args__")
        assert "evidence" in confirmed_cls.__match_args__

    def test_struct_variant_fields_dict(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        v = t.Outcome.Confirmed(evidence="proof")
        assert v._prompt_template_fields == {"evidence": "proof"}


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    """Edge case tests."""

    def test_empty_params(self, tmp_path: Path) -> None:
        path = tmp_path / "empty.tmpl.md"
        path.write_text("---\nparams: []\n---\nStatic content")
        tmpl = Template.from_file(str(path))
        assert tmpl.render() == "Static content"

    def test_unicode_params(self) -> None:
        tmpl = Template.from_source("---\nparams: [msg = str]\n---\n{{ msg }}")
        assert tmpl.render(msg="🎉 Hello 世界!") == "🎉 Hello 世界!"

    def test_multiline_template(self) -> None:
        tmpl = Template.from_source(
            "---\nparams: [title = str]\n---\n# {{ title }}\n\nBody text."
        )
        output = tmpl.render(title="Test")
        assert "# Test" in output
        assert "Body text." in output

    def test_multiple_vars_same_template(self) -> None:
        tmpl = Template.from_source(
            "---\nparams: [a = str, b = str]\n---\n{{ a }} and {{ b }}"
        )
        assert tmpl.render(a="X", b="Y") == "X and Y"

    def test_template_caching_different_files(self, tmp_path: Path) -> None:
        p1 = tmp_path / "a.tmpl.md"
        p2 = tmp_path / "b.tmpl.md"
        p1.write_text("---\nparams: [x = str]\n---\nA: {{ x }}")
        p2.write_text("---\nparams: [x = str]\n---\nB: {{ x }}")

        cache = TemplateCache()
        t1 = cache.load(str(p1))
        t2 = cache.load(str(p2))
        assert t1.render(x="v") == "A: v"
        assert t2.render(x="v") == "B: v"
        assert t1.source_hash() != t2.source_hash()

    def test_validate_declarations_match(self) -> None:
        tmpl = Template.from_source("---\nparams: [name = str]\n---\n{{ name }}")
        decls = tmpl.declarations()
        # Should not raise — declarations match themselves.
        tmpl.validate_declarations_against(decls)

    def test_validate_declarations_mismatch(self) -> None:
        tmpl = Template.from_source("---\nparams: [name = str]\n---\n{{ name }}")
        with pytest.raises(ValueError, match="declarations changed"):
            tmpl.validate_declarations_against([("different", "int")])
