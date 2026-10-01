/**
 * What a canvas keeps of where it was left (03-workflows §The designer): the
 * step it had picked and where it looked. Every door back to a designer —
 * the library's card, the Inbox, the Pulse, a conversation's chip — opens a
 * bare `#/workflows/<id>`, so without this a designer came back fitted,
 * nothing picked. With it, the designer opens on the same step at the same
 * place, in this window and after a restart; a step the workflow no longer
 * has is not picked.
 *
 * The memory itself is the view memory's (`shell/viewMemoryStore`, through
 * `designerMemoryStore.ts`): how many places are kept, and for how long, is
 * its rule. What is here is what makes a kept value safe to open on. Facts
 * only, no React. Plain `.mjs`, so `node --test` reads it.
 */

/** The longest id a memory gives back as a step. */
const MAX_ID = 512;

/** What a canvas never opened remembers: nothing. */
export const NOTHING = Object.freeze({ selected: null, viewport: null });

/**
 * A viewport made safe to open on — finite coordinates and a zoom above
 * nothing — else none: a canvas that opens nowhere is worse than a fitted one.
 * @param {unknown} v
 * @returns {{x: number, y: number, zoom: number} | null}
 */
export function viewportOf(v) {
  if (!v || typeof v !== "object") return null;
  const { x, y, zoom } = v;
  const finite = [x, y, zoom].every((n) => typeof n === "number" && Number.isFinite(n));
  if (!finite || zoom <= 0) return null;
  return { x, y, zoom };
}

/**
 * Whether two viewports are the same place.
 * @param {{x: number, y: number, zoom: number} | null | undefined} a
 * @param {{x: number, y: number, zoom: number} | null | undefined} b
 */
export function sameViewport(a, b) {
  return !!a && !!b && a.x === b.x && a.y === b.y && a.zoom === b.zoom;
}

/**
 * A step's id as the memory gives it back, or none: the memory outlives the
 * window, and may have been written by another version or by hand.
 * @param {unknown} raw
 * @returns {string | null}
 */
export function keptStep(raw) {
  return typeof raw === "string" && raw.length > 0 && raw.length <= MAX_ID ? raw : null;
}

/**
 * What the memory kept of one canvas, made safe to open on: the step picked
 * and the place looked at, each what it is or nothing.
 * @param {{selected?: unknown, viewport?: unknown} | null | undefined} raw
 * @returns {{selected: string | null, viewport: {x: number, y: number, zoom: number} | null}}
 */
export function parseMemory(raw) {
  const selected = keptStep(raw?.selected);
  const viewport = viewportOf(raw?.viewport);
  return selected === null && viewport === null ? NOTHING : { selected, viewport };
}

/**
 * The step to pick: `selected` while the workflow still has it, else none —
 * a remembered step an agent's save took, or one deleted since, is no pick.
 * @param {string | null | undefined} selected
 * @param {readonly {id: string}[] | null | undefined} steps
 * @returns {string | null}
 */
export function keptSelection(selected, steps) {
  if (!selected) return null;
  return (steps ?? []).some((s) => s.id === selected) ? selected : null;
}
