/**
 * Putting a scrollport back where a person left it (ide/03 §Tabs).
 *
 * A rendering is rarely whole when it mounts: a PDF's pages arrive, a sheet's
 * rows are measured, pictures load and push the text down. Setting
 * `scrollTop` once, at mount, lands short — the browser clamps it to what
 * can be scrolled *then*. So a restore is a few steps: each time the content
 * grows, ask again what to apply, until the kept offset fits.
 *
 * No DOM here: numbers in, numbers out, so `node --test` reads it.
 */

/** The attribute a scrollport that keeps its place is marked with; its value names it within the document. */
export const KEEP_ATTR = "data-scroll-keep";

/** How long a restore keeps trying while a rendering is still growing, in milliseconds. */
export const RESTORE_GRACE_MS = 4000;

/** How far a scrollport can scroll on one axis. */
function reach(size, client) {
  return Math.max(0, Math.round(size) - Math.round(client));
}

/**
 * One step of a restore on one scrollport.
 * @param {{top: number, left: number} | null | undefined} kept
 * @param {{scrollHeight: number, clientHeight: number, scrollWidth: number, clientWidth: number}} box as it measures now
 * @returns {{top: number, left: number, done: boolean}} what to scroll to now, and whether the kept place was reached
 */
export function restoreStep(kept, box) {
  const top = Math.max(0, kept?.top ?? 0);
  const left = Math.max(0, kept?.left ?? 0);
  if (top === 0 && left === 0) return { top: 0, left: 0, done: true };
  const maxTop = reach(box.scrollHeight, box.clientHeight);
  const maxLeft = reach(box.scrollWidth, box.clientWidth);
  // As far as it goes for now; done only once both offsets fit what there is.
  return { top: Math.min(top, maxTop), left: Math.min(left, maxLeft), done: top <= maxTop && left <= maxLeft };
}

/**
 * A place read back from a memory that outlives the window: two whole
 * offsets that are not negative, else nothing — a memory written by hand or
 * by another version is no place.
 * @param {unknown} raw
 * @returns {{top: number, left: number} | null}
 */
export function parsePlace(raw) {
  if (!raw || typeof raw !== "object") return null;
  const { top, left } = raw;
  const whole = (n) => typeof n === "number" && Number.isFinite(n) && n >= 0;
  if (!whole(top) || !whole(left)) return null;
  return placeOf(top, left);
}

/** A scrollport's place as kept: whole pixels, nothing for a scrollport at its origin. */
export function placeOf(scrollTop, scrollLeft) {
  const top = Math.max(0, Math.round(scrollTop));
  const left = Math.max(0, Math.round(scrollLeft));
  return top === 0 && left === 0 ? null : { top, left };
}
