"""Exception hierarchy for md_tmpl.

Provides specific exception classes so users can catch different
error conditions without parsing error message strings.

All exceptions inherit from :class:`TemplateError`, which itself
inherits from :class:`ValueError` for backwards compatibility.

Every exception carries a stable, machine-readable :attr:`~TemplateError.kind`
attribute that mirrors the Rust core ``ErrorKind`` identifiers (``io``,
``undefined_variable``, ``syntax``, ``missing_params``, ``type_mismatch``,
``unknown_filter``, ``include_not_found``, ``declarations_mutated``,
``extra_params``, ``panic``). The base :class:`TemplateError` uses the empty
string ``""`` as the "unknown" sentinel, matching the Go bindings. Where the
underlying error variant carries payload data, the corresponding exception
exposes it via structured attributes instead of forcing callers to parse the
message.
"""


class TemplateError(ValueError):
    """Base class for all template errors.

    Also serves as the fallback for error variants without a dedicated
    subclass (currently only the I/O error, constructed with
    ``kind="io"``). The class-level :attr:`kind` is the empty-string
    "unknown" sentinel, matching the Go bindings.
    """

    #: Stable, machine-readable error identifier. Overridden per subclass.
    kind: str = ""

    def __init__(self, message: str, *, kind: str | None = None) -> None:
        """Initialise the error.

        Args:
            message: Human-readable error message. ``str(exc)`` returns this.
            kind: Optional per-instance override of the machine-readable
                :attr:`kind`. When ``None`` (the default), the class-level
                :attr:`kind` applies. The pyo3 mapper passes ``kind="io"``
                for the I/O variant.
        """
        super().__init__(message)
        if kind is not None:
            self.kind = kind


class TemplateSyntaxError(TemplateError):
    """Raised when a template contains syntax errors.

    Covers invalid frontmatter and malformed expressions.
    """

    kind = "syntax"

    def __init__(
        self,
        message: str,
        *,
        line: int | None = None,
        snippet: str | None = None,
    ) -> None:
        """Initialise the syntax error.

        Args:
            message: Human-readable error message.
            line: 1-based line number where the error occurred, if known.
            snippet: Snippet of the offending source line, if available.
        """
        super().__init__(message)
        self.line = line
        self.snippet = snippet


class UndefinedVariableError(TemplateError):
    """Raised when a template references a variable not found in the context."""

    kind = "undefined_variable"

    def __init__(self, message: str, *, variable: str) -> None:
        """Initialise the undefined-variable error.

        Args:
            message: Human-readable error message.
            variable: Name of the variable that was not found.
        """
        super().__init__(message)
        self.variable = variable


class MissingParamsError(TemplateError):
    """Raised when required parameters are not provided at render time."""

    kind = "missing_params"

    def __init__(self, message: str, *, missing: list[str]) -> None:
        """Initialise the missing-parameters error.

        Args:
            message: Human-readable error message.
            missing: Names of the required parameters that were not provided.
        """
        super().__init__(message)
        self.missing = missing


class TypeMismatchError(TemplateError, TypeError):
    """Raised when a parameter value has the wrong type.

    Also inherits from :class:`TypeError` for natural ``except TypeError``
    handling.
    """

    kind = "type_mismatch"

    def __init__(
        self,
        message: str,
        *,
        path: str,
        expected: str,
        actual: str,
    ) -> None:
        """Initialise the type-mismatch error.

        Args:
            message: Human-readable error message.
            path: Name/path of the parameter with the wrong type.
            expected: The type declared in frontmatter.
            actual: The type found in the context.
        """
        super().__init__(message)
        self.path = path
        self.expected = expected
        self.actual = actual


class UnknownFilterError(TemplateError):
    """Raised when a template uses a filter that is not registered."""

    kind = "unknown_filter"

    def __init__(self, message: str, *, filter: str) -> None:
        """Initialise the unknown-filter error.

        Args:
            message: Human-readable error message.
            filter: Name of the unknown filter.
        """
        super().__init__(message)
        self.filter = filter


class IncludeNotFoundError(TemplateError):
    """Raised when an ``{% include %}`` references a file that cannot be found."""

    kind = "include_not_found"

    def __init__(self, message: str, *, include: str) -> None:
        """Initialise the include-not-found error.

        Args:
            message: Human-readable error message.
            include: The include path that could not be resolved.
        """
        super().__init__(message)
        self.include = include


class DeclarationsMutatedError(TemplateError):
    """Raised when a template's ``params:`` declarations change at runtime.

    The frontmatter ``params:`` block is part of the compile-time contract
    and must remain stable across hot-reloads.
    """

    kind = "declarations_mutated"

    def __init__(self, message: str, *, details: str) -> None:
        """Initialise the declarations-mutated error.

        Args:
            message: Human-readable error message.
            details: Description of what changed in the declarations.
        """
        super().__init__(message)
        self.details = details


class ExtraParamsError(TemplateError):
    """Raised when undeclared parameters are provided at render time.

    Suppressed when ``allow_extra=True`` is passed.
    """

    kind = "extra_params"

    def __init__(self, message: str, *, extra: list[str]) -> None:
        """Initialise the extra-parameters error.

        Args:
            message: Human-readable error message.
            extra: Names of the undeclared parameters that were provided.
        """
        super().__init__(message)
        self.extra = extra


class TemplatePanicError(TemplateError):
    """Raised when template rendering is halted by an explicit {% panic(...) %} statement."""

    kind = "panic"

    def __init__(self, message: str) -> None:
        """Initialise the panic error.

        Args:
            message: Human-readable panic message.
        """
        super().__init__(message)
