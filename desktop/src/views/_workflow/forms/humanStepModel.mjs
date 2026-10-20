/**
 * The human step's options as the form edits them. Plain `.mjs`, so
 * `node --test` reads it.
 */

/**
 * One option recommended at most: pressing the mark on option `i` makes it
 * the recommended one, or takes the mark off it; every other loses it.
 * @template {{recommended?: boolean}} O
 * @param {readonly O[]} options @param {number} i
 * @returns {O[]}
 */
export function pickRecommended(options, i) {
  return (options ?? []).map((o, j) => ({ ...o, recommended: j === i ? !o.recommended : false }));
}
