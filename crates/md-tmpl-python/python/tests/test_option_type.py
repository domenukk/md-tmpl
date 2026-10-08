"""Tests for md_tmpl Option type support."""

from md_tmpl import (
    Template,
)

# -- option(T) support -------------------------------------------------------


class TestOptionType:
    """Tests for option(T) type support in the Python binding.

    option(T) is a first-class type with a transparent representation:
    Python None maps to the engine's absent value, and any other value is
    the present inner value directly (no wrapper).
    """

    def test_option_none_via_match(self) -> None:
        """Passing Python None renders the None arm of a match block."""
        tmpl = Template.from_source("""---
params:
  - label = option(str)
---
> {% match label %}
> {% case Some %}

got: {{ label }}

> {% case None %}

empty

> {% /match %}""")
        result = tmpl.render(label=None)
        assert result.strip() == "empty"

    def test_option_some_via_match(self) -> None:
        """Passing a Some struct renders the Some arm of a match block."""
        tmpl = Template.from_source("""---
params:
  - label = option(str)
---
> {% match label %}
> {% case Some %}

got: {{ label }}

> {% case None %}

empty

> {% /match %}""")
        result = tmpl.render(label="hello")
        assert "got: hello" in result

    def test_option_none_via_has(self) -> None:
        """has() returns false for None, rendering the else branch."""
        tmpl = Template.from_source("""---
params:
  - label = option(str)
---
> {% if has(label) %}

got: {{ label }}

> {% else %}

empty

> {% /if %}""")
        result = tmpl.render(label=None)
        assert result.strip() == "empty"

    def test_option_some_via_has(self) -> None:
        """has() returns true for Some, rendering the if branch."""
        tmpl = Template.from_source("""---
params:
  - label = option(str)
---
> {% if has(label) %}

got: {{ label }}

> {% else %}

empty

> {% /if %}""")
        result = tmpl.render(label="world")
        assert "got: world" in result

    def test_option_int_none(self) -> None:
        """option(int) with None renders the None case."""
        tmpl = Template.from_source("""---
params:
  - count = option(int)
---
> {% if has(count) %}

count={{ count }}

> {% else %}

no-count

> {% /if %}""")
        result = tmpl.render(count=None)
        assert result.strip() == "no-count"

    def test_option_int_some(self) -> None:
        """option(int) with a value renders the Some case."""
        tmpl = Template.from_source("""---
params:
  - count = option(int)
---
> {% if has(count) %}

count={{ count }}

> {% else %}

no-count

> {% /if %}""")
        result = tmpl.render(count=42)
        assert "count=42" in result

    def test_option_default_none(self) -> None:
        """option(str) with default None works when no value is provided."""
        tmpl = Template.from_source("""---
params:
  - label = option(str) := None
---
> {% if has(label) %}

{{ label }}

> {% else %}

default

> {% /if %}""")
        result = tmpl.render()
        assert result.strip() == "default"


# -- option(T) regression tests -------------------------------------------


class TestOptionTypeRegression:
    """Regression tests for transparent option(T).

    Options are TRANSPARENT:
    - Python None → absent (has() false, renders empty)
    - Python plain value (e.g., "hello") → present (has() true, renders directly)
    - NO .val access needed! The value is used directly.
    """

    def test_option_none_is_python_none(self) -> None:
        """Passing Python None for option(str) → has() false, renders else."""
        tmpl = Template.from_source("""---
params:
  - name = option(str)
---
> {% if has(name) %}

Hello {{ name }}

> {% else %}

No name

> {% /if %}""")
        result = tmpl.render(name=None)
        assert "Hello" not in result
        assert result.strip() == "No name"

    def test_option_some_is_plain_value(self) -> None:
        """Passing a plain string for option(str) → has() true, renders value."""
        tmpl = Template.from_source("""---
params:
  - name = option(str)
---
> {% if has(name) %}

Hello {{ name }}

> {% else %}

No name

> {% /if %}""")
        result = tmpl.render(name="Alice")
        assert "Hello Alice" in result
        assert "No name" not in result

    def test_option_none_in_struct(self) -> None:
        """A struct field typed as option(str) accepts None."""
        tmpl = Template.from_source("""---
params:
  - user = struct(name = str, bio = option(str))
---
Name: {{ user.name }}

> {% if has(user.bio) %}

Bio: {{ user.bio }}

> {% else %}

No bio

> {% /if %}""")
        # None field
        result = tmpl.render(user={"name": "Bob", "bio": None})
        assert "Name: Bob" in result
        assert "No bio" in result
        assert "Bio:" not in result

        # Present field (plain value, not __kind__ dict!)
        result_some = tmpl.render(user={"name": "Bob", "bio": "hacker"})
        assert "Name: Bob" in result_some
        assert "Bio: hacker" in result_some
        assert "No bio" not in result_some

    def test_option_none_in_list(self) -> None:
        """A list(option(str)) can contain None elements."""
        tmpl = Template.from_source("""---
params:
  - items = list(option(str))
---
> {% for item in items %}
> {% if has(item) %}

val={{ item }}

> {% else %}

missing

> {% /if %}
> {% /for %}""")
        result = tmpl.render(items=["hello", None, "world"])
        assert "val=hello" in result
        assert "missing" in result
        assert "val=world" in result

    def test_option_defaults_to_none(self) -> None:
        """option(str) with := None default → omitting the param yields empty."""
        tmpl = Template.from_source("""---
params:
  - title = option(str) := None
---
> {% if has(title) %}

Title: {{ title }}

> {% else %}

untitled

> {% /if %}""")
        # Omit the param entirely — should use the None default
        result = tmpl.render()
        assert result.strip() == "untitled"
        # Explicitly passing a value should override the default
        result_with = tmpl.render(title="My Doc")
        assert "Title: My Doc" in result_with

    def test_option_match_some_none(self) -> None:
        """{% match %} with {% case Some %} and {% case None %} branches."""
        tmpl = Template.from_source("""---
params:
  - tag = option(str)
---
> {% match tag %}
> {% case Some %}

tagged={{ tag }}

> {% case None %}

untagged

> {% /match %}""")
        # None path
        result_none = tmpl.render(tag=None)
        assert result_none.strip() == "untagged"
        # Some path (plain value, no __kind__ needed!)
        result_some = tmpl.render(tag="v1.0")
        assert "tagged=v1.0" in result_some

    def test_option_transparent_no_val(self) -> None:
        """Options are transparent — access the value directly, no .val needed."""
        tmpl = Template.from_source("""---
params:
  - name = option(str)
---
> {% if has(name) %}

{{ name }}

> {% /if %}""")
        result = tmpl.render(name="direct")
        assert result.strip() == "direct"
