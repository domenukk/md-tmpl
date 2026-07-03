"""Type stubs for the md_tmpl package.

Provides type information for the native Rust extension module and
pure-Python helpers. These stubs enable IDE autocompletion and type
checking via mypy/pyright when the ``py.typed`` marker is present.
"""

import os
import sys
import types as _types
from typing import (
    Any,
    Protocol,
    Sequence,
    TypeVar,
    runtime_checkable,
)

if sys.version_info >= (3, 11):
    from typing import dataclass_transform
else:
    from typing_extensions import dataclass_transform

_T = TypeVar("_T")

# -- Variant protocol -------------------------------------------------------

@runtime_checkable
class VariantProtocol(Protocol):
    """Protocol implemented by all variant types.

    Both ``@variant``-decorated classes and ``Variants`` subclass
    members carry these attributes for converter compatibility.
    """

    _md_tmpl_tag: str
    _md_tmpl_fields: dict[str, Any]
    __match_args__: tuple[str, ...]

# -- Exception hierarchy -------------------------------------------------

from md_tmpl._exceptions import (
    ExtraParamsError as ExtraParamsError,
    MissingParamsError as MissingParamsError,
    TemplateError as TemplateError,
    TemplatePanicError as TemplatePanicError,
    TemplateSyntaxError as TemplateSyntaxError,
    TypeMismatchError as TypeMismatchError,
)

# -- Core classes --------------------------------------------------------

class Template:
    """A parsed, validated template ready for rendering."""

    @staticmethod
    def from_file(path: str | os.PathLike[str]) -> "Template": ...
    @staticmethod
    def from_source(source: str) -> "Template": ...
    @staticmethod
    def from_source_allowing_unused(source: str) -> "Template": ...
    @staticmethod
    def from_source_with_base_dir(
        source: str, base_dir: str | os.PathLike[str]
    ) -> "Template": ...
    def render(self, *, allow_extra: bool = False, **kwargs: Any) -> str: ...
    def render_dict(
        self, params: dict[str, Any], *, allow_extra: bool = False
    ) -> str: ...
    def render_cached(
        self, cache: "TemplateCache", *, allow_extra: bool = False, **kwargs: Any
    ) -> str: ...
    def render_cached_dict(
        self,
        params: dict[str, Any],
        cache: "TemplateCache",
        *,
        allow_extra: bool = False,
    ) -> str: ...
    def render_flexbuffers(
        self, buffer: bytes, *, allow_extra: bool = False
    ) -> str: ...
    def render_cached_flexbuffers(
        self,
        buffer: bytes,
        cache: "TemplateCache",
        *,
        allow_extra: bool = False,
    ) -> str: ...
    def render_json(self, json_str: str, *, allow_extra: bool = False) -> str: ...
    def render_json_cached(
        self,
        json_str: str,
        cache: "TemplateCache",
        *,
        allow_extra: bool = False,
    ) -> str: ...
    def render_empty(self) -> str: ...
    def declarations(self) -> list[tuple[str, str]]: ...
    def source_hash(self) -> int: ...
    def defaults(self) -> dict[str, Any]: ...
    def consts(self) -> dict[str, Any]: ...
    def imported_consts(self) -> dict[str, Any]: ...
    def validate_declarations_against(
        self, expected: list[tuple[str, str]]
    ) -> None: ...
    def body(self) -> str: ...
    def set_max_include_depth(self, depth: int) -> None: ...
    def __enter__(self) -> "Template": ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> bool: ...
    def __repr__(self) -> str: ...

class TemplateCache:
    """Content-hashed template cache for hot-reload scenarios."""

    def __init__(self, *, max_entries: int | None = None) -> None: ...
    def load(self, path: str | os.PathLike[str]) -> Template: ...
    def clear(self) -> None: ...
    def template_count(self) -> int: ...
    def include_count(self) -> int: ...
    def __len__(self) -> int: ...
    def __enter__(self) -> "TemplateCache": ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> bool: ...

# -- Template with generated types ---------------------------------------

class TemplateWithTypes:
    """A template object with generated types as attributes.

    Returned by :func:`template`. Generated enum, model, and
    params classes are available as attributes.
    """

    def render(self, **kwargs: Any) -> str: ...
    def render_dict(self, params: dict[str, Any]) -> str: ...
    def declarations(self) -> list[tuple[str, str]]: ...
    def __repr__(self) -> str: ...
    def __getattr__(self, name: str) -> Any: ...

# -- Helper functions ----------------------------------------------------

def template(path: str | os.PathLike[str]) -> TemplateWithTypes: ...
def load_template(path: str | os.PathLike[str]) -> Template: ...
def load_types(
    path: str | os.PathLike[str],
    *,
    pick: Sequence[str] | None = None,
) -> _types.SimpleNamespace: ...
def md_tmpl_import_hook(
    *, search_paths: Sequence[str | os.PathLike[str]] | None = None
) -> None: ...
def generate_types_source(path: str | os.PathLike[str]) -> str:
    """Generate Python source code with typed classes for a template.

    Write the output to a ``.py`` file for static type checking support
    with mypy/pyright.

    Args:
        path: Path to a ``.tmpl.md`` template file.

    Returns:
        Python source code string with ``@dataclass`` classes and
        ``Variants`` subclasses.
    """
    ...

@dataclass_transform()
def variant(cls: Any) -> Any:
    """Transform a class with annotations into a matchable variant.

    The returned class has:

    - ``__match_args__`` for positional pattern matching
    - ``__slots__`` for memory efficiency
    - ``_md_tmpl_tag: str`` class attribute
    - ``_md_tmpl_fields: dict[str, Any]`` property
    - ``__init__``, ``__repr__``, ``__eq__``, ``__hash__``
    """
    ...

@dataclass_transform()
class Variants:
    """Base class for defining variant enums.

    Unit variants: ``Approved = ()``
    Struct variants: ``NeedsChanges = {"reason": str}``
    """

    def __class_getitem__(cls, item: Any) -> type: ...
