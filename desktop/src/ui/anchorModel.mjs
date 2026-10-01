/**
 * A list's place kept as a row, not as pixels (`VirtualList`'s `keepAnchor`).
 *
 * A feed grows at its head: forty rows land above what a person was reading,
 * and the pixels that were kept now fall on another row. So the place is the
 * first row in view and how far into it the viewport's top edge stood;
 * putting it back is finding that row again, wherever it is now.
 *
 * No DOM here: rows in, numbers out, so `node --test` reads it.
 */

/** The longest key an anchor read back from a memory names. */
const MAX_KEY = 512;

/**
 * The first row whose foot is below the viewport's top edge — the row a
 * person sees first — or -1 for no rows, or a viewport below them all.
 * @param {readonly {key: string, top: number, height: number}[]} rows top to bottom
 * @param {number} top the viewport's top edge, in the list's own pixels
 * @returns {number} the row's index in `rows`
 */
export function firstVisible(rows, top) {
  for (let i = 0; i < rows.length; i++) {
    if (rows[i].top + rows[i].height > top) return i;
  }
  return -1;
}

/**
 * The place as it is kept: the first row in view, and how far below its
 * top the viewport's top edge stands — negative for a viewport above the
 * row. Nothing for a list at its origin: there is no place to keep.
 * @param {readonly {key: string, top: number, height: number}[]} rows top to bottom
 * @param {number} top the viewport's top edge
 * @returns {{key: string, offset: number} | null}
 */
export function anchorOf(rows, top) {
  if (!(top > 0)) return null;
  const at = firstVisible(rows, top);
  if (at < 0) return null;
  const row = rows[at];
  return { key: row.key, offset: Math.round(top - row.top) };
}

/**
 * The `scrollTop` that puts an anchor's row back where it stood, or `null`
 * when the row is gone — an anchor whose row is gone is no place.
 * @param {{key: string, offset: number} | null | undefined} anchor
 * @param {readonly {key: string, top: number, height: number}[]} rows
 * @returns {number | null}
 */
export function scrollFor(anchor, rows) {
  if (!anchor) return null;
  const row = rows.find((r) => r.key === anchor.key);
  if (!row) return null;
  return Math.max(0, Math.round(row.top + anchor.offset));
}

/**
 * An anchor read back from a memory that outlives the window: a row's key
 * and a whole offset, else nothing — a memory written by hand or by another
 * version is no anchor.
 * @param {unknown} raw
 * @returns {{key: string, offset: number} | null}
 */
export function parseAnchor(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const { key, offset } = raw;
  if (typeof key !== "string" || key.length === 0 || key.length > MAX_KEY) return null;
  if (typeof offset !== "number" || !Number.isFinite(offset)) return null;
  return { key, offset: Math.round(offset) };
}
