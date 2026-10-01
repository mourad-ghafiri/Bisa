/**
 * The project rail's geometry, in one place: how deep a level indents, how
 * wide the disclosure column is, where a guide line and a row's content
 * sit — so every row of every kind puts its chevron, its glyph and its
 * children's guide in the same column, and a number is never typed twice.
 *
 * The tree (`ui/tree/TreeList.tsx`) turns a depth into a left margin with
 * `indentBase + depth * indent`; the guide of level *d* is drawn at
 * `indentBase + (d - 1) * indent + guideInset`. The rail's rule: one level is
 * one disclosure column, and the guide hangs from the chevron's centre.
 */

/** Pixels per nesting level — one disclosure column. */
export const INDENT_PX = 16;
/** Where a top-level row's content starts. */
export const INDENT_BASE_PX = 8;
/** The disclosure column: the chevron button, or its blank on a leaf. */
export const TWISTY_PX = 16;

/** Where a level's guide line sits inside its column: under the chevron's centre. */
export function guideInset() {
  return TWISTY_PX / 2;
}

/**
 * The token a row kind's height is read from: a project is the one tall
 * card (`--spacing-row`), every other row a tree row (`--spacing-row-sm`).
 * @param {string} kind
 */
export function rowHeightToken(kind) {
  return kind === "project" ? "--spacing-row" : "--spacing-row-sm";
}
