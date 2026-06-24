/**
 * Template rendering context — holds all variables available during rendering.
 *
 * Mirrors the Rust `Context` struct and the Python kwargs API.
 *
 * @module
 */

import { type Value, fromJs } from "./value.js";

/**
 * Template rendering context.
 *
 * Holds top-level variables that are resolved during rendering. Values
 * are automatically converted from plain JS types to typed `Value`s.
 *
 * @example
 * ```ts
 * const ctx = new Context();
 * ctx.set("name", "world");
 * ctx.set("count", 42);
 * ```
 *
 * @example Builder-style
 * ```ts
 * const ctx = Context.from({ name: "world", count: 42 });
 * ```
 */
export class Context {
  /** Internal variable storage. */
  readonly values: Map<string, Value> = new Map();

  /** Create an empty context. */
  constructor() {}

  /** Insert a value into the context. Plain JS values are auto-converted. */
  set(key: string, value: unknown): void {
    this.values.set(key, fromJs(value));
  }

  /** Insert a pre-converted Value directly into the context. */
  setRaw(key: string, value: Value): void {
    this.values.set(key, value);
  }

  /** Look up a top-level variable. */
  get(key: string): Value | undefined {
    return this.values.get(key);
  }

  /** Returns `true` if a variable with the given key exists. */
  has(key: string): boolean {
    return this.values.has(key);
  }

  /** Returns the number of variables. */
  get size(): number {
    return this.values.size;
  }

  /** Iterate over all entries. */
  entries(): IterableIterator<[string, Value]> {
    return this.values.entries();
  }

  /** Return all keys. */
  keys(): IterableIterator<string> {
    return this.values.keys();
  }

  /**
   * Create a context from a plain object.
   *
   * Each value is recursively converted via `fromJs()`.
   *
   * @example
   * ```ts
   * const ctx = Context.from({
   *   name: "Alice",
   *   items: [{ label: "hello" }],
   * });
   * ```
   */
  static from(obj: Record<string, unknown>): Context {
    const ctx = new Context();
    for (const [k, v] of Object.entries(obj)) {
      ctx.set(k, v);
    }
    return ctx;
  }
}
