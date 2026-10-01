/**
 * The platform's shape libraries (19 — Drawings): the software-engineering
 * shapes, the stickers, the drawing elements — the canvas's library sidebar
 * holds them, handed in as `initialData.libraryItems`. Each item is a
 * skeleton; `buildLibrary` turns them into the canvas's `LibraryItems` with
 * the converter the caller lends, so this stays data with no canvas in it.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

import { DRAWING_ITEMS } from "./drawing.mjs";
import { SOFTWARE_ITEMS } from "./software.mjs";
import { STICKER_ITEMS } from "./stickers.mjs";

/** Every item, in the order the sidebar lists them. */
export const LIBRARY_ITEMS = Object.freeze([...SOFTWARE_ITEMS, ...STICKER_ITEMS, ...DRAWING_ITEMS]);

/**
 * The canvas's library items from ours.
 * @template E
 * @param {(skeleton: Record<string, unknown>[]) => E[]} convert the canvas's `convertToExcalidrawElements`, or an identity in a test
 * @param {number} [created] epoch milliseconds
 * @returns {{id: string, status: "published", created: number, name: string, elements: E[]}[]}
 */
export function buildLibrary(convert, created = Date.now()) {
  return LIBRARY_ITEMS.map((item) => ({
    id: `bisa-${item.id}`,
    status: "published",
    created,
    name: item.name,
    elements: convert(item.skeleton()),
  }));
}
