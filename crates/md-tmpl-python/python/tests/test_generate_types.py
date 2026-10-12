"""Tests for md_tmpl generated type sources, models, aliases, and duck typing."""

import sys
import textwrap
from pathlib import Path
from typing import Any

from md_tmpl import (
    Template,
    load_types,
    template,
)

# ---------------------------------------------------------------------------
# Generated model __dict__ property
# ---------------------------------------------------------------------------


class TestGeneratedModelDict:
    """Tests for the __dict__ property on generated model classes."""

    def test_model_dict_property(self, tmp_path: Path) -> None:
        path = tmp_path / "item.tmpl.md"
        path.write_text("""---
params:
  - items = list(name = str, count = int)
---
> {% for item in items %}

{{ item.name }}

> {% /for %}
""")
        types = load_types(path)
        # Find the generated item model class (the one with name+count fields)
        item_classes = [
            n for n in dir(types) if "ItemsItem" in n and not n.startswith("_")
        ]
        assert item_classes, f"No ItemsItem class found in {dir(types)}"
        ItemCls = getattr(types, item_classes[0])
        obj = ItemCls(name="test", count=5)
        d = obj.__dict__
        assert d["name"] == "test"
        assert d["count"] == 5


# ---------------------------------------------------------------------------
# Generated params render() method
# ---------------------------------------------------------------------------


class TestGeneratedParamsRender:
    """Tests for the render() method on generated params classes."""

    def test_params_render_method(self, tmp_path: Path) -> None:
        path = tmp_path / "hello.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            Hello {{ name }}!"""))
        types = load_types(path)
        tmpl = Template.from_file(path)
        Hello = types.Hello
        params = Hello(name="world")
        result = params.render(tmpl)
        assert result == "Hello world!"

    def test_params_render_with_template(self, tmp_path: Path) -> None:
        path = tmp_path / "greeting.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            Hi {{ name }}!"""))
        types = load_types(path)
        tmpl = Template.from_file(path)
        params = types.Greeting(name="test")
        result = params.render(template=tmpl)
        assert result == "Hi test!"


# ---------------------------------------------------------------------------
# Type alias — generated type via load_types
# ---------------------------------------------------------------------------


class TestTypeAliasGeneration:
    """Test that type aliases generate loadable Python types."""

    def test_type_alias_generates_class(self, tmp_path: Path) -> None:
        path = tmp_path / "review.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            types:
              - Priority = enum(High, Medium, Low)

            params:
              - prio = Priority
            ---
            > {% match prio %}
            > {% case High %}

            URGENT

            > {% case Medium %}

            NORMAL

            > {% case Low %}

            MINOR

            > {% /match %}"""))
        types = load_types(path)
        assert hasattr(types, "Priority"), f"Expected Priority class, got {dir(types)}"
        # The Priority type should have variants
        assert hasattr(types.Priority, "High")

    def test_type_alias_variant_renders(self, tmp_path: Path) -> None:
        path = tmp_path / "review.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            types:
              - Priority = enum(High, Medium, Low)

            params:
              - prio = Priority
            ---
            > {% match prio %}
            > {% case High %}

            URGENT

            > {% case Medium %}

            NORMAL

            > {% case Low %}

            MINOR

            > {% /match %}"""))
        t = template(str(path))
        result = t.render(prio=t.Priority.High)
        assert result == "URGENT\n"


# ---------------------------------------------------------------------------
# generate_types_source — static type generation for mypy/pyright
# ---------------------------------------------------------------------------


class TestGenerateTypesSource:
    """Tests for generate_types_source() — Python source code generation."""

    def test_basic_source_generation(self, simple_template_path: Path) -> None:
        """Generated source contains required imports and dataclass."""
        from md_tmpl import generate_types_source

        source = generate_types_source(simple_template_path)
        assert "from __future__ import annotations" in source
        assert "from dataclasses import dataclass" in source
        assert "from md_tmpl import Template, Variants" in source
        assert "@dataclass" in source
        assert "class Greeting:" in source
        assert "    name: str" in source

    def test_source_has_render_method(self, simple_template_path: Path) -> None:
        """Generated params class has a typed render() method."""
        from md_tmpl import generate_types_source

        source = generate_types_source(simple_template_path)
        assert "def render(self, template: Template | None = None) -> str:" in source
        assert "render_dict(dataclasses.asdict(self))" in source

    def test_source_is_valid_python(self, simple_template_path: Path) -> None:
        """Generated source compiles without syntax errors."""
        from md_tmpl import generate_types_source

        source = generate_types_source(simple_template_path)
        compile(source, "<test>", "exec")  # Raises SyntaxError if invalid.

    def test_source_has_docstring(self, simple_template_path: Path) -> None:
        """Generated source has a module-level docstring."""
        from md_tmpl import generate_types_source

        source = generate_types_source(simple_template_path)
        assert "Auto-generated typed stubs" in source
        assert "Do not edit" in source

    def test_source_has_all(self, simple_template_path: Path) -> None:
        """Generated source has __all__ export list."""
        from md_tmpl import generate_types_source

        source = generate_types_source(simple_template_path)
        assert '__all__ = ["Greeting"]' in source

    def test_source_with_defaults(self, default_template_path: Path) -> None:
        """Params with defaults use field(default=...)."""
        from md_tmpl import generate_types_source

        source = generate_types_source(default_template_path)
        assert "from dataclasses import dataclass, field" in source
        assert 'name: str = field(default="World")' in source
        assert "count: int = field(default=1)" in source

    def test_source_with_enum(self, enum_template_path: Path) -> None:
        """Enum types generate Variants subclasses."""
        from md_tmpl import generate_types_source

        source = generate_types_source(enum_template_path)
        assert "class Outcome(Variants):" in source
        assert "Rejected = ()" in source
        assert "NeedsWork = ()" in source
        # Struct variant with fields.
        assert 'Confirmed = {"evidence": str}' in source

    def test_source_with_struct(self, struct_template_path: Path) -> None:
        """Struct types generate @dataclass model classes."""
        from md_tmpl import generate_types_source

        source = generate_types_source(struct_template_path)
        assert "@dataclass" in source
        assert "class Config:" in source
        assert "    host: str" in source
        assert "    port: int" in source

    def test_source_with_list_of_structs(self, list_template_path: Path) -> None:
        """List<struct> generates an Item model class."""
        from md_tmpl import generate_types_source

        source = generate_types_source(list_template_path)
        assert "Item" in source  # Should generate a ...Item class
        assert "title: str" in source
        assert "priority: str" in source

    def test_source_all_includes_nested_types(self, enum_template_path: Path) -> None:
        """__all__ includes both the params class and nested types."""
        from md_tmpl import generate_types_source

        source = generate_types_source(enum_template_path)
        assert '"Status"' in source or '"Outcome"' in source

    def test_source_exec_and_render(self, simple_template_path: Path) -> None:
        """Generated source can be exec'd and the render() method works."""
        from md_tmpl import generate_types_source

        source = generate_types_source(simple_template_path)
        namespace: dict[str, Any] = {}
        exec(source, namespace)  # noqa: S102

        Greeting = namespace["Greeting"]
        params = Greeting(name="ExecTest")

        # The render method should work (loads from file).
        result = params.render()
        assert result == "Hello ExecTest!"

    def test_source_exec_with_defaults(self, default_template_path: Path) -> None:
        """Generated source with defaults can be exec'd with default values."""
        from md_tmpl import generate_types_source

        source = generate_types_source(default_template_path)
        namespace: dict[str, Any] = {}
        exec(source, namespace)  # noqa: S102

        Defaults = namespace["Defaults"]

        # Construct with defaults only.
        params = Defaults()
        result = params.render()
        assert result == "Hello World, count=1!"

        # Override defaults.
        params2 = Defaults(name="Custom", count=99)
        result2 = params2.render()
        assert result2 == "Hello Custom, count=99!"

    def test_source_exec_with_explicit_template(
        self, simple_template_path: Path
    ) -> None:
        """render() accepts an explicit Template argument."""
        from md_tmpl import Template, generate_types_source

        source = generate_types_source(simple_template_path)
        namespace: dict[str, Any] = {}
        exec(source, namespace)  # noqa: S102

        tmpl = Template.from_file(str(simple_template_path))
        Greeting = namespace["Greeting"]
        result = Greeting(name="Explicit").render(template=tmpl)
        assert result == "Hello Explicit!"

    def test_source_exec_enum_rendering(self, enum_template_path: Path) -> None:
        """Generated enum type can be used with render()."""
        from md_tmpl import generate_types_source

        source = generate_types_source(enum_template_path)
        namespace: dict[str, Any] = {}
        exec(source, namespace)  # noqa: S102

        Status = namespace["Status"]
        # Unit variant.
        params = Status(outcome="Rejected")
        result = params.render()
        assert result == "NO\n"

    def test_source_with_optional_type(self, tmp_path: Path) -> None:
        """Optional types get Optional[T] annotation and Optional import."""
        tmpl_path = tmp_path / "optional.tmpl.md"
        tmpl_path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
              - title = option(str)
            ---
            Hello {{ name }}{% if has(title) %} {{ title }}{% /if %}"""))

        from md_tmpl import generate_types_source

        source = generate_types_source(tmpl_path)
        assert "from typing import Any, Optional" in source
        assert "title: str | None" in source
        assert "name: str" in source

    def test_source_empty_params(self, tmp_path: Path) -> None:
        """Template with no params still generates a class with render()."""
        tmpl_path = tmp_path / "empty.tmpl.md"
        tmpl_path.write_text(textwrap.dedent("""\
            ---
            params: []
            ---
            Static content"""))

        from md_tmpl import generate_types_source

        source = generate_types_source(tmpl_path)
        assert "class Empty:" in source
        assert "def render(self" in source

        # And it should actually render.
        namespace: dict[str, Any] = {}
        exec(source, namespace)  # noqa: S102
        result = namespace["Empty"]().render()
        assert result == "Static content"

    def test_source_multiple_types(self, tmp_path: Path) -> None:
        """All scalar types get correct annotations."""
        tmpl_path = tmp_path / "multi.tmpl.md"
        tmpl_path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
              - count = int
              - score = float
              - active = bool
            ---
            {{ name }} {{ count }} {{ score }} {{ active }}"""))

        from md_tmpl import generate_types_source

        source = generate_types_source(tmpl_path)
        assert "name: str" in source
        assert "count: int" in source
        assert "score: float" in source
        assert "active: bool" in source

    def test_source_write_to_file_and_import(self, tmp_path: Path) -> None:
        """Generated source can be written and imported as a module."""
        import importlib.util

        tmpl_path = tmp_path / "greet.tmpl.md"
        tmpl_path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            Hi {{ name }}!"""))

        from md_tmpl import generate_types_source

        source = generate_types_source(tmpl_path)
        py_path = tmp_path / "greet_types.py"
        py_path.write_text(source)

        # Import the generated module.
        spec = importlib.util.spec_from_file_location("greet_types", str(py_path))
        assert spec is not None
        assert spec.loader is not None
        mod = importlib.util.module_from_spec(spec)
        sys.modules["greet_types"] = mod  # Required for @dataclass + PEP 563.
        try:
            spec.loader.exec_module(mod)

            # Use the imported class.
            params = mod.Greet(name="Module")
            result = params.render()
            assert result == "Hi Module!"
        finally:
            sys.modules.pop("greet_types", None)


# ---------------------------------------------------------------------------
# Duck typing / structural typing — extra fields are silently ignored
# ---------------------------------------------------------------------------


class TestDuckTypingExtraFields:
    """Tests that md-tmpl uses structural (duck) typing for struct and list values.

    Extra fields in struct dicts are silently ignored — only declared fields
    are checked for presence and type.  This applies recursively through nested
    structs, list items, and type aliases.
    """

    # -- struct param with extra fields ------------------------------------

    def test_struct_extra_fields_ignored(self) -> None:
        """Extra dict keys in a struct param are silently dropped."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - user = struct(name = str, age = int)
            ---
            {{ user.name }} is {{ user.age }}"""))
        output = tmpl.render(
            user={
                "name": "Alice",
                "age": 30,
                "email": "alice@example.com",  # extra
                "is_admin": True,  # extra
            }
        )
        assert output == "Alice is 30"

    def test_struct_extra_fields_via_render_dict(self) -> None:
        """render_dict also ignores extra struct fields."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - cfg = struct(host = str, port = int)
            ---
            {{ cfg.host }}:{{ cfg.port }}"""))
        output = tmpl.render_dict(
            {
                "cfg": {
                    "host": "localhost",
                    "port": 8080,
                    "timeout_ms": 5000,  # extra
                }
            }
        )
        assert output == "localhost:8080"

    def test_struct_extra_fields_via_render_json(self) -> None:
        """render_json also ignores extra struct fields."""
        import json

        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - item = struct(id = int, label = str)
            ---
            {{ item.id }}: {{ item.label }}"""))
        payload = json.dumps(
            {
                "item": {
                    "id": 1,
                    "label": "widget",
                    "color": "red",  # extra
                    "weight": 3.5,  # extra
                }
            }
        )
        output = tmpl.render_json(payload)
        assert output == "1: widget"

    # -- list items with extra fields --------------------------------------

    def test_list_items_extra_fields_ignored(self) -> None:
        """Extra fields on list-item structs are silently ignored."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - tasks = list(title = str, done = bool)
            ---
            > {% for t in tasks %}

            - {{ t.title }} ({{ t.done }})

            > {% /for %}"""))
        output = tmpl.render(
            tasks=[
                {
                    "title": "Write docs",
                    "done": False,
                    "assignee": "Bob",
                    "priority": 1,
                },
                {"title": "Ship it", "done": True, "eta": "tomorrow"},
            ]
        )
        assert output == "- Write docs (false)\n- Ship it (true)\n"

    def test_list_items_extra_fields_via_render_dict(self) -> None:
        """render_dict also ignores extra fields on list items."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - items = list(name = str)
            ---
            > {% for i in items %}

            {{ i.name }}

            > {% /for %}"""))
        output = tmpl.render_dict(
            {
                "items": [
                    {"name": "alpha", "extra_key": 999},
                    {"name": "beta", "another": "value"},
                ]
            }
        )
        assert output == "alpha\nbeta\n"

    # -- type alias with duck typing ---------------------------------------

    def test_type_alias_struct_extra_fields(self) -> None:
        """A type-alias struct still ignores extra fields."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            types:
              - Address = struct(city = str, zip = str)

            params:
              - addr = Address
            ---
            {{ addr.city }} {{ addr.zip }}"""))
        output = tmpl.render(
            addr={
                "city": "Berlin",
                "zip": "10115",
                "country": "DE",  # extra
                "lat": 52.52,  # extra
            }
        )
        assert output == "Berlin 10115"

    def test_type_alias_list_and_struct_extra_fields_combined(self) -> None:
        """Type-alias struct + inline list param both ignore extra fields."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            types:
              - Entry = struct(key = str, value = str)

            params:
              - primary = Entry
              - tags = list(name = str)
            ---
            {{ primary.key }}={{ primary.value }}

            > {% for t in tags %}

            #{{ t.name }}

            > {% /for %}"""))
        output = tmpl.render(
            primary={"key": "a", "value": "1", "comment": "first"},
            tags=[
                {"name": "foo", "extra_meta": True},
                {"name": "bar", "color": "red"},
            ],
        )
        assert output == "a=1\n#foo\n#bar\n"

    # -- nested struct with extra fields at every depth --------------------

    def test_nested_struct_extra_fields_at_every_depth(self) -> None:
        """Extra fields are ignored at each nesting level of structs."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - outer = struct(label = str, inner = struct(x = int, y = int))
            ---
            {{ outer.label }}: ({{ outer.inner.x }}, {{ outer.inner.y }})"""))
        output = tmpl.render(
            outer={
                "label": "point",
                "color": "blue",  # extra on outer
                "inner": {
                    "x": 10,
                    "y": 20,
                    "z": 30,  # extra on inner
                    "comment": "nope",  # extra on inner
                },
                "metadata": {},  # extra on outer
            }
        )
        assert output == "point: (10, 20)"

    def test_triple_nested_struct_extra_fields(self) -> None:
        """Three levels of nesting — extras ignored at every level."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - a = struct(name = str, b = struct(value = int, c = struct(flag = bool)))
            ---
            {{ a.name }}/{{ a.b.value }}/{{ a.b.c.flag }}"""))
        output = tmpl.render(
            a={
                "name": "root",
                "extra_a": "ignored",
                "b": {
                    "value": 42,
                    "extra_b": [1, 2, 3],
                    "c": {
                        "flag": True,
                        "extra_c": None,
                    },
                },
            }
        )
        assert output == "root/42/true"

    def test_list_of_nested_structs_extra_fields(self) -> None:
        """List items containing nested structs also ignore extras at all levels."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - rows = list(id = int, detail = struct(text = str))
            ---
            > {% for r in rows %}

            {{ r.id }}: {{ r.detail.text }}

            > {% /for %}"""))
        output = tmpl.render(
            rows=[
                {"id": 1, "detail": {"text": "hello", "extra": 99}, "bonus": "x"},
                {"id": 2, "detail": {"text": "world", "color": "green"}, "tag": "y"},
            ]
        )
        assert output == "1: hello\n2: world\n"
