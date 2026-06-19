"""prompt_templates — Strongly-typed template engine for LLM prompts.

Python bindings for the Rust ``prompt-templates`` engine. Templates are
``.tmpl.md`` files with YAML frontmatter declaring typed parameters.

Quick start::

    from prompt_templates import Template

    tmpl = Template.from_source('''
    ---
    params:
      - name = str
    ---
    Hello {{ name }}!
    ''')
    print(tmpl.render(name="world"))  # → "Hello world!"

Import hook (import types directly from template files)::

    from prompt_templates import prompt_template_import_hook
    prompt_template_import_hook()

    # Now ``.tmpl.md`` files are importable as Python modules:
    from prompts.code_review import CodeReviewParams, Status

    output = CodeReviewParams(
        reviewer="Alice",
        items=[...],
    ).render()

See the ``template()`` function for a simpler non-import-hook API.
"""

import os
from typing import Any

from prompt_templates._prompt_templates import (
    Template,
    TemplateCache,
    generate_python_source_for_template as _generate_python_source,
)
from prompt_templates._exceptions import (
    ExtraParamsError,
    MissingParamsError,
    TemplateError,
    TemplateSyntaxError,
    TypeMismatchError,
)
from prompt_templates._import_hook import prompt_template_import_hook
from prompt_templates._template_helper import template
from prompt_templates._variants import variant, Variants, load_types


def load_template(path: str | os.PathLike[str]) -> Template:
    """Load a template from a ``.tmpl.md`` file.

    Convenience function matching Rust's ``include_template!`` macro.

    Args:
        path: Path to the template file (str or path-like).

    Returns:
        Template: A parsed and validated template.

    Raises:
        TemplateSyntaxError: If the file contains syntax errors.
        ValueError: If the file cannot be read.

    Example::

        from prompt_templates import load_template, load_types

        tmpl = load_template("prompts/greeting.tmpl.md")
        types = load_types("prompts/greeting.tmpl.md")
        params = types.Greeting(name="world")
        result = params.render(template=tmpl)
    """
    return Template.from_file(os.fspath(path))


def generate_types_source(path: str | os.PathLike[str]) -> str:
    """Generate Python source code with typed classes for a template.

    Write the output to a ``.py`` file for static type checking support
    with mypy/pyright. The generated source uses ``@dataclass`` for model
    classes and ``Variants`` subclasses for enum types.

    Args:
        path: Path to a ``.tmpl.md`` template file.

    Returns:
        Python source code string.

    Example::

        from prompt_templates import generate_types_source

        source = generate_types_source("prompts/review.tmpl.md")
        with open("review_types.py", "w") as f:
            f.write(source)
    """
    return _generate_python_source(os.fspath(path))


__all__ = [
    "ExtraParamsError",
    "MissingParamsError",
    "Template",
    "TemplateCache",
    "TemplateError",
    "TemplateSyntaxError",
    "TypeMismatchError",
    "generate_types_source",
    "load_template",
    "load_types",
    "prompt_template_import_hook",
    "template",
    "variant",
    "Variants",
]
