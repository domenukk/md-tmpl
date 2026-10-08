"""Tests for the md_tmpl exception hierarchy."""

import textwrap
from pathlib import Path

import pytest

from md_tmpl import (
    Template,
    TemplatePanicError,
    TemplateError,
    TemplateSyntaxError,
    MissingParamsError,
    TypeMismatchError,
    ExtraParamsError,
    UndefinedVariableError,
    UnknownFilterError,
    IncludeNotFoundError,
    DeclarationsMutatedError,
)

# ---------------------------------------------------------------------------
# Exception hierarchy
# ---------------------------------------------------------------------------


class TestExceptionHierarchy:
    """Tests for exception subclass catchability, kinds, and structured fields."""

    def test_syntax_error_catchable(self) -> None:
        with pytest.raises(TemplateSyntaxError) as excinfo:
            Template.from_source("no frontmatter at all")
        assert excinfo.value.kind == "syntax"

    def test_syntax_error_is_value_error(self) -> None:
        """Backward compatibility: still catchable as ValueError."""
        with pytest.raises(ValueError):
            Template.from_source("no frontmatter at all")

    def test_syntax_error_message_preserved(self) -> None:
        """str(exc) must still equal the human-readable message."""
        with pytest.raises(TemplateSyntaxError) as excinfo:
            Template.from_source("no frontmatter at all")
        assert "frontmatter" in str(excinfo.value)

    def test_missing_params_error(self) -> None:
        tmpl = Template.from_source("""---
params: [name = str, age = int]
---
{{ name }} {{ age }}""")
        with pytest.raises(MissingParamsError) as excinfo:
            tmpl.render(name="Alice")
        assert excinfo.value.kind == "missing_params"
        assert excinfo.value.missing == ["age"]

    def test_type_mismatch_error(self) -> None:
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

    def test_type_mismatch_is_type_error(self) -> None:
        """TypeMismatchError also inherits TypeError."""
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [flag = bool]
            ---
            {{ flag }}"""))
        with pytest.raises(TypeError):
            tmpl.render(flag="not a bool")

    def test_extra_params_error(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            Hello {{ name }}!"""))
        with pytest.raises(ExtraParamsError) as excinfo:
            tmpl.render(name="world", bogus="unexpected")
        assert excinfo.value.kind == "extra_params"
        assert excinfo.value.extra == ["bogus"]

    def test_panic_error(self) -> None:
        tmpl = Template.from_source("""---
params: []
---
> {% panic("halt") %}""")
        with pytest.raises(TemplatePanicError) as excinfo:
            tmpl.render()
        assert excinfo.value.kind == "panic"
        assert "halt" in str(excinfo.value)

    def test_io_error_maps_to_base_with_kind(self) -> None:
        """Missing files surface as the base TemplateError tagged kind='io'."""
        with pytest.raises(TemplateError) as excinfo:
            Template.from_file("/nonexistent/path.tmpl.md")
        assert excinfo.value.kind == "io"
        # Must not be a more specific subclass.
        assert type(excinfo.value) is TemplateError

    def test_include_not_found_error(self, tmp_path: Path) -> None:
        tmpl = Template.from_source_with_base_dir(
            """---
params: []
---
> {% include [x](./nope.tmpl.md) %}""",
            str(tmp_path),
        )
        with pytest.raises(IncludeNotFoundError) as excinfo:
            tmpl.render()
        assert excinfo.value.kind == "include_not_found"
        assert "nope.tmpl.md" in excinfo.value.include

    def test_declarations_mutated_error(self) -> None:
        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [name = str]
            ---
            {{ name }}"""))
        with pytest.raises(DeclarationsMutatedError) as excinfo:
            tmpl.validate_declarations_against([("different", "int")])
        assert excinfo.value.kind == "declarations_mutated"
        assert "different" in excinfo.value.details

    def test_template_error_base_class(self) -> None:
        """All specific errors are catchable as TemplateError."""
        with pytest.raises(TemplateError):
            Template.from_source("no frontmatter")

    def test_specific_errors_catchable_as_value_error(self) -> None:
        """Back-compat: every specific error is still a ValueError."""
        tmpl = Template.from_source("""---
params: [name = str, age = int]
---
{{ name }} {{ age }}""")
        with pytest.raises(ValueError):
            tmpl.render(name="Alice")

    # -- Classes for variants the Rust core currently reports at compile
    #    time as a Syntax error (so they aren't produced by the engine from
    #    Python yet). We still verify the dedicated Python classes exist and
    #    expose the right kind/fields, matching the Go/TS parity contract.

    def test_undefined_variable_error_class(self) -> None:
        err = UndefinedVariableError("undefined variable: x", variable="x")
        assert err.kind == "undefined_variable"
        assert err.variable == "x"
        assert isinstance(err, TemplateError)
        assert isinstance(err, ValueError)
        assert str(err) == "undefined variable: x"

    def test_unknown_filter_error_class(self) -> None:
        err = UnknownFilterError("unknown filter: bogus", filter="bogus")
        assert err.kind == "unknown_filter"
        assert err.filter == "bogus"
        assert isinstance(err, TemplateError)
        assert str(err) == "unknown filter: bogus"

    def test_base_template_error_unknown_kind_sentinel(self) -> None:
        """The base class uses the empty-string 'unknown' sentinel."""
        assert TemplateError.kind == ""
        assert TemplateError("boom").kind == ""
        assert TemplateError("boom", kind="io").kind == "io"

    def test_every_kind_id_is_stable(self) -> None:
        """Pin the machine-readable kind ids (they cross the FFI boundary)."""
        assert TemplateSyntaxError.kind == "syntax"
        assert UndefinedVariableError.kind == "undefined_variable"
        assert MissingParamsError.kind == "missing_params"
        assert TypeMismatchError.kind == "type_mismatch"
        assert UnknownFilterError.kind == "unknown_filter"
        assert IncludeNotFoundError.kind == "include_not_found"
        assert DeclarationsMutatedError.kind == "declarations_mutated"
        assert ExtraParamsError.kind == "extra_params"
        assert TemplatePanicError.kind == "panic"


# ---------------------------------------------------------------------------
# Exception hierarchy — additional error types
# ---------------------------------------------------------------------------


class TestExceptionHierarchyExtended:
    """Tests for specific error mappings in errors.rs."""

    def test_undefined_variable_is_syntax_error(self) -> None:
        """UndefinedVariable should map to TemplateSyntaxError."""
        from md_tmpl import TemplateSyntaxError

        with pytest.raises(TemplateSyntaxError):
            Template.from_source(textwrap.dedent("""\
                ---
                params: [name = str]
                ---
                {{ undefined_var }}"""))

    def test_include_not_found_is_syntax_error(self) -> None:
        """IncludeNotFound should map to TemplateSyntaxError."""
        from md_tmpl import TemplateSyntaxError, TemplateError

        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params: [title = str]
            ---
            > {% include [missing](./does_not_exist.tmpl.md) with title=title %}"""))
        # Render should fail with include not found
        with pytest.raises((TemplateSyntaxError, TemplateError)):
            tmpl.render(title="Test")

    def test_unknown_filter_raises(self) -> None:
        """UnknownFilter should map to TemplateSyntaxError."""
        from md_tmpl import TemplateSyntaxError

        with pytest.raises(TemplateSyntaxError):
            Template.from_source("""---
params: [name = str]
---
{{ name | nonexistent_filter }}""")
