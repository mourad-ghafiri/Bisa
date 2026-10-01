/**
 * What a document's view reads back as (ide/03 §Tabs; `docViewStore.ts`
 * keeps it). A document's view outlives the window now — it is kept in the
 * documents' memory (`shell/viewMemoryStore.ts`), window furniture that
 * another version of the app, or a hand, may have written — so what is read
 * back is made safe before a viewer is handed it: a place is two offsets, a
 * mode is a word the document offers, and an editor's view is an object.
 *
 * Monaco's view state stays opaque: it is Monaco's own format, handed back
 * as it was kept. What this refuses is only what could never have been one
 * — a text, a list, a number.
 *
 * Facts only. Plain `.mjs`, so `node --test` reads it.
 */

import { parsePlace } from "../../ui/keptScrollModel.mjs";

/** The names a document's view is kept under, beside one another in the document's place. */
export const EDITOR_VIEW = "editor";
export const PAGE_SCROLL = "page";
export const DOC_MODE = "mode";

/** The name a marked scrollport's place is kept under. @param {string} name its `data-scroll-keep` */
export function scrollName(name) {
  return `scroll:${name}`;
}

/**
 * An editor's view as it may be handed to the editor: the object that was
 * kept, else nothing.
 * @param {unknown} raw
 * @returns {object | null}
 */
export function editorViewValue(raw) {
  return raw && typeof raw === "object" && !Array.isArray(raw) ? raw : null;
}

/**
 * A scrollport's place as it may be handed to a viewer: the very value that
 * was kept when it is a place as it stands — so what a viewer is handed is
 * the same between renders — the place made of it when it needed rounding,
 * else nothing.
 * @param {unknown} raw
 * @returns {{top: number, left: number} | null}
 */
export function placeValue(raw) {
  const place = parsePlace(raw);
  if (!place) return null;
  return place.top === raw.top && place.left === raw.left ? raw : place;
}
