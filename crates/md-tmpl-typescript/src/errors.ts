/**
 * Template error types.
 *
 * Mirrors the Rust `TemplateError` enum and the Python exception hierarchy.
 * All errors extend `TemplateError` (which extends `Error`) so callers
 * can catch the base class or specific subclasses.
 *
 * @module
 */

/**
 * Stable, machine-readable error kind identifiers.
 *
 * These mirror the Rust core's `ErrorKind::as_str()` values and the Go
 * binding's `Kind*` constants. They are part of the public contract and must
 * not change between releases. The empty string `""` is the
 * unknown/unclassified sentinel (matching the Go binding's `KindUnknown`).
 */
export type ErrorKind =
  | "io"
  | "undefined_variable"
  | "syntax"
  | "missing_params"
  | "type_mismatch"
  | "unknown_filter"
  | "include_not_found"
  | "declarations_mutated"
  | "extra_params"
  | "panic";

/** Base class for all template errors. */
export class TemplateError extends Error {
  /**
   * Stable, machine-readable identifier for this error's kind.
   *
   * Empty string `""` denotes an unknown/unclassified error.
   */
  readonly kind: ErrorKind | "";

  constructor(message: string, kind: ErrorKind | "" = "") {
    super(message);
    this.name = "TemplateError";
    this.kind = kind;
  }
}

/** Raised when template source contains syntax errors. */
export class TemplateSyntaxError extends TemplateError {
  readonly line?: number;
  readonly column?: number;
  readonly snippet?: string;

  constructor(
    message: string,
    line?: number,
    column?: number,
    snippet?: string,
  ) {
    const formattedMessage =
      line !== undefined
        ? `${message} (line ${line}${snippet ? `, --> ${snippet}` : ""})`
        : message;
    super(formattedMessage, "syntax");
    this.name = "TemplateSyntaxError";
    this.line = line;
    this.column = column;
    this.snippet = snippet;
  }
}

/** Raised when required parameters are not provided at render time. */
export class MissingParamsError extends TemplateError {
  readonly missing: readonly string[];

  constructor(missing: readonly string[]) {
    super(
      `missing required parameter(s): ${missing.join(", ")}`,
      "missing_params",
    );
    this.name = "MissingParamsError";
    this.missing = missing;
  }
}

/** Raised when a parameter value has the wrong type. */
export class TypeMismatchError extends TemplateError {
  readonly path: string;
  readonly expected: string;
  readonly actual: string;

  constructor(path: string, expected: string, actual: string) {
    super(
      `type mismatch at '${path}': expected ${expected}, got ${actual}`,
      "type_mismatch",
    );
    this.name = "TypeMismatchError";
    this.path = path;
    this.expected = expected;
    this.actual = actual;
  }
}

/** Raised when undeclared parameters are provided at render time. */
export class ExtraParamsError extends TemplateError {
  readonly extra: readonly string[];

  constructor(extra: readonly string[]) {
    super(`extra undeclared parameter(s): ${extra.join(", ")}`, "extra_params");
    this.name = "ExtraParamsError";
    this.extra = extra;
  }
}

/** Raised when an undefined variable is referenced during rendering. */
export class UndefinedVariableError extends TemplateError {
  readonly variable: string;

  constructor(variable: string) {
    super(`undefined variable: ${variable}`, "undefined_variable");
    this.name = "UndefinedVariableError";
    this.variable = variable;
  }
}

/** Raised when an unknown filter is used. */
export class UnknownFilterError extends TemplateError {
  readonly filter: string;

  constructor(filter: string) {
    super(`unknown filter: ${filter}`, "unknown_filter");
    this.name = "UnknownFilterError";
    this.filter = filter;
  }
}

/** Raised when a {% panic(...) %} statement is executed during rendering. */
export class TemplatePanicError extends TemplateError {
  constructor(message: string) {
    super(`template panic: ${message}`, "panic");
    this.name = "TemplatePanicError";
  }
}

/**
 * Raised when an included template file cannot be found or loaded.
 *
 * Mirrors the Rust core's `TemplateError::IncludeNotFound`.
 */
export class IncludeNotFoundError extends TemplateError {
  /** The include path that could not be resolved. */
  readonly include: string;

  constructor(include: string) {
    super(`include not found: ${include}`, "include_not_found");
    this.name = "IncludeNotFoundError";
    this.include = include;
  }
}

/**
 * Raised when a runtime-reloaded template's parameter declarations differ
 * from the compile-time contract.
 *
 * The frontmatter `params:` block is part of the compile-time contract and
 * must not be changed. Mirrors the Rust core's
 * `TemplateError::DeclarationsMutated`.
 */
export class DeclarationsMutatedError extends TemplateError {
  /** Human-readable description of what changed. */
  readonly details: string;

  constructor(details: string) {
    super(
      `template parameter declarations were modified at runtime: ${details}. ` +
        "The frontmatter `params:` block is part of the compile-time " +
        "contract and must not be changed",
      "declarations_mutated",
    );
    this.name = "DeclarationsMutatedError";
    this.details = details;
  }
}
