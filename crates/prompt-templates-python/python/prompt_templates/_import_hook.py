"""PEP 302 import hook for ``.tmpl.md`` template files.

After calling ``prompt_template_import_hook()``, you can import typed
classes directly from your template files:

    from prompts.code_review import CodeReviewParams, Status

The hook intercepts import statements, finds the corresponding
``.tmpl.md`` file, parses its frontmatter, and dynamically generates
Python classes (params models, enums, nested item models).

The generated module is cached in ``sys.modules`` — subsequent imports
are free.

Configuration:

    ``prompt_template_import_hook(search_paths=["prompts/", "other/"])``

    By default, searches the current working directory. Pass explicit
    paths to limit where ``.tmpl.md`` files are discovered.
"""

from __future__ import annotations

import importlib
import importlib.abc
import importlib.machinery
import os
import sys
import types
from pathlib import Path
from typing import Any, Callable, Sequence

# Lazy import to avoid circular dependency at module load time.
_generate_types: Callable[..., Any] | None = None


def _get_generate_types() -> Callable[..., Any]:
    """Lazy-load the native type generator."""
    global _generate_types
    if _generate_types is None:
        from prompt_templates._prompt_templates import generate_types_for_template

        _generate_types = generate_types_for_template
    return _generate_types


def prompt_template_import_hook(
    search_paths: Sequence[str | Path] | None = None,
) -> None:
    """Install the ``.tmpl.md`` import hook into ``sys.meta_path``.

    After calling this function, ``.tmpl.md`` template files become
    importable as Python modules. The frontmatter type declarations are
    used to generate typed Python classes.

    Args:
        search_paths: Directories to search for ``.tmpl.md`` files.
            Defaults to the current working directory. Paths are resolved
            relative to the current working directory at import time.

    Example::

        from prompt_templates import prompt_template_import_hook

        # Install with default search paths (cwd)
        prompt_template_import_hook()

        # Or specify explicit paths
        prompt_template_import_hook(search_paths=["prompts/", "templates/"])

        # Now import directly from template files:
        from prompts.greeting import GreetingParams
        output = GreetingParams(name="world").render()

    Notes:
        - The hook only triggers for paths that contain a ``.tmpl.md``
          file; normal Python imports are unaffected.
        - Generated modules are cached in ``sys.modules``.
        - Calling this function multiple times is safe; subsequent calls
          update the search paths but don't duplicate the hook.
    """
    resolved = [str(Path(p).resolve()) for p in (search_paths or ["."])]

    # Check if we already installed a hook — update paths instead of duplicating.
    for entry in sys.meta_path:
        if isinstance(entry, _TemplateFinder):
            entry.search_paths = resolved
            return

    sys.meta_path.append(_TemplateFinder(resolved))


class _TemplateFinder(importlib.abc.MetaPathFinder):
    """Meta-path finder that locates ``.tmpl.md`` files."""

    def __init__(self, search_paths: list[str]) -> None:
        self.search_paths = search_paths

    def find_spec(
        self,
        fullname: str,
        path: Sequence[str] | None,
        target: types.ModuleType | None = None,
    ) -> importlib.machinery.ModuleSpec | None:
        # Convert dotted module name to a file path.
        # e.g. "prompts.code_review" → "prompts/code_review.tmpl.md"
        parts = fullname.split(".")
        relative = os.path.join(*parts) + ".tmpl.md"

        for search_dir in self.search_paths:
            candidate = os.path.join(search_dir, relative)
            if os.path.isfile(candidate):
                return importlib.machinery.ModuleSpec(
                    fullname,
                    _TemplateLoader(candidate),
                    origin=candidate,
                )

            # Also try: the last part as filename in a directory matching
            # the parent parts. e.g. "prompts.code_review" could be
            # "code_review.tmpl.md" in a "prompts/" directory.
            if len(parts) > 1:
                parent_dir = os.path.join(search_dir, *parts[:-1])
                candidate = os.path.join(parent_dir, parts[-1] + ".tmpl.md")
                if os.path.isfile(candidate):
                    return importlib.machinery.ModuleSpec(
                        fullname,
                        _TemplateLoader(candidate),
                        origin=candidate,
                    )

        return None


class _TemplateLoader(importlib.abc.Loader):
    """Module loader that generates Python types from a ``.tmpl.md`` file."""

    def __init__(self, file_path: str) -> None:
        self.file_path = file_path

    def create_module(self, spec: importlib.machinery.ModuleSpec) -> None:
        # Use default module creation.
        return None

    def exec_module(self, module: types.ModuleType) -> None:
        generate = _get_generate_types()
        generated_types = generate(self.file_path)

        # Populate the module with generated types.
        for name, cls in generated_types.items():
            setattr(module, name, cls)

        # Also expose the raw Template for advanced usage.
        from prompt_templates._prompt_templates import Template

        setattr(module, "_template_path", self.file_path)
        setattr(module, "_Template", Template)

        module.__file__ = self.file_path
        module.__loader__ = self
