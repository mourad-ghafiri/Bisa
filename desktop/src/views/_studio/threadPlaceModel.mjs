/**
 * Where a thread was being read (13 — Conversations; `crates/desktop.md`
 * §Per-viewer state), so that leaving a thread — and closing the app — come
 * back to the message a person was on, not to the newest.
 *
 * A place is a **message and an offset**, never a pixel count from the top:
 * a thread grows at both ends — replies land at its foot, older pages above
 * — so a `scrollTop` kept yesterday names another message today. The message
 * is the first row whose foot is below the viewport's top edge, the offset
 * how far that edge is inside it. **At the bottom** is no place at all
 * (`null`): a thread left at its bottom opens at its bottom, following the
 * newest as it always did.
 *
 * Putting a place back is a few steps (`restoreStep`): the first page may
 * still be out, and the message may be older than the page — then older
 * pages are read, `RESTORE_PAGES` at most, and a place that cannot be reached
 * is forgotten rather than approximated.
 *
 * No DOM here: numbers in, facts out, so `node --test` reads it.
 */

/** How many older pages a restore reads to reach a kept message before it gives the place up. */
export const RESTORE_PAGES = 4;

/** The longest id a place is read back with; a longer one is no id of ours. */
const MAX_ID = 256;
/** The furthest an edge stands from its row's top, either way; past it the memory was not written by this screen. */
const MAX_OFFSET = 1_000_000;

/**
 * The place a viewport stands at: the first row whose foot is below the
 * viewport's top edge, and how far the edge is inside it — negative when the
 * edge stands above the row, in what is drawn between two messages. `null`
 * for a thread with no rows, or one scrolled past its last.
 * @param {readonly {id: string, top: number, height: number}[]} rows the message rows in document order, measured from the content's top
 * @param {number} top the viewport's top edge, measured the same way — its `scrollTop`
 * @returns {{message: string, offset: number} | null}
 */
export function placeFrom(rows, top) {
  const edge = Number(top) || 0;
  for (const row of rows ?? []) {
    if (!row || typeof row.id !== "string" || !row.id) continue;
    if (row.top + row.height > edge) return { message: row.id, offset: Math.round(edge - row.top) };
  }
  return null;
}

/**
 * A place read back from a memory that outlives the window: a message's id
 * and a whole offset, else nothing — a memory written by hand or by another
 * version is no place.
 * @param {unknown} raw
 * @returns {{message: string, offset: number} | null}
 */
export function parseThreadPlace(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const { message, offset } = raw;
  if (typeof message !== "string" || message.length === 0 || message.length > MAX_ID) return null;
  if (typeof offset !== "number" || !Number.isFinite(offset) || Math.abs(offset) > MAX_OFFSET) return null;
  return { message, offset: Math.round(offset) };
}

/**
 * One step of putting a thread back where it was being read.
 *
 * - Nothing kept: the thread starts at its bottom, as it always did.
 * - The first page is out: nothing moves — a skeleton has no message to stand on.
 * - The kept message is in the page: scroll to it.
 * - It is not, and there is more above: read an older page — `RESTORE_PAGES` at most.
 * - It cannot be reached — retracted out of the page, deleted, further back
 *   than the bound: the thread starts at its bottom and the place is forgotten.
 * @param {{kept: {message: string, offset: number} | null | undefined, loading: boolean, ids: readonly string[], hasOlder: boolean, pagesLoaded: number}} facts
 * @returns {{do: "wait"} | {do: "scroll", message: string, offset: number} | {do: "older"} | {do: "bottom", forget: boolean}}
 */
export function restoreStep({ kept, loading, ids, hasOlder, pagesLoaded }) {
  if (!kept) return { do: "bottom", forget: false };
  if (loading) return { do: "wait" };
  if ((ids ?? []).includes(kept.message)) return { do: "scroll", message: kept.message, offset: kept.offset };
  if (hasOlder && (Number(pagesLoaded) || 0) < RESTORE_PAGES) return { do: "older" };
  return { do: "bottom", forget: true };
}
