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
    generate_types_for_template,
)
from prompt_templates._import_hook import prompt_template_import_hook
from prompt_templates._template_helper import template
from prompt_templates._variants import variant, Variants, load_types

__all__ = [
    "Template",
    "TemplateCache",
    "template",
    "prompt_template_import_hook",
    "generate_types_for_template",
    "variant",
    "Variants",
    "load_types",
]
