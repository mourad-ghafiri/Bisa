/**
 * A scene addon, as facts (18 — Addons): an addon whose manifest says
 * `frame: "scene"` is not a window but the whole desktop under the chrome,
 * see-through, with two modes the host owns — `work`, where the app has the
 * pointer and only the regions the page calls solid take it, and `hunt`,
 * where the scene has it everywhere under the chrome. Nothing here touches
 * the page or the layer: `AddonScene.tsx` reads these and performs them.
 *
 * A scene never publishes its whole box as a browser clear — on macOS a
 * browser tab is a native view above the page, and a viewport-sized hole
 * would blank every tab — so the page is told where the tabs stand
 * (`obstaclesOf`) and only its solid regions are cut around (`solidClears`).
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

import { clearOf } from "../shell/browserClearModel.mjs";
import { SCENE_MODES } from "./addonBridgeModel.mjs";

/** The scene's box on the page: everything under the chrome. @param {{width: number, height: number, top: number}} viewport */
export function sceneBox(viewport) {
  return { left: 0, top: viewport.top, width: Math.max(0, viewport.width), height: Math.max(0, viewport.height - viewport.top) };
}

/**
 * Whether the frame takes the pointer: always in `hunt`; in `work` only
 * while the pointer hovers a solid region — and never while the layer is
 * shielded for another window's drag.
 * @param {{mode: string, hovering: boolean, shielded: boolean}} facts
 */
export function frameTaken({ mode, hovering, shielded }) {
  return mode === "hunt" || (!!hovering && !shielded);
}

/**
 * Whether a page point (viewport pixels) is inside one of the scene's solid
 * regions (scene pixels, from the chrome's bottom).
 * @param {{x: number, y: number}} point
 * @param {ReadonlyArray<{x: number, y: number, width: number, height: number}>} rects
 * @param {number} top the chrome's height
 */
export function pointInSolid(point, rects, top) {
  const x = point.x;
  const y = point.y - top;
  return rects.some((r) => x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height);
}

/**
 * The boxes the page must keep off: every showing browser slot, as the
 * scene sees it (from the chrome's bottom, clamped to it) — a native view
 * paints over whatever walks there. Terminal slots are the page's own DOM
 * and are no obstacle.
 * @param {ReadonlyArray<{rect: {left: number, top: number, width: number, height: number} | null, visible: boolean, layer: string | null}>} slots
 * @param {number} top
 * @returns {Array<{x: number, y: number, width: number, height: number}>}
 */
export function obstaclesOf(slots, top) {
  /** @type {Array<{x: number, y: number, width: number, height: number}>} */
  const out = [];
  for (const s of slots) {
    if (!s.visible || s.layer !== "browser" || !s.rect) continue;
    const sceneTop = s.rect.top - top;
    const y = Math.max(0, sceneTop);
    const height = s.rect.height + Math.min(0, sceneTop);
    if (s.rect.width <= 0 || height <= 0) continue;
    out.push({ x: Math.round(s.rect.left), y: Math.round(y), width: Math.round(s.rect.width), height: Math.round(height) });
  }
  return out;
}

/**
 * The solid regions as browser clears — page boxes rounded outward, with
 * the corner the page asked for — so they show and take clicks over a tab.
 * @param {ReadonlyArray<{x: number, y: number, width: number, height: number, radius?: number}>} rects
 * @param {number} top
 */
export function solidClears(rects, top) {
  const out = [];
  for (const r of rects) {
    const clear = clearOf({ left: r.x, top: r.y + top, width: r.width, height: r.height }, r.radius ?? 0);
    if (clear) out.push(clear);
  }
  return out;
}

/**
 * What the page is told: the mode the host decided, the scene's size, the
 * tabs to keep off, and whether the shell cuts around the solid regions.
 * @param {{mode: string, viewport: {width: number, height: number, top: number}, slots: ReadonlyArray<{rect: {left: number, top: number, width: number, height: number} | null, visible: boolean, layer: string | null}>, cuts: boolean}} facts
 */
export function sceneEvent({ mode, viewport, slots, cuts }) {
  const box = sceneBox(viewport);
  return { mode, viewport: { width: box.width, height: box.height }, obstacles: obstaclesOf(slots, viewport.top), cuts: !!cuts };
}

/** One string for an event, so an effect sends again only when a fact moved. @param {unknown} event */
export function sceneKey(event) {
  return JSON.stringify(event);
}

/** The mode after an ask: one of the two words, or the mode as it stands. @param {string} current @param {unknown} asked */
export function modeAfter(current, asked) {
  return typeof asked === "string" && SCENE_MODES.includes(asked) ? asked : current;
}
