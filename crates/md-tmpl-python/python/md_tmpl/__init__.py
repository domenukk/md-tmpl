"""md_tmpl — Strongly-typed template engine for LLM prompts.

Python bindings for the Rust ``md-tmpl`` engine. Templates are
``.tmpl.md`` files with YAML frontmatter declaring typed parameters.

Quick start::

    from md_tmpl import Template

    tmpl = Template.from_source('''
    ---
    params:
      - name = str
    ---
    Hello {{ name }}!
    ''')
    print(tmpl.render(name="world"))  # → "Hello world!"

Import hook (import types directly from template files)::

    from md_tmpl import md_tmpl_import_hook
    md_tmpl_import_hook()

    # Now ``.tmpl.md`` files are importable as Python modules:
    from prompts.code_review import CodeReviewParams, Status

    output = CodeReviewParams(
        reviewer="Alice",
        items=[...],
    ).render()

See the ``template()`` function for a simpler non-import-hook API.
"""

import os

from md_tmpl._md_tmpl import (
    DEFAULT_QUARANTINE_TAG,
    DEFAULT_SANITIZE_NOTICE,
    DEFAULT_SANITIZE_TAG,
    ROLE_TOKEN_DELIMITERS,
    TOKEN_DELIMITERS,
    Template,
    TemplateCache,
    escape_json,
    escape_xml,
    fence,
    generate_python_source_for_template as _generate_python_source,
    has_control_tokens,
    has_quarantine_tag_breakout,
    has_role_control_tokens,
    has_untrusted_breakout,
    is_quarantined,
    is_sanitized_block,
    quarantine,
    quarantine_untrusted,
    sanitize,
    sanitize_block,
    sanitize_quarantine_payload,
    sanitize_role_tokens,
    sanitize_tokens,
    sanitize_untrusted,
    unsanitize_block,
)
from md_tmpl._exceptions import (
    DeclarationsMutatedError,
    ExtraParamsError,
    IncludeNotFoundError,
    MissingParamsError,
    TemplateError,
    TemplatePanicError,
    TemplateSyntaxError,
    TypeMismatchError,
    UndefinedVariableError,
    UnknownFilterError,
)
from md_tmpl._import_hook import md_tmpl_import_hook
from md_tmpl._template_helper import template
from md_tmpl._variants import variant, Variants, load_types


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

        from md_tmpl import load_template, load_types

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

        from md_tmpl import generate_types_source

        source = generate_types_source("prompts/review.tmpl.md")
        with open("review_types.py", "w") as f:
            f.write(source)
    """
    return _generate_python_source(os.fspath(path))


__all__ = [
    "DEFAULT_QUARANTINE_TAG",
    "DEFAULT_SANITIZE_NOTICE",
    "DEFAULT_SANITIZE_TAG",
    "DeclarationsMutatedError",
    "ExtraParamsError",
    "IncludeNotFoundError",
    "MissingParamsError",
    "ROLE_TOKEN_DELIMITERS",
    "TOKEN_DELIMITERS",
    "Template",
    "TemplateCache",
    "TemplateError",
    "TemplatePanicError",
    "TemplateSyntaxError",
    "TypeMismatchError",
    "UndefinedVariableError",
    "UnknownFilterError",
    "escape_json",
    "escape_xml",
    "fence",
    "generate_types_source",
    "has_control_tokens",
    "has_quarantine_tag_breakout",
    "has_role_control_tokens",
    "has_untrusted_breakout",
    "is_quarantined",
    "is_sanitized_block",
    "load_template",
    "load_types",
    "md_tmpl_import_hook",
    "quarantine",
    "quarantine_untrusted",
    "sanitize",
    "sanitize_block",
    "sanitize_quarantine_payload",
    "sanitize_role_tokens",
    "sanitize_tokens",
    "sanitize_untrusted",
    "template",
    "unsanitize_block",
    "variant",
    "Variants",
]
