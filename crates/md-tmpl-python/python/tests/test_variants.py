"""Tests for md_tmpl @variant decorator, Variants metaclass, and pattern matching."""

import textwrap
from pathlib import Path
from typing import Any

import pytest

from md_tmpl import (
    Template,
    Variants,
    load_types,
    template,
    variant,
)

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
        # @variant injects _md_tmpl_tag at runtime; dataclass_transform hides it.
        assert v._md_tmpl_tag == "NeedsChanges"  # type: ignore[attr-defined]
        assert v.reason == "fix tests"

    def test_variant_fields_property(self) -> None:
        @variant
        class Error:
            code: int
            message: str

        v = Error(code=404, message="not found")
        # @variant injects _md_tmpl_fields at runtime; dataclass_transform hides it.
        fields = v._md_tmpl_fields  # type: ignore[attr-defined]
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

        # Metaclass rewrites `()` class-attrs into unit-variant instances at runtime.
        assert Status.Active._md_tmpl_tag == "Active"  # type: ignore[attr-defined]
        assert Status.Active._md_tmpl_fields == {}  # type: ignore[attr-defined]

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

        # Metaclass rewrites dict class-attrs into variant constructors at runtime.
        ok = Result.Ok(value="done")  # type: ignore[operator]
        assert ok._md_tmpl_tag == "Ok"
        assert ok.value == "done"
        assert ok._md_tmpl_fields == {"value": "done"}

        err = Result.Err(code=500, message="fail")  # type: ignore[operator]
        assert err._md_tmpl_tag == "Err"
        assert err.code == 500
        assert err.message == "fail"

    def test_mixed_variants(self) -> None:
        class Status(Variants):
            Approved = ()
            Rejected = ()
            NeedsChanges = {"reason": str}

        assert repr(Status.Approved) == "Approved"
        # Metaclass rewrites the dict class-attr into a variant constructor.
        nc = Status.NeedsChanges(reason="fix tests")  # type: ignore[operator]
        assert nc.reason == "fix tests"

    def test_struct_variant_repr(self) -> None:
        class Wrap(Variants):
            Inner = {"x": int}

        # Metaclass rewrites the dict class-attr into a variant constructor.
        v = Wrap.Inner(x=42)  # type: ignore[operator]
        assert "Inner" in repr(v)
        assert "42" in repr(v)

    def test_struct_variant_equality(self) -> None:
        class Op(Variants):
            Add = {"n": int}

        # Metaclass rewrites dict class-attrs into variant constructors.
        assert Op.Add(n=1) == Op.Add(n=1)  # type: ignore[operator]
        assert Op.Add(n=1) != Op.Add(n=2)  # type: ignore[operator]


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
        assert v._md_tmpl_fields == {"evidence": "proof"}


# ---------------------------------------------------------------------------
# Variants metaclass — extended edge cases
# ---------------------------------------------------------------------------


class TestVariantsMetaclassExtended:
    """Extended tests for Variants metaclass edge cases."""

    def test_struct_variant_match_args(self) -> None:
        class Op(Variants):
            Add = {"n": int}

        assert hasattr(Op.Add, "__match_args__")
        assert "n" in Op.Add.__match_args__

    def test_struct_variant_hash(self) -> None:
        class Op(Variants):
            Add = {"n": int}

        # Metaclass rewrites dict class-attrs into variant constructors.
        a = Op.Add(n=1)  # type: ignore[operator]
        b = Op.Add(n=1)  # type: ignore[operator]
        assert hash(a) == hash(b)
        assert len({a, b}) == 1

    def test_struct_variant_fields_property(self) -> None:
        class Op(Variants):
            Add = {"n": int, "m": int}

        # Metaclass rewrites the dict class-attr into a variant constructor.
        v = Op.Add(n=1, m=2)  # type: ignore[operator]
        assert v._md_tmpl_fields == {"n": 1, "m": 2}

    def test_invalid_field_name_raises(self) -> None:
        """Field names must be valid Python identifiers."""
        with pytest.raises(ValueError, match="invalid field name"):

            class Bad(Variants):
                X = {"not-valid": str}

    def test_unit_variant_as_template_param(self) -> None:
        """Unit variants from Variants should render through the template engine."""

        class Status(Variants):
            Active = ()
            Inactive = ()

        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - status = enum(Active, Inactive)
            ---
            > {% match status %}
            > {% case Active %}

            on

            > {% case Inactive %}

            off

            > {% /match %}"""))
        result = tmpl.render(status=Status.Active)
        assert result == "on\n"

    def test_struct_variant_as_template_param(self) -> None:
        """Struct variants from Variants should render through the template engine."""

        class Result(Variants):
            Success = {"value": str}
            Failure = {"code": int}

        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - outcome = enum(Success(value = str), Failure(code = int))
            ---
            > {% match outcome %}
            > {% case Success %}

            OK: {{ outcome.value }}

            > {% case Failure %}

            ERR: {{ outcome.code }}

            > {% /match %}"""))
        # Metaclass rewrites the dict class-attr into a variant constructor.
        result = tmpl.render(outcome=Result.Success(value="done"))  # type: ignore[operator]
        assert result == "OK: done\n"


# ---------------------------------------------------------------------------
# @variant decorator — extended
# ---------------------------------------------------------------------------


class TestVariantDecoratorExtended:
    """Extended tests for @variant decorator integration."""

    def test_variant_as_template_param(self) -> None:
        """A @variant instance should work as a struct variant in rendering."""

        @variant
        class NeedsChanges:
            reason: str

        tmpl = Template.from_source(textwrap.dedent("""\
            ---
            params:
              - outcome = enum(Approved, NeedsChanges(reason = str))
            ---
            > {% match outcome %}
            > {% case Approved %}

            YES

            > {% case NeedsChanges %}

            CHANGES: {{ outcome.reason }}

            > {% /match %}"""))
        result = tmpl.render(outcome=NeedsChanges(reason="add tests"))
        assert result == "CHANGES: add tests\n"

    def test_variant_custom_tag(self) -> None:
        """A @variant class with a custom _md_tmpl_tag should use it."""

        @variant
        class MyCustomTag:
            _md_tmpl_tag = "Approved"
            note: str

        v = MyCustomTag(note="lgtm")
        assert v._md_tmpl_tag == "Approved"


# ---------------------------------------------------------------------------
# Regression: exec() removal in Variants metaclass
# ---------------------------------------------------------------------------


class TestVariantsExecRemoval:
    """Regression tests for the closure-based __init__ that replaced exec().

    The _build_variant_from_dict() function in _variants.py originally used
    exec() to build __init__ for struct variants. This was replaced with a
    closure-based approach. These tests ensure the new implementation handles
    all argument passing styles correctly.
    """

    # -- helpers --

    @staticmethod
    def _make_result() -> Any:
        """Create a fresh Result Variants class for each test."""

        class Result(Variants):
            Ok = {"value": str}
            Err = {"code": int, "message": str}

        return Result

    # (a) Positional args

    def test_struct_variant_positional_args(self) -> None:
        Result = self._make_result()
        err = Result.Err(500, "fail")
        assert err.code == 500
        assert err.message == "fail"

    # (b) Keyword args

    def test_struct_variant_keyword_args(self) -> None:
        Result = self._make_result()
        err = Result.Err(code=500, message="fail")
        assert err.code == 500
        assert err.message == "fail"

    # (c) Mixed positional and keyword args

    def test_struct_variant_mixed_args(self) -> None:
        Result = self._make_result()
        err = Result.Err(500, message="fail")
        assert err.code == 500
        assert err.message == "fail"

    # (d) Duplicate arg raises TypeError

    def test_struct_variant_duplicate_arg_raises(self) -> None:
        Result = self._make_result()
        with pytest.raises(TypeError):
            Result.Err(500, code=999)  # code given positionally and as kwarg

    # (e) Missing required arg raises TypeError

    def test_struct_variant_missing_arg_raises(self) -> None:
        Result = self._make_result()
        with pytest.raises(TypeError, match="missing"):
            Result.Err(500)  # missing 'message'

    # (f) Unknown keyword arg raises TypeError

    def test_struct_variant_unknown_kwarg_raises(self) -> None:
        Result = self._make_result()
        with pytest.raises(TypeError, match="unexpected"):
            Result.Err(code=500, message="fail", extra="bad")

    # (g) Equality, repr, hash on struct variants

    def test_struct_variant_equality(self) -> None:
        Result = self._make_result()
        assert Result.Err(code=1, message="a") == Result.Err(code=1, message="a")
        assert Result.Err(code=1, message="a") != Result.Err(code=2, message="a")

    def test_struct_variant_repr(self) -> None:
        Result = self._make_result()
        assert "Err" in repr(Result.Err(code=1, message="a"))

    def test_struct_variant_hash(self) -> None:
        Result = self._make_result()
        a = Result.Err(code=1, message="a")
        b = Result.Err(code=1, message="a")
        assert hash(a) == hash(b)
        assert {a, b} == {a}  # deduplication via hash + eq

    # (h) Unit variants mixed with struct variants

    def test_mixed_unit_and_struct_variants(self) -> None:
        class Status(Variants):
            Approved = ()
            NeedsWork = {"reason": str}

        assert repr(Status.Approved) == "Approved"
        # Metaclass rewrites the dict class-attr into a variant constructor.
        assert Status.NeedsWork(reason="fix").reason == "fix"  # type: ignore[operator]

    # (i) Template render with Variants struct variants

    def test_template_render_with_struct_variant(self) -> None:
        tmpl = Template.from_source("""---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% /match %}""")

        class Outcome(Variants):
            Confirmed = {"evidence": str}
            Rejected = ()

        # Metaclass rewrites the dict class-attr into a variant constructor.
        result = tmpl.render(outcome=Outcome.Confirmed(evidence="proof"))  # type: ignore[operator]
        assert "YES: proof" in result


# ---------------------------------------------------------------------------
# Pattern matching (Python 3.10+)
# ---------------------------------------------------------------------------


class TestPatternMatching:
    """Tests for Python 3.10+ match/case support on variant types."""

    def test_unit_variant_match(self) -> None:
        class Status(Variants):
            Approved = ()
            Rejected = ()

        def classify(s: Any) -> str:
            match s:
                case Status.Approved:
                    return "approved"
                case Status.Rejected:
                    return "rejected"
                case _:
                    return "unknown"

        assert classify(Status.Approved) == "approved"
        assert classify(Status.Rejected) == "rejected"

    def test_struct_variant_match(self) -> None:
        class Status(Variants):
            Approved = ()
            NeedsChanges = {"reason": str}

        def explain(s: Any) -> str:
            match s:
                # Metaclass rewrites the dict class-attr into a matchable variant class.
                case Status.NeedsChanges(reason=r):  # type: ignore[misc]
                    return f"needs changes: {r}"
                case Status.Approved:
                    return "approved!"
                case _:
                    return "unknown"

        # Metaclass rewrites the dict class-attr into a variant constructor.
        assert (
            explain(Status.NeedsChanges(reason="fix tests"))  # type: ignore[operator]
            == "needs changes: fix tests"
        )
        assert explain(Status.Approved) == "approved!"

    def test_variant_decorator_match(self) -> None:
        @variant
        class Confirmed:
            evidence: str
            confidence: float

        def handle(v: Any) -> str:
            match v:
                case Confirmed(evidence=e, confidence=c) if c > 0.9:
                    return f"high confidence: {e}"
                case Confirmed(evidence=e):
                    return f"low confidence: {e}"
                case _:
                    return "unknown"

        assert (
            handle(Confirmed(evidence="found it", confidence=0.95))
            == "high confidence: found it"
        )
        assert (
            handle(Confirmed(evidence="maybe", confidence=0.5))
            == "low confidence: maybe"
        )

    def test_load_types_match(self) -> None:
        """Pattern matching with types generated from a template file."""
        import tempfile
        import os

        with tempfile.NamedTemporaryFile(
            suffix=".tmpl.md", mode="w", delete=False
        ) as f:
            f.write("""---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)

allow_unused: true
---
{{ kind(outcome) }}""")
            path = f.name

        try:
            types = load_types(path)
            Outcome = types.Outcome

            v = Outcome.Confirmed(evidence="found it")
            match v:
                case Outcome.Confirmed(evidence=e):
                    result = f"confirmed: {e}"
                case _:
                    result = "other"

            assert result == "confirmed: found it"
        finally:
            os.unlink(path)
