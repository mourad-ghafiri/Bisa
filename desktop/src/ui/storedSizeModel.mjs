/**
 * A pane size read back from storage. What the resize handle enforces
 * while dragging — the pane's minimum and maximum — is enforced again on
 * the way in, so a value written by a wider window or another build never
 * opens a pane past the screen or a canvas with no width; anything that is
 * not a positive number is the default.
 */

/**
 * @param {string | null | undefined} raw what storage holds
 * @param {{ initial: number, min?: number, max?: number }} bounds
 * @returns {number}
 */
export function clampStored(raw, bounds) {
  const n = raw == null || raw === "" ? NaN : Number(raw);
  if (!Number.isFinite(n) || n <= 0) return bounds.initial;
  return clampSize(n, bounds);
}

/**
 * A size already read, held within bounds — what a pane draws when the room
 * it stands in is smaller than the width it remembers. Pure: the stored
 * number is the caller's to keep.
 * @param {number} px
 * @param {{ min?: number, max?: number }} bounds
 * @returns {number}
 */
export function clampSize(px, bounds) {
  const min = bounds.min ?? 0;
  const max = bounds.max ?? Number.POSITIVE_INFINITY;
  return Math.min(max, Math.max(min, px));
}
