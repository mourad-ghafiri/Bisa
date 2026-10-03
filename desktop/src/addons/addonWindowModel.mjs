/**
 * Where an addon's window stands and how big it is, as facts (18 — Addons).
 *
 * A window is a dock like the pet's — a placement anchored to its nearest
 * edges (`ui/dockModel.mjs`), painted clamped, never stored clamped — with a
 * **size** beside it, bounded by what the manifest allows and by the window
 * it stands in. Everything a drag, a resize, a first opening or a stored
 * preference has to get right is here, with tests; the component paints.
 */

import { placementFrom } from "../ui/dockModel.mjs";

/** The slim bar a framed window wears, in CSS px. */
export const BAR_HEIGHT = 24;
/** The least a window may shrink to, when the manifest names nothing. */
export const DEFAULT_MIN = Object.freeze({ width: 120, height: 80 });
/** How far from a corner the first window opens, and how far each next one is staggered. */
export const EDGE = 24;
export const STAGGER = 16;
/** What the bottom-right corner already holds: the notes dock and the pet. */
export const BOTTOM_RIGHT_CLEARANCE = 160;
/** Room a window keeps from the viewport's edges when the manifest allows more. */
export const VIEWPORT_MARGIN = 16;

/**
 * The size bounds a manifest allows, kept inside the viewport.
 * @param {{width: number, height: number, min_width?: number | null, min_height?: number | null, max_width?: number | null, max_height?: number | null}} manifestWindow
 * @param {{width: number, height: number, top: number}} viewport
 * @returns {{minWidth: number, minHeight: number, maxWidth: number, maxHeight: number}}
 */
export function sizeBounds(manifestWindow, viewport) {
  const roomW = Math.max(DEFAULT_MIN.width, viewport.width - 2 * VIEWPORT_MARGIN);
  const roomH = Math.max(DEFAULT_MIN.height, viewport.height - viewport.top - 2 * VIEWPORT_MARGIN);
  const minWidth = Math.min(manifestWindow.min_width ?? DEFAULT_MIN.width, roomW);
  const minHeight = Math.min(manifestWindow.min_height ?? DEFAULT_MIN.height, roomH);
  const maxWidth = Math.max(minWidth, Math.min(manifestWindow.max_width ?? roomW, roomW));
  const maxHeight = Math.max(minHeight, Math.min(manifestWindow.max_height ?? roomH, roomH));
  return { minWidth, minHeight, maxWidth, maxHeight };
}

/**
 * A size inside the bounds, rounded to whole pixels.
 * @param {{width: number, height: number}} size
 * @param {{minWidth: number, minHeight: number, maxWidth: number, maxHeight: number}} bounds
 */
export function clampSize(size, bounds) {
  const clamp = (n, lo, hi) => Math.round(Math.min(Math.max(Number(n) || lo, lo), hi));
  return { width: clamp(size.width, bounds.minWidth, bounds.maxWidth), height: clamp(size.height, bounds.minHeight, bounds.maxHeight) };
}

/**
 * The size a corner drag lands on: the size the gesture began at, plus the
 * pointer's travel, inside the bounds.
 * @param {{width: number, height: number}} start
 * @param {number} dx @param {number} dy
 * @param {{minWidth: number, minHeight: number, maxWidth: number, maxHeight: number}} bounds
 */
export function sizeFrom(start, dx, dy, bounds) {
  return clampSize({ width: start.width + dx, height: start.height + dy }, bounds);
}

/**
 * The placement after a resize from the bottom-right corner, so the top-left
 * corner stays where the person left it: a window anchored to the right or
 * bottom edge is measured from that edge, so growing it moves that edge
 * inward by exactly what was added.
 * @param {{h: "left" | "right", x: number, v: "top" | "bottom", y: number}} placement
 * @param {{width: number, height: number}} before
 * @param {{width: number, height: number}} after
 */
export function placementAfterResize(placement, before, after) {
  const dw = after.width - before.width;
  const dh = after.height - before.height;
  return {
    h: placement.h,
    x: placement.h === "right" ? Math.max(0, placement.x - dw) : placement.x,
    v: placement.v,
    y: placement.v === "bottom" ? Math.max(0, placement.y - dh) : placement.y,
  };
}

/**
 * Where a window opens before anyone dragged it: the manifest's corner,
 * staggered by how many windows came before it, and — bottom right — clear
 * of the notes dock and the pet, which live there.
 * @param {"top_left" | "top_right" | "bottom_left" | "bottom_right"} dock
 * @param {number} index
 * @returns {{h: "left" | "right", x: number, v: "top" | "bottom", y: number}}
 */
export function defaultPlacement(dock, index = 0) {
  const step = Math.max(0, index) * STAGGER;
  switch (dock) {
    case "top_left":
      return { h: "left", x: EDGE + step, v: "top", y: EDGE + step };
    case "top_right":
      return { h: "right", x: EDGE + step, v: "top", y: EDGE + step };
    case "bottom_left":
      return { h: "left", x: EDGE + step, v: "bottom", y: EDGE + step };
    default:
      return { h: "right", x: BOTTOM_RIGHT_CLEARANCE + step, v: "bottom", y: EDGE + step };
  }
}

/**
 * Whether a window's box overlaps a native layer that is showing — a
 * browser tab paints above every DOM element, so where the platform cannot
 * cut around the window it hides while they meet and hears it (ide/18).
 * Where the browser layer leaves a hole for it (`cutsAround`, macOS), it
 * never hides: it shows and takes its own clicks over the live page.
 * @param {{left: number, top: number}} box
 * @param {{width: number, height: number}} size
 * @param {ReadonlyArray<{visible: boolean, layer: string | null, rect: {left: number, top: number, width: number, height: number} | null}>} slots
 * @param {boolean} [cutsAround]
 */
export function hiddenByLayer(box, size, slots, cutsAround = false) {
  if (cutsAround) return false;
  return slots.some((slot) => {
    if (!slot.visible || slot.layer !== "browser" || !slot.rect) return false;
    const r = slot.rect;
    return box.left < r.left + r.width && box.left + size.width > r.left && box.top < r.top + r.height && box.top + size.height > r.top;
  });
}

/**
 * A stored preference as `{dock, size}`, each half judged on its own: a
 * corrupt dock takes the default, a corrupt size the manifest's.
 * @param {unknown} raw the stored JSON text, or null
 * @param {{width: number, height: number}} opening the manifest's size
 * @param {{h: "left" | "right", x: number, v: "top" | "bottom", y: number}} fallbackDock
 * @param {{minWidth: number, minHeight: number, maxWidth: number, maxHeight: number}} bounds
 */
export function windowPrefFrom(raw, opening, fallbackDock, bounds) {
  let parsed = null;
  if (typeof raw === "string" && raw !== "") {
    try {
      parsed = JSON.parse(raw);
    } catch {
      parsed = null;
    }
  }
  const dock = placementFrom(parsed && typeof parsed === "object" ? parsed.dock : null, fallbackDock);
  const rawSize = parsed && typeof parsed === "object" && parsed.size && typeof parsed.size === "object" ? parsed.size : null;
  const size = clampSize(rawSize && Number.isFinite(rawSize.width) && Number.isFinite(rawSize.height) ? rawSize : opening, bounds);
  return { dock, size };
}
