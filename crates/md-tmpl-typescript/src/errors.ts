/**
 * Template error types.
 *
 * Mirrors the Rust `TemplateError` enum and the Python exception hierarchy.
 * All errors extend `TemplateError` (which extends `Error`) so callers
 * can catch the base class or specific subclasses.
 *
 * @module
 */

/** Base class for all template errors. */
export class TemplateError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TemplateError";
  }
}

/** Raised when template source contains syntax errors. */
export class TemplateSyntaxError extends TemplateError {
  readonly line?: number;
  readonly column?: number;
  readonly snippet?: string;

  constructor(message: string, line?: number, snippet?: string);
  constructor(
    message: string,
    line?: number,
    column?: number,
    snippet?: string,
  );
  constructor(
    message: string,
    line?: number,
    columnOrSnippet?: number | string,
    snippet?: string,
  ) {
    let actualLine = line;
    let actualSnippet = snippet;
    if (typeof columnOrSnippet === "string") {
      actualSnippet = columnOrSnippet;
    }
    let formattedMessage = message;
    if (actualLine !== undefined) {
      formattedMessage = `${message} (line ${actualLine}${actualSnippet ? `, --> ${actualSnippet}` : ""})`;
    }
    super(formattedMessage);
    this.name = "TemplateSyntaxError";
    this.line = actualLine;
    if (typeof columnOrSnippet === "string") {
      this.snippet = columnOrSnippet;
      this.column = undefined;
    } else {
      this.column = columnOrSnippet;
      this.snippet = actualSnippet;
    }
  }
}

/** Raised when required parameters are not provided at render time. */
export class MissingParamsError extends TemplateError {
  readonly missing: readonly string[];

  constructor(missing: readonly string[]) {
    super(`missing required parameter(s): ${missing.join(", ")}`);
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
    super(`type mismatch at '${path}': expected ${expected}, got ${actual}`);
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
    super(`extra undeclared parameter(s): ${extra.join(", ")}`);
    this.name = "ExtraParamsError";
    this.extra = extra;
  }
}

/** Raised when an undefined variable is referenced during rendering. */
export class UndefinedVariableError extends TemplateError {
  readonly variable: string;

  constructor(variable: string) {
    super(`undefined variable: ${variable}`);
    this.name = "UndefinedVariableError";
    this.variable = variable;
  }
}

/** Raised when an unknown filter is used. */
export class UnknownFilterError extends TemplateError {
  readonly filter: string;

  constructor(filter: string) {
    super(`unknown filter: ${filter}`);
    this.name = "UnknownFilterError";
    this.filter = filter;
  }
}

/** Raised when a {% panic(...) %} statement is executed during rendering. */
export class TemplatePanicError extends TemplateError {
  constructor(message: string) {
    super(`template panic: ${message}`);
    this.name = "TemplatePanicError";
  }
}
