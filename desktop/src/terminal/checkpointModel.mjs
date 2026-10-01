/**
 * When a scrollback checkpoint that keeps failing is said (ide/06): the
 * serialiser throws on a buffer mid-reflow, and one such throw is nothing —
 * the next debounce tries again. A run of them is a terminal whose scrollback
 * will not be there after a restart, and that is said once, at the third in
 * a row, per terminal: one terminal's good checkpoint never forgives
 * another's bad run, and a run that goes on is not said again on every try.
 */

/** How many failures in a row are said. */
export const FAILURES_SAID = 3;

/**
 * A tally of consecutive failures by terminal key.
 * @returns {{failed: (key: string) => boolean, succeeded: (key: string) => void, forget: (key: string) => void, count: (key: string) => number}}
 */
export function failureTally() {
  const runs = new Map();
  return Object.freeze({
    /** One more failure for `key`; true exactly when this one is the one to say. */
    failed(key) {
      const n = (runs.get(key) ?? 0) + 1;
      runs.set(key, n);
      return n === FAILURES_SAID;
    },
    /** A checkpoint of `key` went through: its run is over, and a later one is said afresh. */
    succeeded(key) {
      runs.delete(key);
    },
    /** The terminal is gone. */
    forget(key) {
      runs.delete(key);
    },
    count(key) {
      return runs.get(key) ?? 0;
    },
  });
}
