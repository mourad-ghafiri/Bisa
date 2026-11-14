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

/**
 * A scene at rest: at work, nothing solid, the pointer the app's. One frozen
 * object, so a reset that changes nothing is the same value and draws nothing.
 * @type {Readonly<{mode: "work" | "hunt", solid: ReadonlyArray<{x: number, y: number, width: number, height: number, radius?: number}>, hover: boolean}>}
 */
export const SCENE_REST = Object.freeze({ mode: "work", solid: Object.freeze([]), hover: false });

/**
 * A scene's state after an event — the one place its mode moves, so every way
 * out of a hunt is a row here and none can be forgotten:
 *
 * - `ask` — the page asked for a mode; a word the scene does not know moves nothing.
 * - `leave` — `Esc` while the app has the keyboard.
 * - `solid` — the page said which regions take the pointer at work.
 * - `hover` — the pointer is, or is no longer, over one of them.
 * - `release` — the page gave the pointer back.
 * - `blur` — the window blurred. When the app itself lost focus the pointer
 *   is nobody's and the frame lets go; when focus only moved into the scene's
 *   own frame (`focused`: the document still has it) nothing moves — that
 *   blur comes at the press of a click on the strip, and letting go there
 *   took the frame away before the release, so the click never landed.
 * - `hello` — the page said hello again: it started again, so it is at work.
 *   Its regions stand until it says them anew — a page places its strip once
 *   and says so once, and a hello repeated before its ready landed must not
 *   take the strip from under the pointer.
 * - `frame` — another frame is another page: nothing of the last one stands.
 *   A hunt kept across a reload was a see-through pane that took every click
 *   for a page that might never load.
 *
 * Every way back to work lets go of the pointer. The same object comes back
 * when nothing moved.
 * @param {typeof SCENE_REST} state
 * @param {{type: string, mode?: unknown, rects?: ReadonlyArray<{x: number, y: number, width: number, height: number, radius?: number}>, on?: boolean, focused?: boolean} | null | undefined} event
 * @returns {typeof SCENE_REST}
 */
export function sceneAfter(state, event) {
  switch (event?.type) {
    case "ask": {
      const mode = modeAfter(state.mode, event.mode);
      if (mode === state.mode) return state;
      return mode === "work" ? { ...state, mode, hover: false } : { ...state, mode };
    }
    case "leave":
    case "hello":
      return state.mode === "work" && !state.hover ? state : { ...state, mode: "work", hover: false };
    case "solid":
      return { ...state, solid: event.rects ?? [] };
    case "hover":
      return state.hover === !!event.on ? state : { ...state, hover: !!event.on };
    case "release":
      return state.hover ? { ...state, hover: false } : state;
    case "blur":
      return state.hover && !event.focused ? { ...state, hover: false } : state;
    case "frame":
      return SCENE_REST;
    default:
      return state;
  }
}

/**
 * How much of its bottom edge a scene gives up, in pixels: the footer's
 * height while it hunts, nothing at work. A hunt takes every pointer event
 * under the chrome, and the page that could end it may have stopped
 * answering — so the footer stays the app's, and the Addons control in it can
 * always put the scene away. At work the scene takes only its solid regions,
 * and the insects cross the footer as they cross everything else.
 * @param {string} mode @param {number} footer the footer's height
 */
export function footerKept(mode, footer) {
  return mode === "hunt" && footer > 0 ? Math.ceil(footer) : 0;
}
