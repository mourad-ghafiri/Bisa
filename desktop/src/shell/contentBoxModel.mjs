/**
 * Where the content column stands (19 — Drawings): the room right of the
 * sidebar and between the header and the footer, as `App.tsx` measures it.
 * A panel that **maximizes** fills exactly that box — never the chrome — and
 * the rules of how are here, testable without a window:
 *
 * - {@link maximizedStyle}: the fixed box, whole pixels.
 * - {@link restoreOnEscape}: Escape restores only when the canvas has nothing
 *   of its own to cancel — an element being written, a line being drawn, a
 *   selection, an open menu — since the canvas takes Escape for those first.
 */

import { roundRect } from "./centerSlotModel.mjs";

/** @typedef {{left: number, top: number, width: number, height: number}} Rect */

/**
 * The style a maximized panel wears: fixed at the content box.
 * @param {Rect | null | undefined} box
 * @returns {{position: "fixed", left: number, top: number, width: number, height: number} | null}
 */
export function maximizedStyle(box) {
  const r = roundRect(box);
  if (!r || r.width <= 0 || r.height <= 0) return null;
  return { position: "fixed", left: r.left, top: r.top, width: r.width, height: r.height };
}

/**
 * Whether Escape means *restore the panel* rather than something the canvas
 * is in the middle of. The canvas's own Escape ends a text being written,
 * a line being drawn, a selection or an open menu; only when none of those
 * stands does the key reach the panel.
 * @param {{editingTextElement?: unknown, editingLinearElement?: unknown, selectedElementIds?: Record<string, boolean> | null, openMenu?: unknown, openDialog?: unknown, openSidebar?: unknown} | null | undefined} appState
 */
export function restoreOnEscape(appState) {
  if (!appState) return true;
  if (appState.editingTextElement || appState.editingLinearElement) return false;
  if (appState.openMenu || appState.openDialog || appState.openSidebar) return false;
  const selected = appState.selectedElementIds ?? {};
  return !Object.values(selected).some(Boolean);
}
