"""Tests for md_tmpl block syntax: includes, inline templates, consts, and defaults."""

import textwrap
from pathlib import Path
from typing import Any

import pytest

from md_tmpl import (
    Template,
    load_types,
    TemplateSyntaxError,
)

# ---------------------------------------------------------------------------
# Includes
# ---------------------------------------------------------------------------


class TestIncludes:
    """Tests for {% include %} directives."""

    def test_simple_include(self, tmp_path: Path) -> None:
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
        output = tmpl.render(greeting="hello")
        assert output == "Child: hello"

    def test_iterated_include(self, tmp_path: Path) -> None:
        row = tmp_path / "row.tmpl.md"
        row.write_text(textwrap.dedent("""\
            ---
            params: [label = str]
            ---
            - {{ label }}"""))
        parent = tmp_path / "list.tmpl.md"
        parent.write_text(textwrap.dedent("""\
            ---
            params:
              - items = list(label = str)
            ---
            > {% for item in items %}
            > {% include [row](./row.tmpl.md) with label=item.label %}
            > {% /for %}"""))
        tmpl = Template.from_file(str(parent))
        output = tmpl.render(items=[{"label": "alpha"}, {"label": "beta"}])
        assert output == "- alpha- beta"

    def test_iterated_include_for_syntax(self, tmp_path: Path) -> None:
        """Test {% include ... for item in items %} iterated include syntax."""
        row = tmp_path / "row.tmpl.md"
        row.write_text("""---
params: [item = struct(label = str)]
---
- {{ item.label }}""")
        parent = tmp_path / "list.tmpl.md"
        parent.write_text(textwrap.dedent("""\
            ---
            params:
              - items = list(label = str)
            ---
            > {% include [row](./row.tmpl.md) for item in items %}"""))
        tmpl = Template.from_file(str(parent))
        output = tmpl.render(items=[{"label": "alpha"}, {"label": "beta"}])
        assert output == "- alpha- beta"


# ---------------------------------------------------------------------------
# Inline templates
# ---------------------------------------------------------------------------


class TestInlineTemplates:
    """Tests for {% tmpl %} inline template blocks."""

    def test_inline_template(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - items = list(name = str)
            ---
            > {% tmpl row %}

            ---
            params:
              - name = str
            ---
            * {{ name }}

            > {% /tmpl %}
            > {% for item in items %}
            > {% include row with name=item.name %}
            > {% /for %}"""))
        output = tmpl.render(items=[{"name": "Alice"}, {"name": "Bob"}])
        assert output == "* Alice\n* Bob\n"


# ---------------------------------------------------------------------------
# Raw blocks
# ---------------------------------------------------------------------------


class TestRawBlocks:
    """Tests for {% raw %} blocks."""

    def test_raw_block(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: []
            ---
            > {% raw %}

            {{ not_evaluated }}

            > {% /raw %}"""))
        output = tmpl.render()
        assert output == "{{ not_evaluated }}\n"


# ---------------------------------------------------------------------------
# Comments
# ---------------------------------------------------------------------------


class TestComments:
    """Tests for {# comment #} syntax."""

    def test_comment_stripped(self) -> None:
        tmpl = Template.from_source("""---
params: [name = str]
---
Hello{# a comment #} {{ name }}!""")
        output = tmpl.render(name="world")
        assert output == "Hello world!"


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------


class TestConstants:
    """Tests for consts: block in frontmatter."""

    def test_const_in_body(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            consts:
              - APP = str := "MyApp"

            params: []
            ---
            {{ APP }}"""))
        assert tmpl.render() == "MyApp"

    def test_consts_method(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            consts:
              - VERSION = str := "1.0"
              - MAX = int := 100

            params: []
            ---
            {{ VERSION }} {{ MAX }}"""))
        consts = tmpl.consts()
        assert consts["VERSION"] == "1.0"
        assert consts["MAX"] == 100


# ---------------------------------------------------------------------------
# Imported const iteration (imports: block)
# ---------------------------------------------------------------------------


class TestImportedConstIteration:
    """Iterating over a list const exported by an imported template.

    Mirrors the real ARTIST `artist.SEVERITY_LADDER` usage, where a shared
    template exposes a typed list const (optionally with enum-typed fields)
    that consumers iterate over via `{% for row in stem.CONST %}`.
    """

    def _write_lib(self, tmp_path: Path) -> Path:
        lib = tmp_path / "lib.tmpl.md"
        lib.write_text(textwrap.dedent("""\
            ---
            types:
              - Tier = enum(L0, L1, L2)

            consts:
              - LADDER = list(tier = Tier, desc = str) := [{tier = L0, desc = "theory"}, {tier = L1, desc = "trigger"}]

            params: []
            ---
            (library)"""))
        return lib

    def test_iterate_imported_const_list(self, tmp_path: Path) -> None:
        self._write_lib(tmp_path)
        parent = tmp_path / "parent.tmpl.md"
        parent.write_text(textwrap.dedent("""\
            ---
            imports:
              - [lib](./lib.tmpl.md)

            params: []
            ---
            > {% for row in lib.LADDER %}

            {{ kind(row.tier) }}: {{ row.desc }}

            > {% /for %}"""))
        tmpl = Template.from_file(str(parent))
        output = tmpl.render()
        assert output == "L0: theory\nL1: trigger\n"

    def test_iterate_imported_const_and_use_imported_enum(self, tmp_path: Path) -> None:
        """Regression: the same stem exposes both a typed const and an enum type.

        Typing the import stem as a struct of only-consts previously broke
        access to the enum type via the stem (`kinds(lib.Tier)`). Both must
        work simultaneously.
        """
        self._write_lib(tmp_path)
        parent = tmp_path / "parent.tmpl.md"
        parent.write_text(textwrap.dedent("""\
            ---
            imports:
              - [lib](./lib.tmpl.md)

            params: []
            ---
            > {% for t in kinds(lib.Tier) %}

            - {{ t }}

            > {% /for %}
            > {% for row in lib.LADDER %}

            {{ row.desc }}

            > {% /for %}"""))
        tmpl = Template.from_file(str(parent))
        output = tmpl.render()
        assert output == "- L0\n- L1\n- L2\ntheory\ntrigger\n"


# ---------------------------------------------------------------------------
# Whitespace control
# ---------------------------------------------------------------------------


class TestWhitespaceControl:
    """Tests for whitespace trimming with - markers."""

    def test_trim_left(self) -> None:
        tmpl = Template.from_source("""---
params: [name = str]
---
hello  {{- name }}""")
        assert tmpl.render(name="world") == "helloworld"

    def test_trim_right(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            {{ name -}}  end"""))
        assert tmpl.render(name="world") == "worldend"


# ---------------------------------------------------------------------------
# Type aliases
# ---------------------------------------------------------------------------


class TestTypeAliases:
    """Tests for types: block (type aliases)."""

    def test_type_alias_enum(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
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
        output = tmpl.render(prio="High")
        assert output == "URGENT\n"


# ---------------------------------------------------------------------------
# imported_consts()
# ---------------------------------------------------------------------------


class TestImportedConsts:
    """Tests for imported_consts() method."""

    def test_imported_consts_empty(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            {{ name }}"""))
        assert tmpl.imported_consts() == {}

    def test_imported_consts_with_import(self, tmp_path: Path) -> None:
        # Create a constants template
        consts_tmpl = tmp_path / "config.tmpl.md"
        consts_tmpl.write_text("""---
consts:
  - MAX_RETRIES = int := 3
  - TIMEOUT = str := "30s"

---
Config loaded.
""")
        # Create a template that imports consts
        main_tmpl = tmp_path / "main.tmpl.md"
        main_tmpl.write_text(f"""---
imports: [config]({consts_tmpl})

params: [name = str]
---
{{{{ name }}}} (max={{{{ config.MAX_RETRIES }}}})
""")
        tmpl = Template.from_file(main_tmpl)
        consts = tmpl.imported_consts()
        assert consts.get("config.MAX_RETRIES") == 3
        assert consts.get("config.TIMEOUT") == "30s"


# ---------------------------------------------------------------------------
# Cross-template imports
# ---------------------------------------------------------------------------


class TestCrossTemplateImports:
    """Tests for cross-template type imports."""

    def test_import_types_from_another_template(self, tmp_path: Path) -> None:
        # Create a shared types template
        shared = tmp_path / "shared.tmpl.md"
        shared.write_text("""---
types:
  Status = enum(Active, Inactive)

params: [s = Status]
---
{{ s }}
""")
        # Create a template that imports the shared type
        main = tmp_path / "main.tmpl.md"
        main.write_text(f"""---
imports: [shared]({shared})
params: [status = shared.Status]
---
> {{% match status %}}
> {{% case Active %}}

active

> {{% case Inactive %}}

inactive

> {{% /match %}}
""")
        tmpl = Template.from_file(main)
        types = load_types(main)
        # The imported type should be available and template should work
        assert tmpl.declarations()


# ---------------------------------------------------------------------------
# Defaults introspection — value checks (ported from Go)
# ---------------------------------------------------------------------------


class TestDefaultsIntrospection:
    """Tests for default value retrieval and type accuracy."""

    def test_defaults_value_types(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - name = str := "World"
              - count = int := 5
              - flag = bool
            ---
            {{ name }} {{ count }} {{ flag }}"""))
        defaults = tmpl.defaults()
        assert defaults["name"] == "World"
        assert defaults["count"] == 5
        assert "flag" not in defaults, "flag has no default"

    def test_defaults_partial_override(self) -> None:
        """Only override some defaults, let others use their default values."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - name = str := "World"
              - greeting = str
            ---
            {{ greeting }} {{ name }}"""))
        result = tmpl.render(greeting="Hello")
        assert result == "Hello World"

    def test_defaults_empty(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        defaults = tmpl.defaults()
        assert len(defaults) == 0

    def test_defaults_bool_value(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - enabled = bool := true
            ---
            > {% if enabled %}

            on

            > {% /if %}"""))
        assert tmpl.defaults()["enabled"] is True
        assert tmpl.render() == "on\n"


# ---------------------------------------------------------------------------
# Delimiters inside quoted strings in frontmatter default values
# ---------------------------------------------------------------------------


class TestEmbeddedDelimiterDefaults:
    """Regression tests: delimiters inside quoted default-value literals.

    Commas ``,`` and the bracket family ``()[]{}<>`` that appear **inside**
    quoted string literals (``"..."`` or ``'...'``) in a frontmatter default
    (``:= ...``) must be treated as literal characters, not as field/element
    separators. Python inherits this quote-aware parsing through the PyO3
    binding to the Rust core (``split_at_depth_zero``); these end-to-end tests
    lock the behavior in at the binding boundary via ``Template.defaults()``.

    Each test maps to a case in the canonical regression matrix (T1–T8).
    """

    @staticmethod
    def _defaults(params: str) -> dict[str, Any]:
        """Compile a params-only template and return its parsed defaults.

        ``from_source_allowing_unused`` is used so the params need not be
        referenced in the body — these tests only introspect the parsed
        default values, which is exactly the code path under test.
        """
        source = textwrap.dedent("""\
            ---
            params:
            {params}
            ---
            body""").format(params=params)
        return Template.from_source_allowing_unused(source).defaults()

    def test_t1_list_of_strings_with_commas(self) -> None:
        """T1: commas inside list string items are not element separators."""
        defaults = self._defaults('  - x = list(str) := ["a, b", "c, d"]')
        assert defaults["x"] == ["a, b", "c, d"]

    def test_t2_struct_field_with_comma(self) -> None:
        """T2: a comma inside a struct string field stays in that field."""
        defaults = self._defaults(
            '  - x = struct(msg = str, n = int) := {msg = "a, b", n = 1}'
        )
        assert defaults["x"] == {"msg": "a, b", "n": 1}

    def test_t3_list_of_records_with_commas(self) -> None:
        """T3: commas inside record string fields don't split records."""
        defaults = self._defaults(
            "  - x = list(name = str, note = str) := "
            '[{name = "x", note = "p, q, r"}, {name = "y", note = "s"}]'
        )
        records = defaults["x"]
        assert len(records) == 2
        assert records[0] == {"name": "x", "note": "p, q, r"}
        assert records[1] == {"name": "y", "note": "s"}

    def test_t4_list_items_with_brackets(self) -> None:
        """T4: bracket family inside quoted items is preserved verbatim."""
        defaults = self._defaults('  - x = list(str) := ["a[b]c", "d{e}f", "g(h)i"]')
        assert defaults["x"] == ["a[b]c", "d{e}f", "g(h)i"]

    def test_t5_single_quoted_items_with_commas(self) -> None:
        """T5: single-quoted strings are quote-aware just like double."""
        defaults = self._defaults("  - x = list(str) := ['a, b', 'c']")
        assert defaults["x"] == ["a, b", "c"]

    def test_t6_unicode_items_intact(self) -> None:
        """T6: unicode (em-dash, emoji) and embedded commas survive."""
        defaults = self._defaults(
            '  - x = list(str) := ["Theory — not a finding", "✅ done, ok"]'
        )
        assert defaults["x"] == ["Theory — not a finding", "✅ done, ok"]

    def test_t7_empty_strings_in_records(self) -> None:
        """T7: empty quoted strings parse as empty, commas still literal."""
        defaults = self._defaults(
            "  - x = list(name = str, note = str) := "
            '[{name = "", note = ""}, {name = "a", note = "y, z"}]'
        )
        records = defaults["x"]
        assert len(records) == 2
        assert records[0] == {"name": "", "note": ""}
        assert records[1] == {"name": "a", "note": "y, z"}

    def test_t8_severity_ladder_shape(self) -> None:
        """T8: faithful reduced SEVERITY_LADDER — list of enum-tagged records
        with prose fields containing commas, em-dashes, and brackets.
        """
        source = textwrap.dedent("""\
            ---
            types:
              - Tier = enum(L0, L1, L2)

            params:
              - ladder = list(tier = Tier, short = str, proves = str) := [{tier = L0, short = "theory", proves = "Nothing proven — just a hypothesis, unverified"}, {tier = L1, short = "trigger", proves = "Crash, panic, or sanitizer report (e.g. [heap-overflow])"}]
            ---
            body""")
        ladder = Template.from_source_allowing_unused(source).defaults()["ladder"]
        assert len(ladder) == 2
        assert ladder[0]["tier"] == "L0"
        assert ladder[0]["short"] == "theory"
        # Comma- and em-dash-bearing prose must stay in a single field.
        assert ladder[0]["proves"] == "Nothing proven — just a hypothesis, unverified"
        assert ladder[1]["tier"] == "L1"
        # Commas and brackets inside the prose must not split the record.
        assert ladder[1]["proves"] == (
            "Crash, panic, or sanitizer report (e.g. [heap-overflow])"
        )

    def test_e1_single_quote_char_inside_double_quoted_item(self) -> None:
        """E1: an apostrophe inside a double-quoted item is a literal char and
        does not open a quote context that would hide the following comma.
        """
        defaults = self._defaults('  - x = list(str) := ["it\'s, fine", "ok"]')
        assert defaults["x"] == ["it's, fine", "ok"]

    def test_e2_double_quotes_inside_single_quoted_item(self) -> None:
        """E2: double quotes nested inside a single-quoted item are literal,
        and the comma between them stays inside the item.
        """
        defaults = self._defaults('  - x = list(str) := [\'say "hi", bye\', "z"]')
        assert defaults["x"] == ['say "hi", bye', "z"]

    def test_e3_nested_list_of_lists(self) -> None:
        """E3: commas inside quoted items of a nested list(list(str)) default
        never split the inner or outer lists.
        """
        defaults = self._defaults(
            '  - x = list(list(str)) := [["a, b"], ["c, d", "e"]]'
        )
        assert defaults["x"] == [["a, b"], ["c, d", "e"]]

    def test_e4_leading_trailing_spaces_preserved(self) -> None:
        """E4: whitespace inside a quoted item is preserved verbatim."""
        defaults = self._defaults('  - x = list(str) := [" a, b "]')
        assert defaults["x"] == [" a, b "]

    def test_e5a_hash_without_leading_space_is_literal(self) -> None:
        """E5a: a `#` with no leading space is an ordinary character, kept."""
        defaults = self._defaults('  - x = str := "a#b,c"')
        assert defaults["x"] == "a#b,c"

    def test_e5b_space_hash_starts_yaml_comment(self) -> None:
        """E5b: a ` #` (space-hash) starts a YAML inline comment (the core is
        not md-tmpl-string-aware), truncating the scalar to ``x = str := "a`` —
        an unterminated string literal that is rejected as an invalid default.
        Wrap the whole declaration in outer YAML quotes to keep a literal ` #`
        (see E5c).
        """
        with pytest.raises(TemplateSyntaxError, match="strings must be quoted"):
            self._defaults('  - x = str := "a # b, c"')

    def test_e5c_outer_yaml_quotes_protect_hash(self) -> None:
        """E5c: outer YAML quotes around the whole declaration protect an inner
        ` #`, recovering the full default value.
        """
        defaults = self._defaults('  - "x = str := \\"a # b, c\\""')
        assert defaults["x"] == "a # b, c"

    def test_e5d_space_hash_outside_string_is_stripped(self) -> None:
        """E5d: a ` #` outside any string literal starts a YAML inline comment
        and is stripped, so the numeric default still parses.
        """
        defaults = self._defaults("  - x = int := 3 # the retry count")
        assert defaults["x"] == 3


# ---------------------------------------------------------------------------
# Constants — empty and multiple types (ported from Go)
# ---------------------------------------------------------------------------


class TestConstantsExtended:
    """Additional tests for consts: block."""

    def test_consts_empty_when_not_declared(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [x = str]
            ---
            {{ x }}"""))
        consts = tmpl.consts()
        assert len(consts) == 0

    def test_consts_multiple_types(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            consts:
              - MAX = int := 100
              - GREETING = str := "hello"
              - ENABLED = bool := true

            params: []
            ---
            {{ MAX }} {{ GREETING }} {{ ENABLED }}"""))
        consts = tmpl.consts()
        assert consts["MAX"] == 100
        assert consts["GREETING"] == "hello"
        assert consts["ENABLED"] is True
        output = tmpl.render()
        assert output == "100 hello true"


# ---------------------------------------------------------------------------
# elif branches (ported from Go)
# ---------------------------------------------------------------------------


class TestElifBranches:
    """Tests for if/elif/else branching."""

    def test_elif_all_branches(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [level = int]
            ---
            > {% if level == 1 %}

            Low

            > {% elif level == 2 %}

            Medium

            > {% else %}

            High

            > {% /if %}"""))
        assert tmpl.render(level=1) == "Low\n"
        assert tmpl.render(level=2) == "Medium\n"
        assert tmpl.render(level=3) == "High\n"


# -- for...else support ----------------------------------------------------


class TestForElse:
    """Tests for {% for...else %} via the Python binding.

    When the list is empty, the else body renders.
    When the list has items, the loop body renders and else is skipped.
    """

    def test_for_else_empty_list(self) -> None:
        """Empty list renders the else body."""
        tmpl = Template.from_source("""---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ item.name }}

> {% else %}

No items

> {% /for %}""")
        result = tmpl.render(items=[])
        assert result.strip() == "No items"

    def test_for_else_non_empty_list(self) -> None:
        """Non-empty list renders the loop body, not else."""
        tmpl = Template.from_source("""---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ item.name }}

> {% else %}

No items

> {% /for %}""")
        result = tmpl.render(items=[{"name": "Alice"}])
        assert "Alice" in result
        assert "No items" not in result

    def test_for_without_else_still_works(self) -> None:
        """Basic for-loop without else continues to work."""
        tmpl = Template.from_source("""---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ item.name }}

> {% /for %}""")
        result = tmpl.render(items=[{"name": "Bob"}])
        assert result.strip() == "Bob"
