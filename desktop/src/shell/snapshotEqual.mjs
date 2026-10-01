/**
 * Structural equality for a store's snapshot list.
 *
 * A `useSyncExternalStore` store hands React one snapshot object; React
 * re-renders every reader when that reference changes. A poll that returns the
 * same data, or a duplicate stream frame, must therefore keep the old reference
 * or it re-renders the whole shell for nothing. The rows these stores hold are
 * wire DTOs — plain JSON — so element-by-element JSON equality is exact and
 * cheap for the handful of rows a roster or a socket table carries.
 */

/**
 * Whether two lists of JSON-serialisable values are element-for-element equal.
 * Same reference short-circuits; a length difference is an early no.
 * @param {readonly unknown[]} a
 * @param {readonly unknown[]} b
 * @returns {boolean}
 */
export function sameJsonList(a, b) {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (JSON.stringify(a[i]) !== JSON.stringify(b[i])) return false;
  }
  return true;
}
