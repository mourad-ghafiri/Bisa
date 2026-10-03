/**
 * Where the Draw panel floats while the Notes panel floats too (19 —
 * Drawings): both are cards anchored in the window's bottom-right corner, so
 * the second would land on the first. Beside it, when the window holds both
 * with their gaps; where it always stands when it does not — the person can
 * shut either, maximize either, or drag a size down. Plain `.mjs`, so
 * `node --test` reads the rule.
 */

/** The corner's inset and the air between the two cards, in CSS pixels (the panels' `right-4`). */
export const INSET = 16;

/**
 * How far left of its corner the Draw panel stands.
 * @param {{ notesWidth: number | null, drawWidth: number, viewport: number }} p `notesWidth` the floating Notes panel's width, `null` when it is not floating
 * @returns {number}
 */
export function besideOffset({ notesWidth, drawWidth, viewport }) {
  if (!(notesWidth !== null && notesWidth > 0) || !(viewport > 0)) return 0;
  return INSET + notesWidth + INSET + drawWidth + INSET <= viewport ? notesWidth + INSET : 0;
}
