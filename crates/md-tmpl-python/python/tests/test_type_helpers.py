"""Tests for md_tmpl template() helper, import hook, load_types, and template params."""

import sys
import textwrap
from pathlib import Path

import pytest

from md_tmpl import (
    Template,
    load_types,
    template,
)

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
        assert rejected._md_tmpl_tag == "Rejected"

        # Struct variant — callable constructor.
        confirmed = t.Outcome.Confirmed(evidence="proof")
        assert confirmed._md_tmpl_tag == "Confirmed"
        assert confirmed._md_tmpl_fields["evidence"] == "proof"

    def test_render_with_generated_enum(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        output = t.render(outcome=t.Outcome.Rejected)
        assert output == "NO\n"

    def test_render_with_struct_variant(self, enum_template_path: Path) -> None:
        t = template(str(enum_template_path))
        output = t.render(outcome=t.Outcome.Confirmed(evidence="found it"))
        assert output == "YES: found it\n"

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
    """Tests for md_tmpl_import_hook()."""

    def test_import_hook_installs(self) -> None:
        from md_tmpl import md_tmpl_import_hook

        # Should not raise.
        md_tmpl_import_hook(search_paths=["/nonexistent"])

        # Check it's in sys.meta_path.
        from md_tmpl._import_hook import _TemplateFinder

        finders = [f for f in sys.meta_path if isinstance(f, _TemplateFinder)]
        assert len(finders) == 1

    def test_import_hook_idempotent(self) -> None:
        from md_tmpl import md_tmpl_import_hook

        md_tmpl_import_hook(search_paths=["/tmp"])
        md_tmpl_import_hook(search_paths=["/tmp"])

        from md_tmpl._import_hook import _TemplateFinder

        finders = [f for f in sys.meta_path if isinstance(f, _TemplateFinder)]
        assert len(finders) == 1, "Should not duplicate the hook"

    def test_import_template_file(self, simple_template_path: Path) -> None:
        from md_tmpl import md_tmpl_import_hook
        import importlib

        # Install hook with the temp directory.
        md_tmpl_import_hook(search_paths=[str(simple_template_path.parent)])

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
        from md_tmpl import md_tmpl_import_hook
        import os as os_mod

        md_tmpl_import_hook()

        # These should still work fine.
        assert os_mod.path is not None
        assert sys.version is not None


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
# Template-as-param (tmpl() type)
# ---------------------------------------------------------------------------


class TestTemplateAsParam:
    """Tests for passing Template objects as tmpl() parameters."""

    def test_basic_tmpl_param_rendering(self) -> None:
        """A Template passed as a param should be usable via {% include %}."""
        helper = Template.from_source("""---
params: [name = str]
---
Hello {{ name }}!""")
        main = Template.from_source("""---
params: [greet = tmpl(name = str)]
---
> {% include greet with name="World" %}""")
        result = main.render(greet=helper)
        assert result == "Hello World!"

    def test_tmpl_param_type_mismatch(self) -> None:
        """Template with wrong params should fail type checking."""
        wrong = Template.from_source(textwrap.dedent("""\
            ---
            params: [age = int]
            ---
            Age: {{ age }}"""))
        main = Template.from_source("""---
params: [greet = tmpl(name = str)]
---
> {% include greet with name="World" %}""")
        with pytest.raises(ValueError, match="type mismatch"):
            main.render(greet=wrong)

    def test_tmpl_param_with_defaults(self) -> None:
        """Template with extra defaulted params should still match."""
        helper = Template.from_source("""---
params: [name = str, greeting = str := "Hi"]
---
{{ greeting }} {{ name }}!""")
        main = Template.from_source("""---
params: [greet = tmpl(name = str)]
---
> {% include greet with name="World" %}""")
        result = main.render(greet=helper)
        assert result == "Hi World!"

    def test_nested_tmpl_params(self) -> None:
        """Nested template-as-param should work."""
        inner = Template.from_source(textwrap.dedent("""\
            ---
            params: [val = str]
            ---
            Inner: {{ val }}"""))
        middle = Template.from_source("""---
params: [target = tmpl(val = str), value = str]
---
> {% include target with val=value %}""")
        main = Template.from_source("""---
params:
  - processor = tmpl(target = tmpl(val = str), value = str)
  - callback = tmpl(val = str)
---
> {% include processor with target=callback, value="Success" %}""")
        result = main.render(processor=middle, callback=inner)
        assert result == "Inner: Success"

    def test_non_template_as_tmpl_param_raises(self) -> None:
        """Passing a non-Template value for a tmpl param should fail."""
        main = Template.from_source("""---
params: [greet = tmpl(name = str)]
---
> {% include greet with name="World" %}""")
        with pytest.raises(ValueError, match="type mismatch"):
            main.render(greet="not a template")

    def test_tmpl_param_for_each(self) -> None:
        """Template-as-param with {% include ... for item in list %}."""
        row = Template.from_source("""---
params: [item = struct(label = str)]
---
- {{ item.label }}
""")
        parent = Template.from_source("""---
params:
  - row = tmpl(item = struct(label = str))
  - items = list(label = str)
---
> {% include row for item in items %}
""")
        result = parent.render(row=row, items=[{"label": "alpha"}, {"label": "beta"}])
        assert result == "- alpha\n- beta\n"

    def test_tmpl_param_contract_rejects_missing_params(self) -> None:
        """Include without required with= vars should fail."""
        child = Template.from_source("""---
params: [title = str, count = int]
---
{{ title }} ({{ count }})""")
        parent = Template.from_source("""---
params:
  - widget = tmpl(title = str, count = int)
---
> {% include widget %}
""")
        with pytest.raises(ValueError):
            parent.render(widget=child)

    def test_tmpl_param_from_file(self, tmp_path: Path) -> None:
        """A Template loaded from file can be passed as a tmpl param."""
        child_path = tmp_path / "child.tmpl.md"
        child_path.write_text(textwrap.dedent("""\
            ---
            params: [msg = str]
            ---
            [{{ msg }}]"""))
        child = Template.from_file(str(child_path))
        parent = Template.from_source("""---
params:
  - widget = tmpl(msg = str)
  - text = str
---
> {% include widget with msg=text %}
""")
        result = parent.render(widget=child, text="hello")
        assert result == "[hello]"


# ---------------------------------------------------------------------------
# Enum member conversion
# ---------------------------------------------------------------------------


class TestEnumMemberConversion:
    """Tests for Python enum.Enum -> template value conversion."""

    def test_enum_member_as_unit_variant(self) -> None:
        import enum

        class Color(enum.Enum):
            Red = "Red"
            Blue = "Blue"

        tmpl = Template.from_source("""---
params:
  - status = enum(Red, Blue)
---
> {% match status %}
> {% case Red %}

red

> {% case Blue %}

blue

> {% /match %}""")
        assert tmpl.render(status=Color.Red) == "red\n"


# ---------------------------------------------------------------------------
# load_types — params class render and __dict__
# ---------------------------------------------------------------------------


class TestLoadTypesExtended:
    """Extended tests for load_types generated classes."""

    def test_generated_params_dict_property(self, tmp_path: Path) -> None:
        path = tmp_path / "greeting.tmpl.md"
        path.write_text("""---
params:
  - name = str
  - count = int
---
{{ name }} {{ count }}""")
        types = load_types(path)
        Greeting = types.Greeting
        params = Greeting(name="Alice", count=42)
        d = params.__dict__
        assert d["name"] == "Alice"
        assert d["count"] == 42

    def test_generated_params_repr(self, tmp_path: Path) -> None:
        path = tmp_path / "greeting.tmpl.md"
        path.write_text(textwrap.dedent("""\
            ---
            params:
              - name = str
            ---
            {{ name }}"""))
        types = load_types(path)
        params = types.Greeting(name="world")
        r = repr(params)
        assert "Greeting" in r
        assert "world" in r


# ---------------------------------------------------------------------------
# template() helper — generated model usage
# ---------------------------------------------------------------------------


class TestTemplateHelperExtended:
    """Extended tests for template() generated models."""

    def test_generated_model_used_in_render(self, list_template_path: Path) -> None:
        """Generated item models can be used in render_dict."""
        t = template(str(list_template_path))
        type_names = list(t._types.keys())
        item_classes = [n for n in type_names if "Item" in n]
        assert item_classes, f"Expected item classes, got {type_names}"
        ItemCls = getattr(t, item_classes[0])
        item = ItemCls(title="Task 1", priority="High")
        # Use __dict__ to pass as parameter
        output = t.render(tasks=[item.__dict__])
        assert "Task 1" in output
        assert "High" in output
