/**
 * The Details pane's facts (ide/18 §The Browser pane): its toggle, and its
 * width.
 *
 * **The toggle.** The same occupant asked with no tab in mind, or with the
 * tab it already shows, closes the pane — so ⌘⇧L, the palette and a screen's
 * Browser button close a pane opened on a tab at once; the same occupant
 * asked with another tab switches to it; another occupant opens.
 *
 * **The width.** The pane's edge drags between a floor of `AUX_MIN_WIDTH`
 * and **seven tenths of the room** beside the sidebar — the content column
 * `App.tsx` measures — never a fixed number of pixels: 720 px was a third
 * of a wide window, and the browser lives here. The width a person chose is
 * kept as they chose it (`bisa.aux.width`); what is *drawn* is that width
 * held within the room's bounds (`shownWidth`), so a window made narrower
 * narrows the pane and a window made wider gives the chosen width back. A
 * column not yet measured bounds nothing above the floor: the pane opens at
 * its stored width until the room is known. Plain `.mjs`, so `node --test`
 * reads it.
 */

/** Where the pane's width is kept, per viewer. */
export const AUX_WIDTH_KEY = "bisa.aux.width";
/** The width a pane opens at, and returns to on a double-click of its edge. */
export const AUX_DEFAULT_WIDTH = 380;
/** The narrowest the pane goes: a strip, a bar and a page still read. */
export const AUX_MIN_WIDTH = 300;
/** The widest, as a share of the room beside the sidebar. */
export const AUX_MAX_SHARE = 0.7;

/**
 * The pane's bounds in a column `available` pixels wide: the floor, and
 * seven tenths of the room — never below the floor, so the handle keeps a
 * range; a column not yet measured (nothing, zero, not a number) bounds
 * nothing above the floor.
 * @param {number | null | undefined} available
 * @returns {{min: number, max: number}}
 */
export function auxBounds(available) {
  const room = Number(available);
  if (!Number.isFinite(room) || room <= 0) return { min: AUX_MIN_WIDTH, max: Number.POSITIVE_INFINITY };
  // Whole pixels; the share is read a hair above itself so 1440 × 0.7 is 1008, not 1007.999.
  return { min: AUX_MIN_WIDTH, max: Math.max(AUX_MIN_WIDTH, Math.floor(room * AUX_MAX_SHARE + 1e-6)) };
}

/**
 * The width drawn for a stored one: held within the bounds, the stored
 * number left as it is.
 * @param {number} stored
 * @param {{min: number, max: number}} bounds
 */
export function shownWidth(stored, bounds) {
  return Math.min(bounds.max, Math.max(bounds.min, stored));
}

/**
 * @param {{kind: string | null, id: string | null}} showing what the pane shows now
 * @param {string} next the occupant asked for
 * @param {string | undefined} nextId the tab asked for, when one is named
 * @returns {{aux: string, auxId: string | null} | null} the address to open on, or null to close
 */
export function toggledAux(showing, next, nextId) {
  const same = showing.kind === next && (nextId === undefined || nextId === showing.id);
  return same ? null : { aux: next, auxId: nextId ?? null };
}
