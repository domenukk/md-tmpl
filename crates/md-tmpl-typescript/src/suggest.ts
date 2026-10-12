/**
 * "Did you mean …?" suggestions shared by compile-time diagnostics.
 */

/** Largest edit distance at which a declared name is offered as a suggestion. */
export const MAX_SUGGESTION_DISTANCE = 2;

/**
 * Compute the Levenshtein edit distance between two strings.
 *
 * Returns the minimum number of single-character edits (insertions,
 * deletions, or substitutions) required to transform `a` into `b`.
 * Mirrors the Rust core's `levenshtein_distance` so the "did you mean"
 * suggestions in undeclared-variable errors match byte-for-byte.
 */
export function levenshteinDistance(a: string, b: string): number {
  const aLen = a.length;
  const bLen = b.length;
  if (aLen === 0) return bLen;
  if (bLen === 0) return aLen;
  let prev: number[] = Array.from({ length: bLen + 1 }, (_, i) => i);
  let curr: number[] = new Array<number>(bLen + 1).fill(0);
  for (let i = 0; i < aLen; i++) {
    curr[0] = i + 1;
    for (let j = 0; j < bLen; j++) {
      const cost = a[i] === b[j] ? 0 : 1;
      // Non-null: indices are within the pre-sized arrays above.
      curr[j + 1] = Math.min(
        (prev[j] ?? 0) + cost,
        (prev[j + 1] ?? 0) + 1,
        (curr[j] ?? 0) + 1,
      );
    }
    [prev, curr] = [curr, prev];
  }
  return prev[bLen] ?? 0;
}

/**
 * The first candidate (in iteration order) with the smallest non-zero edit
 * distance to `name` that is at most {@link MAX_SUGGESTION_DISTANCE}.
 * Mirrors the Rust core's suggestion loop.
 */
export function closestName(
  name: string,
  candidates: Iterable<string>,
): string | undefined {
  let best: { candidate: string; dist: number } | undefined;
  for (const candidate of candidates) {
    const dist = levenshteinDistance(name, candidate);
    if (
      dist > 0 &&
      dist <= MAX_SUGGESTION_DISTANCE &&
      (best === undefined || dist < best.dist)
    ) {
      best = { candidate, dist };
    }
  }
  return best?.candidate;
}
