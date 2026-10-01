/**
 * Which drop target a pointer means when several contain it: the smallest —
 * a row inside a heading, a card inside a column — never the container that
 * happens to be drawn around it. A target with no box known sorts last.
 * Plain `.mjs`, so `node --test` reads it.
 */

/**
 * The candidate ids the pointer is within, smallest area first; the order of
 * equals is kept.
 * @template {string | number} Id
 * @param {readonly Id[]} within
 * @param {(id: Id) => {width: number, height: number} | null | undefined} rectOf
 * @returns {Id[]}
 */
export function smallestFirst(within, rectOf) {
  const area = (id) => {
    const r = rectOf(id);
    return r ? r.width * r.height : Number.POSITIVE_INFINITY;
  };
  return [...within].sort((a, b) => area(a) - area(b));
}
