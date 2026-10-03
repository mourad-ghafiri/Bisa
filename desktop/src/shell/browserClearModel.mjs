/**
 * What of the browser layer the main page needs see-through, as facts
 * (ide/18). A browser tab is a native view that paints over every DOM
 * element, so the page's floating overlays — the Notes and Draw panels,
 * their docks, the pet, an addon window — would stand under a tab. Each
 * says its box (`shell/browserClear.ts`); this model keeps the ones that
 * meet a showing tab and hands them to the shell, whose browser layer cuts
 * them out (`browser.rs`, `layer`): the overlay shows and takes its own
 * clicks, and the page stays live everywhere else. Plain `.mjs`, so
 * `node --test` reads the rule itself.
 */

/** The most overlays one layer cuts around, the shell's own cap (`clear::MAX_CLEARS`). */
export const MAX_CLEARS = 16;

/**
 * An overlay's box as the layer cuts it: whole CSS pixels rounded outward,
 * so an anti-aliased edge stays inside the hole, and a corner radius no
 * more than half its short side; `null` for a box that is empty or not a
 * number.
 * @param {{left: number, top: number, width: number, height: number}} box
 * @param {number} radius
 * @returns {{left: number, top: number, width: number, height: number, radius: number} | null}
 */
export function clearOf(box, radius) {
  if (![box.left, box.top, box.width, box.height].every(Number.isFinite) || box.width <= 0 || box.height <= 0) return null;
  const left = Math.floor(box.left);
  const top = Math.floor(box.top);
  const width = Math.ceil(box.left + box.width) - left;
  const height = Math.ceil(box.top + box.height) - top;
  const most = Math.min(width, height) / 2;
  return { left, top, width, height, radius: Number.isFinite(radius) && radius > 0 ? Math.min(radius, most) : 0 };
}

/**
 * Whether two boxes share some area — touching edges do not.
 * @param {{left: number, top: number, width: number, height: number}} a
 * @param {{left: number, top: number, width: number, height: number}} b
 */
export function meets(a, b) {
  return a.left < b.left + b.width && b.left < a.left + a.width && a.top < b.top + b.height && b.top < a.top + a.height;
}

/**
 * The overlays the layer must cut: those that meet a slot that holds a
 * browser tab — kept whole, never clipped (the shell cuts only where its
 * tabs are) — in a steady order, at most `MAX_CLEARS`. A slot whose tab is
 * hidden for now (a dialog open) keeps its holes, so the tab shows again
 * already cut around the overlays, never over them for a frame. An overlay
 * beside every tab costs nothing, and with none the layer drops its mask.
 * @param {ReadonlyArray<{id: string, clear: {left: number, top: number, width: number, height: number, radius: number} | null}>} entries
 * @param {ReadonlyArray<{visible: boolean, layer: string | null, rect: {left: number, top: number, width: number, height: number} | null}>} slots
 */
export function clearsOver(entries, slots) {
  const showing = slots.filter((s) => s.layer === "browser" && s.rect && s.rect.width > 0 && s.rect.height > 0);
  return entries
    .filter((e) => e.clear !== null && showing.some((s) => meets(/** @type {any} */ (e.clear), /** @type {any} */ (s.rect))))
    .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    .slice(0, MAX_CLEARS)
    .map((e) => /** @type {{left: number, top: number, width: number, height: number, radius: number}} */ (e.clear));
}

/**
 * Whether two lists cut the same — so an unchanged frame sends nothing.
 * @param {ReadonlyArray<{left: number, top: number, width: number, height: number, radius: number}>} a
 * @param {ReadonlyArray<{left: number, top: number, width: number, height: number, radius: number}>} b
 */
export function sameClears(a, b) {
  return a.length === b.length && a.every((c, i) => sameClear(c, b[i]));
}

/**
 * @param {{left: number, top: number, width: number, height: number, radius: number} | null | undefined} a
 * @param {{left: number, top: number, width: number, height: number, radius: number} | null | undefined} b
 */
export function sameClear(a, b) {
  if (!a || !b) return a === b;
  return a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height && a.radius === b.radius;
}
