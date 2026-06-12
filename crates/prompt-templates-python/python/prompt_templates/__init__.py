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

from prompt_templates._prompt_templates import (
    Template,
    TemplateCache,
)
from prompt_templates._import_hook import prompt_template_import_hook
from prompt_templates._template_helper import template
from prompt_templates._variants import variant, Variants, load_types


def load_template(path: str) -> Template:
    """Load a template from a ``.tmpl.md`` file.

    Convenience function matching Rust's ``include_template!`` macro.

    Args:
        path: Path to the template file.

    Returns:
        Template: A parsed and validated template.

    Raises:
        ValueError: If the file cannot be read or contains syntax errors.

    Example::

        from prompt_templates import load_template, load_types

        tmpl = load_template("prompts/greeting.tmpl.md")
        types = load_types("prompts/greeting.tmpl.md")
        params = types.Greeting(name="world")
        result = params.render(template=tmpl)
    """
    return Template.from_file(path)


__all__ = [
    "Template",
    "TemplateCache",
    "template",
    "load_template",
    "load_types",
    "prompt_template_import_hook",
    "variant",
    "Variants",
]
