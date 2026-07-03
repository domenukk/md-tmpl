"""User-facing helpers for defining custom enum variant types.

Provides three ways to define enum-like types that work with the
md-tmpl converter:

1. ``@variant`` decorator — turn a class with annotations into a matchable
   variant with ``__match_args__`` and the ``_md_tmpl_tag`` protocol.

2. ``Variants`` base class — declare mixed enums (unit + struct variants)
   in a single class body using a metaclass.

3. ``load_types()`` — load generated types from a ``.tmpl.md`` file as a
   namespace object, without the import hook.

Example::

    from md_tmpl import variant, Variants, load_types

    # --- @variant decorator ---
    @variant
    class NeedsChanges:
        reason: str

    v = NeedsChanges(reason="fix tests")
    assert v._md_tmpl_tag == "NeedsChanges"

    match v:
        case NeedsChanges(reason=r):
            print(r)

    # --- Variants base class ---
    class Status(Variants):
        Approved = ()
        Rejected = ()
        NeedsChanges = {"reason": str}

    Status.Approved                          # unit sentinel
    Status.NeedsChanges(reason="fix tests")  # struct variant

    # --- load_types ---
    types = load_types("prompts/review.tmpl.md")
    Status = types.Status
    ReviewParams = types.ReviewParams
"""

from __future__ import annotations

import os

from types import SimpleNamespace
from typing import Any, Callable, Sequence, TypeVar

_T = TypeVar("_T")
import sys

if sys.version_info >= (3, 11):
    from typing import dataclass_transform
else:

    def dataclass_transform(*args: Any, **kwargs: Any) -> Callable[[Any], Any]:  # type: ignore[misc]
        return lambda x: x


# ---------------------------------------------------------------------------
# @variant decorator
# ---------------------------------------------------------------------------


@dataclass_transform()
def variant(cls: Any) -> Any:
    """Transform a class with type annotations into a matchable variant.

    The class must have annotations (``field: type``) that define the
    variant's fields. The decorator adds:

    - ``__match_args__`` for positional pattern matching
    - ``__slots__`` for memory efficiency
    - ``_md_tmpl_tag`` (class attribute, defaults to class name)
    - ``_md_tmpl_fields`` property (dict of field values)
    - ``__init__``, ``__repr__``, ``__eq__``, ``__hash__``

    Args:
        cls: The class to transform.

    Returns:
        The transformed class.

    Example::

        @variant
        class NeedsChanges:
            reason: str

        v = NeedsChanges(reason="fix tests")
        assert v._md_tmpl_tag == "NeedsChanges"
        assert v.reason == "fix tests"

        match v:
            case NeedsChanges(reason=r):
                print(r)
    """
    annotations = getattr(cls, "__annotations__", {})
    if not annotations:
        raise TypeError(
            f"@variant requires at least one annotated field, "
            f"but {cls.__name__!r} has none"
        )

    field_names = list(annotations.keys())
    name = cls.__name__
    tag = getattr(cls, "_md_tmpl_tag", name)

    # Build the new class dynamically to get clean __slots__.
    namespace: dict[str, Any] = {}
    namespace["__match_args__"] = tuple(field_names)
    namespace["__slots__"] = tuple(field_names)
    namespace["_md_tmpl_tag"] = tag
    namespace["__annotations__"] = annotations.copy()

    # __init__
    def make_init(fields: list[str]) -> Callable[..., None]:
        def __init__(self: Any, **kwargs: Any) -> None:
            for f in fields:
                if f not in kwargs:
                    raise TypeError(
                        f"{name}() missing required keyword argument: {f!r}"
                    )
                object.__setattr__(self, f, kwargs[f])
            unexpected = set(kwargs) - set(fields)
            if unexpected:
                raise TypeError(
                    f"{name}() got unexpected keyword arguments: "
                    f"{', '.join(sorted(unexpected))}"
                )

        return __init__

    namespace["__init__"] = make_init(field_names)

    # _md_tmpl_fields property
    def make_fields_prop(fields: list[str]) -> Any:
        @property  # type: ignore[misc]
        def _md_tmpl_fields(self: Any) -> dict[str, Any]:
            return {f: getattr(self, f) for f in fields}

        return _md_tmpl_fields

    namespace["_md_tmpl_fields"] = make_fields_prop(field_names)

    # __repr__
    def make_repr(n: str, fields: list[str]) -> Callable[..., str]:
        def __repr__(self: Any) -> str:
            parts = ", ".join(f"{f}={getattr(self, f)!r}" for f in fields)
            return f"{n}({parts})"

        return __repr__

    namespace["__repr__"] = make_repr(name, field_names)

    # __eq__
    def make_eq(fields: list[str]) -> Callable[..., object]:
        def __eq__(self: Any, other: object) -> object:
            if not isinstance(other, type(self)):
                return NotImplemented
            return all(getattr(self, f) == getattr(other, f) for f in fields)

        return __eq__

    namespace["__eq__"] = make_eq(field_names)

    # __hash__
    def make_hash(tag_val: str, fields: list[str]) -> Callable[..., int]:
        def __hash__(self: Any) -> int:
            return hash((tag_val, *(getattr(self, f) for f in fields)))

        return __hash__

    namespace["__hash__"] = make_hash(tag, field_names)

    new_cls = type(name, (), namespace)
    new_cls.__module__ = cls.__module__
    new_cls.__qualname__ = cls.__qualname__
    return new_cls


# ---------------------------------------------------------------------------
# Variants metaclass
# ---------------------------------------------------------------------------


class _UnitSentinel:
    """A unit variant sentinel for use in Variants subclasses.

    Compared by tag name. Carries ``_md_tmpl_tag`` and empty
    ``_md_tmpl_fields`` for converter compatibility.
    """

    __slots__ = ("_md_tmpl_tag", "_md_tmpl_fields")

    def __init__(self, tag: str) -> None:
        self._md_tmpl_tag = tag
        self._md_tmpl_fields: dict[str, Any] = {}

    def __repr__(self) -> str:
        return self._md_tmpl_tag

    def __eq__(self, other: object) -> bool:
        if isinstance(other, str):
            return self._md_tmpl_tag == other
        if isinstance(other, _UnitSentinel):
            return self._md_tmpl_tag == other._md_tmpl_tag
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self._md_tmpl_tag)


class _VariantsMeta(type):
    """Metaclass for ``Variants`` that processes the class body.

    Replaces:
    - ``Name = ()`` with a unit sentinel
    - ``Name = {"field": type, ...}`` with a generated struct variant class
    """

    def __new__(
        mcs,
        name: str,
        bases: tuple[type, ...],
        namespace: dict[str, Any],
    ) -> type:
        # Skip processing for the Variants base class itself.
        if not bases or all(b is object for b in bases):
            return super().__new__(mcs, name, bases, namespace)

        processed: dict[str, Any] = {}
        for key, value in namespace.items():
            if key.startswith("_") or callable(value):
                processed[key] = value
                continue

            if isinstance(value, tuple) and len(value) == 0:
                # Unit variant: empty tuple → sentinel.
                processed[key] = _UnitSentinel(key)
            elif isinstance(value, dict):
                # Struct variant: dict of {field_name: type} → class.
                processed[key] = _build_variant_from_dict(key, value)
            else:
                processed[key] = value

        return super().__new__(mcs, name, bases, processed)


def _build_variant_from_dict(name: str, fields: dict[str, type]) -> type:
    """Build a struct variant class from a ``{field: type}`` dict."""
    field_names = list(fields.keys())

    # Validate field names are valid Python identifiers.
    for fname in field_names:
        if not fname.isidentifier():
            raise ValueError(
                f"invalid field name {fname!r} in variant {name!r}: "
                f"must be a valid Python identifier"
            )

    namespace: dict[str, Any] = {}
    namespace["__match_args__"] = tuple(field_names)
    namespace["__slots__"] = tuple(field_names)
    namespace["_md_tmpl_tag"] = name
    namespace["__annotations__"] = fields.copy()

    # __init__ with positional + keyword args (closure-based, no exec).
    def make_init(variant_name: str, fnames: list[str]) -> Callable[..., None]:
        def __init__(self: Any, *args: Any, **kwargs: Any) -> None:
            if len(args) > len(fnames):
                raise TypeError(
                    f"{variant_name}() takes {len(fnames)} positional "
                    f"argument(s) but {len(args)} were given"
                )
            # Merge positional args into kwargs.
            for field, value in zip(fnames, args):
                if field in kwargs:
                    raise TypeError(
                        f"{variant_name}() got multiple values for "
                        f"argument {field!r}"
                    )
                kwargs[field] = value
            # Validate all required fields are present.
            missing = [f for f in fnames if f not in kwargs]
            if missing:
                raise TypeError(
                    f"{variant_name}() missing required argument(s): "
                    f"{', '.join(repr(m) for m in missing)}"
                )
            # Reject unexpected fields.
            unexpected = set(kwargs) - set(fnames)
            if unexpected:
                raise TypeError(
                    f"{variant_name}() got unexpected keyword argument(s): "
                    f"{', '.join(sorted(unexpected))}"
                )
            for field in fnames:
                object.__setattr__(self, field, kwargs[field])

        return __init__

    namespace["__init__"] = make_init(name, field_names)

    # _md_tmpl_fields property
    def make_fields_prop(fnames: list[str]) -> Any:
        @property  # type: ignore[misc]
        def _md_tmpl_fields(self: Any) -> dict[str, Any]:
            return {f: getattr(self, f) for f in fnames}

        return _md_tmpl_fields

    namespace["_md_tmpl_fields"] = make_fields_prop(field_names)

    # __repr__
    def make_repr(n: str, fnames: list[str]) -> Callable[..., str]:
        def __repr__(self: Any) -> str:
            parts = ", ".join(f"{f}={getattr(self, f)!r}" for f in fnames)
            return f"{n}({parts})"

        return __repr__

    namespace["__repr__"] = make_repr(name, field_names)

    # __eq__
    def make_eq(fnames: list[str]) -> Callable[..., object]:
        def __eq__(self: Any, other: object) -> object:
            if not isinstance(other, type(self)):
                return NotImplemented
            return all(getattr(self, f) == getattr(other, f) for f in fnames)

        return __eq__

    namespace["__eq__"] = make_eq(field_names)

    # __hash__
    def make_hash(tag: str, fnames: list[str]) -> Callable[..., int]:
        def __hash__(self: Any) -> int:
            return hash((tag, *(getattr(self, f) for f in fnames)))

        return __hash__

    namespace["__hash__"] = make_hash(name, field_names)

    return type(name, (), namespace)


@dataclass_transform()
class Variants(metaclass=_VariantsMeta):
    """Base class for declaring mixed enum types.

    Subclass this to define enums with unit and struct variants::

        class Status(Variants):
            Approved = ()                  # unit variant
            Rejected = ()                  # unit variant
            NeedsChanges = {"reason": str} # struct variant

    Unit variants are sentinels (no parens at use site)::

        Status.Approved
        Status.Rejected

    Struct variants are callable classes with ``__match_args__``::

        v = Status.NeedsChanges(reason="fix tests")
        assert v.reason == "fix tests"

    All variants work with Python 3.10+ ``match``/``case``::

        match outcome:
            case Status.Approved:
                ...
            case Status.NeedsChanges(reason=reason):
                print(reason)

    All variants carry ``_md_tmpl_tag`` and
    ``_md_tmpl_fields`` for converter compatibility.
    """

    def __class_getitem__(cls, item: Any) -> type:
        """Allow ``Variants[...]`` syntax for type annotations."""
        return cls


# ---------------------------------------------------------------------------
# load_types
# ---------------------------------------------------------------------------


def load_types(
    path: str | os.PathLike[str],
    pick: Sequence[str] | None = None,
) -> SimpleNamespace:
    """Load generated types from a ``.tmpl.md`` template file.

    Returns a ``SimpleNamespace`` with all generated types as attributes.
    This is the explicit, no-magic alternative to the import hook.

    Args:
        path: Path to a ``.tmpl.md`` template file.
        pick: Optional list of type names to extract. If provided, only
            those types are included. Raises ``KeyError`` if a name
            is not found.

    Returns:
        A namespace with generated types as attributes.

    Example::

        from md_tmpl import load_types

        # Get all types:
        types = load_types("prompts/review.tmpl.md")
        Status = types.Status
        ReviewParams = types.ReviewParams

        # Or pick specific ones:
        types = load_types("prompts/review.tmpl.md", pick=["Status"])

    Raises:
        ValueError: If the template file cannot be read or parsed.
        KeyError: If a name in ``pick`` is not found in the generated types.
    """
    from md_tmpl._md_tmpl import generate_types_for_template

    path = os.fspath(path)
    all_types: dict[str, Any] = generate_types_for_template(path)

    if pick is not None:
        selected: dict[str, Any] = {}
        for name in pick:
            if name not in all_types:
                available = ", ".join(sorted(all_types.keys()))
                raise KeyError(
                    f"type {name!r} not found in template. " f"Available: {available}"
                )
            selected[name] = all_types[name]
        return SimpleNamespace(**selected)

    return SimpleNamespace(**all_types)
