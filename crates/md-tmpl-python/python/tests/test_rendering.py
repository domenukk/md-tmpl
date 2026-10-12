"""Tests for md_tmpl render APIs, parameter validation, and value conversion."""

import textwrap
from pathlib import Path
from typing import Any

import pytest

from md_tmpl import (
    Template,
    TemplateCache,
    TypeMismatchError,
    ExtraParamsError,
    DeclarationsMutatedError,
)

# ---------------------------------------------------------------------------
# Strict validation — extra params
# ---------------------------------------------------------------------------


class TestStrictValidation:
    """Tests for strict extra-param rejection."""

    def test_extra_param_rejected_by_default(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        with pytest.raises(ExtraParamsError) as excinfo:
            tmpl.render(name="world", bogus="unexpected")
        assert excinfo.value.kind == "extra_params"
        assert excinfo.value.extra == ["bogus"]

    def test_allow_extra_ignores_extra_params(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        result = tmpl.render(name="world", bogus="ignored", allow_extra=True)
        assert result == "Hello world!"

    def test_render_dict_extra_param_rejected(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        with pytest.raises(ExtraParamsError) as excinfo:
            tmpl.render_dict({"name": "world", "bogus": "unexpected"})
        assert excinfo.value.kind == "extra_params"
        assert excinfo.value.extra == ["bogus"]

    def test_render_dict_allow_extra(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
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
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        assert tmpl.render_dict({"name": "dict"}) == "Hello dict!"

    def test_render_dict_type_validation(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [count = int]
            ---
            {{ count }}"""))
        with pytest.raises(TypeMismatchError) as excinfo:
            tmpl.render_dict({"count": "not an int"})
        assert excinfo.value.kind == "type_mismatch"
        assert excinfo.value.path == "count"


# ---------------------------------------------------------------------------
# render_json
# ---------------------------------------------------------------------------


class TestRenderJson:
    """Tests for Template.render_json() — the JSON fast path."""

    def test_basic_render_json(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        assert tmpl.render_json('{"name": "json"}') == "Hello json!"

    def test_render_json_allow_extra(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = int]
            ---
            {{ x }}"""))
        assert tmpl.render_json('{"x": 42, "extra": true}', allow_extra=True) == "42"

    def test_render_json_extra_rejected_by_default(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = int]
            ---
            {{ x }}"""))
        with pytest.raises(ExtraParamsError) as excinfo:
            tmpl.render_json('{"x": 42, "extra": true}')
        assert excinfo.value.kind == "extra_params"
        assert excinfo.value.extra == ["extra"]

    def test_render_json_invalid_json(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        with pytest.raises(ValueError, match="invalid JSON"):
            tmpl.render_json("not valid json")

    def test_render_json_non_object(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        with pytest.raises(ValueError):
            tmpl.render_json("[1, 2, 3]")

    def test_render_json_float_param(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [score = float]
            ---
            {{ score | fixed(2) }}"""))
        assert tmpl.render_json('{"score": 87.456}') == "87.46"

    def test_render_json_bool_param(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [flag = bool]
            ---
            {{ flag }}"""))
        assert tmpl.render_json('{"flag": true}') == "true"

    def test_render_json_matches_render(self) -> None:
        """render_json() should produce identical output to render()."""
        import json

        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str, place = str]
            ---
            Hello {{ name }}, welcome to {{ place }}!"""))
        params: dict[str, Any] = {"name": "Alice", "place": "Wonderland"}
        assert tmpl.render_json(json.dumps(params)) == tmpl.render(**params)


# ---------------------------------------------------------------------------
# Typed lists
# ---------------------------------------------------------------------------


class TestTypedLists:
    """Tests for list(...) parameters."""

    def test_render_list_of_structs(self, list_template_path: Path) -> None:
        tmpl = Template.from_file(str(list_template_path))
        output = tmpl.render(
            tasks=[
                {"title": "Write documentation", "priority": "High"},
                {"title": "Add unit tests", "priority": "Medium"},
            ]
        )
        assert (
            output
            == "- **Write documentation** (High)\n- **Add unit tests** (Medium)\n"
        )

    def test_empty_list(self, list_template_path: Path) -> None:
        tmpl = Template.from_file(str(list_template_path))
        output = tmpl.render(tasks=[])
        assert output.strip() == ""

    def test_wrong_item_type_raises(self, list_template_path: Path) -> None:
        tmpl = Template.from_file(str(list_template_path))
        with pytest.raises((ValueError, TypeError)):
            tmpl.render(tasks=["not a struct"])


# ---------------------------------------------------------------------------
# Struct parameters
# ---------------------------------------------------------------------------


class TestStructParams:
    """Tests for struct(...) parameters."""

    def test_render_struct_param(self, struct_template_path: Path) -> None:
        tmpl = Template.from_file(str(struct_template_path))
        output = tmpl.render(config={"host": "localhost", "port": 8080})
        assert output == "localhost:8080"

    def test_struct_missing_field_raises(self, struct_template_path: Path) -> None:
        tmpl = Template.from_file(str(struct_template_path))
        with pytest.raises(TypeMismatchError) as excinfo:
            tmpl.render(config={"host": "localhost"})  # missing port
        assert excinfo.value.kind == "type_mismatch"
        assert excinfo.value.expected == "int"


# ---------------------------------------------------------------------------
# Multiple param types
# ---------------------------------------------------------------------------


class TestMultipleParamTypes:
    """Tests for templates with multiple param types."""

    def test_all_types(self, multi_param_path: Path) -> None:
        tmpl = Template.from_file(str(multi_param_path))
        output = tmpl.render(name="Alice", count=42, score=9.5, enabled=True)
        assert output == "Alice: count=42, score=9.5, enabled=true"


# ---------------------------------------------------------------------------
# Enum dispatch
# ---------------------------------------------------------------------------


class TestEnumDispatch:
    """Tests for enum(...) parameters with match/case."""

    def test_unit_variant(self, enum_template_path: Path) -> None:
        tmpl = Template.from_file(str(enum_template_path))
        output = tmpl.render(outcome="Rejected")
        assert output == "NO\n"

    def test_struct_variant_as_dict(self, enum_template_path: Path) -> None:
        tmpl = Template.from_file(str(enum_template_path))
        output = tmpl.render(
            outcome={
                "__kind__": "Confirmed",
                "evidence": "found it",
            }
        )
        assert output == "YES: found it\n"

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
        assert output == "Hello World, count=1!"

    def test_defaults_overridden(self, default_template_path: Path) -> None:
        tmpl = Template.from_file(str(default_template_path))
        output = tmpl.render(name="Alice", count=99)
        assert output == "Hello Alice, count=99!"

    def test_defaults_dict(self, default_template_path: Path) -> None:
        tmpl = Template.from_file(str(default_template_path))
        defaults = tmpl.defaults()
        assert "name" in defaults
        assert "count" in defaults


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    """Edge case tests."""

    def test_empty_params(self, tmp_path: Path) -> None:
        path = tmp_path / "empty.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params: []
            ---
            Static content"""))
        tmpl = Template.from_file(str(path))
        assert tmpl.render() == "Static content"

    def test_unicode_params(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [msg = str]
            ---
            {{ msg }}"""))
        assert tmpl.render(msg="🎉 Hello 世界!") == "🎉 Hello 世界!"

    def test_multiline_template(self) -> None:
        tmpl = Template.from_source("""---
params: [title = str]
---
# {{ title }}

Body text.""")
        output = tmpl.render(title="Test")
        assert output == "# Test\n\nBody text."

    def test_multiple_vars_same_template(self) -> None:
        tmpl = Template.from_source("""---
params: [a = str, b = str]
---
{{ a }} and {{ b }}""")
        assert tmpl.render(a="X", b="Y") == "X and Y"

    def test_template_caching_different_files(self, tmp_path: Path) -> None:
        p1 = tmp_path / "a.tmpl.md"
        p2 = tmp_path / "b.tmpl.md"
        p1.write_text(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            A: {{ x }}"""))
        p2.write_text(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            B: {{ x }}"""))

        cache = TemplateCache()
        t1 = cache.load(str(p1))
        t2 = cache.load(str(p2))
        assert t1.render(x="v") == "A: v"
        assert t2.render(x="v") == "B: v"
        assert t1.source_hash() != t2.source_hash()

    def test_validate_declarations_match(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            {{ name }}"""))
        decls = tmpl.declarations()
        # Should not raise — declarations match themselves.
        tmpl.validate_declarations_against(decls)

    def test_validate_declarations_mismatch(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            {{ name }}"""))
        with pytest.raises(DeclarationsMutatedError) as excinfo:
            tmpl.validate_declarations_against([("different", "int")])
        assert excinfo.value.kind == "declarations_mutated"
        assert "different" in excinfo.value.details


# ---------------------------------------------------------------------------
# None handling
# ---------------------------------------------------------------------------


class TestNoneHandling:
    """Tests for None value behavior.

    Python None maps to the engine's transparent ``Value::None``,
    representing an absent value. Passing None for a non-optional
    parameter is a type error — only ``option(T)`` params accept None.
    """

    def test_none_value_raises_type_error(self) -> None:
        """None on a non-option str param raises TypeMismatchError."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            {{ name }}"""))
        with pytest.raises(Exception, match="type mismatch"):
            tmpl.render(name=None)

    def test_none_in_list_raises_type_error(self) -> None:
        """None inside a list element raises TypeError."""
        tmpl = Template.from_source("""---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ item.name }}

> {% /for %}""")
        with pytest.raises(Exception):
            tmpl.render(items=[None])

    def test_none_in_struct_value_raises_type_error(self) -> None:
        """None inside a struct field raises TypeMismatchError."""
        tmpl = Template.from_source("""---
params:
  - config = struct(host = str)
---
{{ config.host }}""")
        with pytest.raises(Exception, match="type mismatch"):
            tmpl.render(config={"host": None})


# ---------------------------------------------------------------------------
# Dataclass-like __dict__ conversion fallback
# ---------------------------------------------------------------------------


class TestDictConversionFallback:
    """Tests for passing objects with __dict__ as template values."""

    def test_object_with_dict_as_model(self) -> None:
        class Config:
            def __init__(self, host: str, port: int) -> None:
                self.host = host
                self.port = port

        tmpl = Template.from_source("""---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}""")
        result = tmpl.render(config=Config(host="localhost", port=8080))
        assert result == "localhost:8080"

    def test_dataclass_as_model(self) -> None:
        from dataclasses import dataclass

        @dataclass
        class Item:
            name: str
            value: int

        tmpl = Template.from_source("""---
params:
  - item = struct(name = str, value = int)
---
{{ item.name }}={{ item.value }}""")
        result = tmpl.render(item=Item(name="score", value=42))
        assert result == "score=42"


# ---------------------------------------------------------------------------
# Boundary value tests (ported from Go)
# ---------------------------------------------------------------------------


class TestBoundaryValues:
    """Tests for edge-case values: empty strings, negative ints, etc."""

    def test_empty_string_param(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            [{{ x }}]"""))
        assert tmpl.render(x="") == "[]"

    def test_negative_int(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = int]
            ---
            {{ x }}"""))
        assert tmpl.render(x=-42) == "-42"

    def test_zero_int(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [val = int]
            ---
            {{ val }}"""))
        assert tmpl.render(val=0) == "0"

    def test_large_int(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = int]
            ---
            {{ x }}"""))
        large = 9_223_372_036_854_775_807
        result = tmpl.render(x=large)
        assert result == str(large)

    def test_false_bool_in_else(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [flag = bool]
            ---
            > {% if flag %}

            yes

            > {% else %}

            no

            > {% /if %}"""))
        assert tmpl.render(flag=False) == "no\n"


# ---------------------------------------------------------------------------
# Nested struct parameters (ported from Go)
# ---------------------------------------------------------------------------


class TestNestedStructs:
    """Tests for deeply nested struct params."""

    def test_nested_struct(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - config = struct(inner = struct(host = str))
            ---
            {{ config.inner.host }}"""))
        result = tmpl.render(config={"inner": {"host": "example.com"}})
        assert result == "example.com"

    def test_nested_struct_dataclass(self) -> None:
        """Nested dataclasses should be convertible via __dict__ fallback."""

        class Inner:
            def __init__(self, host: str) -> None:
                self.host = host

        class Config:
            def __init__(self, inner: Any) -> None:
                self.inner = inner

        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - config = struct(inner = struct(host = str))
            ---
            {{ config.inner.host }}"""))
        result = tmpl.render(config=Config(inner=Inner(host="nested.example.com")))
        assert result == "nested.example.com"


# ---------------------------------------------------------------------------
# Unsupported type conversion
# ---------------------------------------------------------------------------


class TestUnsupportedTypeConversion:
    """Tests for convert.rs error paths with non-convertible types."""

    def test_set_as_param_raises(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        with pytest.raises(TypeError, match="cannot convert"):
            tmpl.render(x={1, 2, 3})

    def test_bytes_as_param_raises(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        with pytest.raises(TypeError, match="cannot convert"):
            tmpl.render(x=b"bytes")

    def test_complex_as_param_raises(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        with pytest.raises(TypeError, match="cannot convert"):
            tmpl.render(x=complex(1, 2))


# ---------------------------------------------------------------------------
# render with no kwargs
# ---------------------------------------------------------------------------


class TestRenderNoKwargs:
    """Tests for rendering with empty or no parameters."""

    def test_render_with_no_kwargs(self) -> None:
        """Render with no keyword arguments at all."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: []
            ---
            Static only."""))
        assert tmpl.render() == "Static only."

    def test_render_dict_empty_dict(self) -> None:
        """render_dict with empty dict on paramless template."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: []
            ---
            Static."""))
        assert tmpl.render_dict({}) == "Static."


# ---------------------------------------------------------------------------
# render_empty
# ---------------------------------------------------------------------------


class TestRenderEmpty:
    """Tests for Template.render_empty()."""

    def test_render_empty_no_params(self) -> None:
        tmpl = Template.from_source("""---
params: []
---
Hello world!""")
        assert tmpl.render_empty() == "Hello world!"

    def test_render_empty_all_defaults(self) -> None:
        tmpl = Template.from_source("""---
params:
  - greeting = str := "Hi"
  - count = int := 5
---
{{ greeting }} {{ count }}""")
        assert tmpl.render_empty() == "Hi 5"

    def test_render_empty_required_params_raises(self) -> None:
        tmpl = Template.from_source("""---
params:
  - name = str
---
Hello {{ name }}!""")
        with pytest.raises(ValueError, match="name"):
            tmpl.render_empty()

    def test_render_empty_mixed_defaults_required_raises(self) -> None:
        tmpl = Template.from_source("""---
params:
  - greeting = str := "Hi"
  - name = str
---
{{ greeting }} {{ name }}!""")
        with pytest.raises(ValueError, match="name"):
            tmpl.render_empty()
