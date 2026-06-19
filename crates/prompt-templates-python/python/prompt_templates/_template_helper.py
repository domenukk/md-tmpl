"""Convenience ``template()`` helper for loading templates with generated types.

This is the simpler alternative to the import hook — no ``sys.meta_path``
magic, just a function call that returns an object with generated types
as attributes::

    from prompt_templates import template

    review = template("prompts/code_review.tmpl.md")

    # Types are attributes on the template object:
    output = review.render(
        reviewer="Alice",
        items=[
            review.Item(file="main.rs", status=review.Status.Approved),
        ],
    )
"""

from __future__ import annotations

import os
from typing import Any


class _TemplateWithTypes:
    """A template object with generated types as attributes.

    Created by the ``template()`` helper. Combines the native
    ``Template`` with generated Python types for a seamless API.

    Attributes:
        All generated types (params class, enum classes, model classes)
        are available as attributes on this object.

    Example::

        review = template("prompts/code_review.tmpl.md")
        review.Status.Approved        # enum variant
        review.Status.NeedsChanges(reason="...")  # enum constructor
        review.Item(file="...", status=...)  # model constructor
        review.render(reviewer="Alice", items=[...])  # render
    """

    def __init__(self, template_path: str | os.PathLike[str]) -> None:
        from prompt_templates._prompt_templates import (
            Template as _NativeTemplate,
            generate_types_for_template,
        )

        self._path = os.fspath(template_path)
        self._native = _NativeTemplate.from_file(self._path)
        self._types: dict[str, Any] = generate_types_for_template(self._path)

        # Attach generated types as attributes.
        for name, cls in self._types.items():
            setattr(self, name, cls)

    def render(self, **kwargs: Any) -> str:
        """Render the template with keyword arguments.

        All arguments are validated against frontmatter type declarations.

        Args:
            **kwargs: Template parameters.

        Returns:
            str: The rendered output.

        Raises:
            ValueError: If validation or rendering fails.
            TypeError: If a value cannot be converted.
        """
        return self._native.render(**kwargs)

    def render_dict(self, params: dict[str, Any]) -> str:
        """Render the template from a dictionary.

        Args:
            params: Dictionary of template parameters.

        Returns:
            str: The rendered output.
        """
        return self._native.render_dict(params)

    def declarations(self) -> list[tuple[str, str]]:
        """Return parameter declarations as (name, type) tuples."""
        return self._native.declarations()

    def __repr__(self) -> str:
        return f"template({self._path!r})"


def template(path: str | os.PathLike[str]) -> _TemplateWithTypes:
    """Load a template and generate typed Python classes from its frontmatter.

    This is the recommended API for using typed templates without the
    import hook. Returns an object with generated types as attributes.

    Args:
        path: Path to a ``.tmpl.md`` template file.

    Returns:
        An object with:
        - Generated type classes as attributes (enums, models, params)
        - A ``render(**kwargs)`` method
        - A ``render_dict(params)`` method

    Example::

        from prompt_templates import template

        review = template("prompts/code_review.tmpl.md")

        # Use generated enum types:
        output = review.render(
            reviewer="Alice",
            items=[
                review.Item(file="main.rs", status=review.Status.Approved),
                review.Item(
                    file="lib.rs",
                    status=review.Status.NeedsChanges(reason="missing tests"),
                ),
            ],
        )

    Raises:
        ValueError: If the template file cannot be read or has syntax errors.
    """
    return _TemplateWithTypes(path)
