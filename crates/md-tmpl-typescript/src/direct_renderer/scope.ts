/**
 * DirectScope — a lightweight variable scope for direct rendering.
 *
 * @module
 */

// ---------------------------------------------------------------------------
// Direct scope — resolves variables from plain JS values
// ---------------------------------------------------------------------------

/** A lightweight scope for direct rendering. */
export class DirectScope {
  private readonly layers: Map<string, unknown>[] = [];
  private readonly consts: ReadonlyMap<string, unknown>;
  private readonly loopMeta = new Map<string, { index: number }>();
  private lastLoopBinding: string | undefined;

  constructor(
    topLevel: ReadonlyMap<string, unknown>,
    consts: ReadonlyMap<string, unknown>,
  ) {
    this.layers.push(new Map(topLevel));
    this.consts = consts;
  }

  resolve(name: string): unknown {
    // Check layers top-down
    for (let i = this.layers.length - 1; i >= 0; i--) {
      const layer = this.layers[i]!;
      if (layer.has(name)) return layer.get(name);
    }
    // Check consts — return raw value (display conversion happens at render)
    if (this.consts.has(name)) {
      return this.consts.get(name);
    }
    return undefined;
  }

  pushLayer(): Map<string, unknown> {
    const layer = new Map<string, unknown>();
    this.layers.push(layer);
    return layer;
  }

  popLayer(): void {
    this.layers.pop();
  }

  setLoopIndex(binding: string, index: number): void {
    this.loopMeta.set(binding, { index });
    this.lastLoopBinding = binding;
  }

  getLoopIndex(binding: string): number | undefined {
    return this.loopMeta.get(binding)?.index;
  }

  getLastLoopBinding(): string | undefined {
    return this.lastLoopBinding;
  }
}
