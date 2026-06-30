"""Exception hierarchy for md_tmpl.

Provides specific exception classes so users can catch different
error conditions without parsing error message strings.

All exceptions inherit from :class:`TemplateError`, which itself
inherits from :class:`ValueError` for backwards compatibility.
"""


class TemplateError(ValueError):
    """Base class for all template errors."""


class TemplateSyntaxError(TemplateError):
    """Raised when a template contains syntax errors.

    Covers invalid frontmatter, malformed expressions, and
    undeclared variable references.
    """


class MissingParamsError(TemplateError):
    """Raised when required parameters are not provided at render time."""


class TypeMismatchError(TemplateError, TypeError):
    """Raised when a parameter value has the wrong type.

    Also inherits from :class:`TypeError` for natural ``except TypeError``
    handling.
    """


class ExtraParamsError(TemplateError):
    """Raised when undeclared parameters are provided at render time.

    Suppressed when ``allow_extra=True`` is passed.
    """
