/**
 * Who gets the GPU (ide/06, ide/14): the WebGL renderer is leased to the
 * terminals a person can see, and handed back by the ones they cannot.
 *
 * Chromium and WebKit cap live WebGL contexts per page (around sixteen) and,
 * past the cap, evict the *oldest* rather than refuse the newest — so an
 * unbounded policy degrades the pane you were not touching. The pool is the
 * bound: `BUDGET` slots, the visible panes first, the active one before the
 * rest; a pane that has been hidden for `RELEASE_AFTER_MS` gives its slot back
 * and draws with the DOM renderer until it is seen again. Pure, so the rule is
 * tested here and `Terminal.tsx` only calls it.
 */

/** How many terminals hold the WebGL renderer at once. */
export const BUDGET = 8;
/** How long a hidden terminal keeps its slot — a tab switch and back costs nothing. */
export const RELEASE_AFTER_MS = 2000;

export function emptyPool() {
  return { held: [] };
}

/** Whether a terminal in this situation should hold a slot at all. */
export function wants({ visible }) {
  return visible === true;
}

/** Whether `key` holds a slot. */
export function holds(pool, key) {
  return pool.held.includes(key);
}

/**
 * Ask for a slot. Granted when one is free or already held; refused — the
 * DOM renderer draws — when the budget is spent. First asked, first served:
 * a pane that comes into sight asks as it does, and a hidden one gives its
 * slot back after `RELEASE_AFTER_MS`, so the budget follows what is seen.
 * @returns {{pool: {held: string[]}, granted: boolean}}
 */
export function acquire(pool, key) {
  if (holds(pool, key)) return { pool, granted: true };
  if (pool.held.length >= BUDGET) return { pool, granted: false };
  return { pool: { held: [...pool.held, key] }, granted: true };
}

/** Give a slot back. A key that holds none changes nothing. */
export function release(pool, key) {
  if (!holds(pool, key)) return pool;
  return { held: pool.held.filter((k) => k !== key) };
}

