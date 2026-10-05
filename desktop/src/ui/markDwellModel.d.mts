/**
 * Types for `markDwellModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

/** The beat a working word holds a session's mark before another working word may replace it. */
export declare const DWELL_MS: number;

/**
 * What the mark shows for a state that just arrived: the new state, unless both it and the shown one are
 * working words and the shown one has held for less than `DWELL_MS` — then the shown one, and how long until
 * the new one may show. Attention and ended words show at once.
 */
export declare function shownState<S>(shown: S | null | undefined, next: S, heldMs: number): { state: S; holdMs: number };
