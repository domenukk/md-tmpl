/**
 * DirectScope — a lightweight variable scope for direct rendering with a
 * pre-allocated loop frame pool for zero per-iteration heap allocations.
 *
 * @module
 */

/** Mutable loop frame updated in place across `{% for %}` iterations. */
export interface DirectLoopFrame {
  binding: string;
  value: unknown;
  index: number;
  len: number;
}

const LOOP_KEY = "loop";

/** A lightweight scope for direct rendering without Map or array allocations per render. */
export class DirectScope {
  private topMap: ReadonlyMap<string, unknown> | undefined;
  topObj: Record<string, unknown> | undefined;
  readonly defaults: ReadonlyMap<string, unknown> | undefined;
  private readonly consts: ReadonlyMap<string, unknown>;
  readonly hasConsts: boolean;
  topFrame: DirectLoopFrame | undefined;
  private loopDepth = 0;
  private readonly loopPool: DirectLoopFrame[];
  private layers: Map<string, unknown>[] | undefined;

  constructor(
    topLevel: ReadonlyMap<string, unknown> | Record<string, unknown>,
    consts: ReadonlyMap<string, unknown>,
    defaults?: ReadonlyMap<string, unknown>,
    _declaredNames?: ReadonlySet<string>,
  ) {
    if (topLevel instanceof Map) {
      this.topMap = topLevel;
    } else {
      this.topObj = topLevel as Record<string, unknown>;
    }
    this.consts = consts;
    this.hasConsts = consts.size > 0;
    this.defaults = defaults && defaults.size > 0 ? defaults : undefined;
    this.loopPool = [];
  }

  /** Reset this scope for a new top-level parameter object without allocating. */
  reset(params: Record<string, unknown>): void {
    this.topObj = params;
    this.topMap = undefined;
    this.topFrame = undefined;
    this.loopDepth = 0;
  }

  /** Fast direct access to the top-level parameter record when available. */
  get paramsObj(): Record<string, unknown> | undefined {
    return this.topObj;
  }

  /** Push a reusable loop frame for `{% for %}` iteration. */
  pushLoop(binding: string, len: number): DirectLoopFrame {
    const depth = this.loopDepth;
    let frame = this.loopPool[depth];
    if (frame === undefined) {
      frame = { binding, value: undefined, index: 0, len };
      this.loopPool.push(frame);
    } else {
      frame.binding = binding;
      frame.value = undefined;
      frame.index = 0;
      frame.len = len;
    }
    this.loopDepth = depth + 1;
    this.topFrame = frame;
    return frame;
  }

  /** Pop the innermost loop frame. */
  popLoop(): void {
    const newDepth = this.loopDepth - 1;
    this.loopDepth = newDepth;
    this.topFrame = newDepth > 0 ? this.loopPool[newDepth - 1] : undefined;
  }

  /** Return the innermost loop frame, if inside a loop. */
  topLoop(): DirectLoopFrame | undefined {
    return this.topFrame;
  }

  resolve(name: string): unknown {
    // 1. Fast path: active loop frames (cannot shadow consts per Rule 11)
    if (this.topFrame !== undefined) {
      if (this.topFrame.binding === name) {
        return this.topFrame.value;
      }
      for (let i = this.loopDepth - 2; i >= 0; i--) {
        const frame = this.loopPool[i];
        if (frame?.binding === name) {
          return frame.value;
        }
      }
      if (name === LOOP_KEY) {
        const top = this.topFrame;
        return {
          first: top.index === 0,
          last: top.len > 0 && top.index + 1 === top.len,
          index0: top.index,
          index: top.index + 1,
          length: top.len,
          len: top.len,
        };
      }
    }

    // 2. Constants (immutable, highest priority)
    if (this.hasConsts) {
      const c = this.consts.get(name);
      if (c !== undefined || this.consts.has(name)) {
        return c;
      }
    }

    // 3. Explicit layers (if any were pushed)
    if (this.layers !== undefined) {
      for (let i = this.layers.length - 1; i >= 0; i--) {
        const layer = this.layers[i];
        if (layer?.has(name)) return layer.get(name);
      }
    }

    // 4. Top-level params + defaults
    if (this.topObj !== undefined) {
      const val = this.topObj[name];
      if (val !== undefined) {
        return val;
      }
      if (this.defaults !== undefined && !Object.hasOwn(this.topObj, name)) {
        return this.defaults.get(name);
      }
      return undefined;
    }

    if (this.topMap !== undefined) {
      const val = this.topMap.get(name);
      if (val !== undefined) {
        return val;
      }
      if (this.defaults !== undefined && !this.topMap.has(name)) {
        return this.defaults.get(name);
      }
    }

    return undefined;
  }

  /** Fast resolution of a single-dot `root.prop` expression. */
  resolveProp1(root: string, prop: string, fullPath: string): unknown {
    const top = this.topFrame;
    if (top !== undefined) {
      if (top.binding === root) {
        const v = top.value;
        return v !== null && typeof v === "object"
          ? (v as Record<string, unknown>)[prop]
          : undefined;
      }
      for (let i = this.loopDepth - 2; i >= 0; i--) {
        const frame = this.loopPool[i];
        if (frame?.binding === root) {
          const v = frame.value;
          return v !== null && typeof v === "object"
            ? (v as Record<string, unknown>)[prop]
            : undefined;
        }
      }
    }
    if (this.hasConsts) {
      const exact = this.consts.get(fullPath);
      if (exact !== undefined) return exact;
    }
    const rootVal = this.resolve(root);
    if (
      rootVal !== null &&
      rootVal !== undefined &&
      typeof rootVal === "object" &&
      !Array.isArray(rootVal)
    ) {
      return (rootVal as Record<string, unknown>)[prop];
    }
    return undefined;
  }

  /** Resolve a variable without throwing (for case label checks). */
  tryResolve(name: string): unknown {
    return this.resolve(name);
  }

  resolveConst(key: string): unknown {
    return this.hasConsts ? this.consts.get(key) : undefined;
  }

  allConsts(): ReadonlyMap<string, unknown> {
    return this.consts;
  }

  pushLayer(): Map<string, unknown> {
    const layer = new Map<string, unknown>();
    this.layers ??= [];
    this.layers.push(layer);
    return layer;
  }

  popLayer(): void {
    this.layers?.pop();
  }

  setLoopIndex(binding: string, index: number, len = 0): void {
    if (this.topFrame?.binding === binding) {
      this.topFrame.index = index;
      return;
    }
    for (let i = this.loopDepth - 2; i >= 0; i--) {
      const frame = this.loopPool[i];
      if (frame?.binding === binding) {
        frame.index = index;
        return;
      }
    }
    this.pushLoop(binding, len).index = index;
  }

  popLoopMeta(): void {
    this.popLoop();
  }

  getLoopIndex(binding: string): number | undefined {
    if (this.topFrame?.binding === binding) {
      return this.topFrame.index;
    }
    for (let i = this.loopDepth - 2; i >= 0; i--) {
      const frame = this.loopPool[i];
      if (frame?.binding === binding) {
        return frame.index;
      }
    }
    return undefined;
  }

  getLastLoopBinding(): string | undefined {
    return this.topFrame?.binding;
  }

  getLastLoopMeta(): { index: number; len: number } | undefined {
    return this.topFrame;
  }
}
