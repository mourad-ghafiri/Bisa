/**
 * Where the terminal layer is drawn (layout): over the rect the
 * workbench's centre reports, or nowhere.
 *
 * The panel stays mounted in `App.tsx` for the reason `TerminalPanel.tsx`
 * gives — unmounting ends shells — so it cannot *be* inside the workbench.
 * It is positioned over it instead. Two rules, both testable:
 *
 * - {@link sameRect}: the store notifies only on a change, because a notify
 *   re-renders every terminal and a `ResizeObserver` ticks more often than
 *   the numbers move.
 * - {@link slotStyle}: a hidden layer keeps its last non-zero box and hides
 *   with `visibility`, never with a zero size or `display: none` — xterm
 *   measures its cells from a real layout box, once.
 */

/** @typedef {{left: number, top: number, width: number, height: number}} Rect */

/**
 * @param {Rect | null | undefined} a
 * @param {Rect | null | undefined} b
 */
export function sameRect(a, b) {
  if (a === b) return true;
  if (!a || !b) return false;
  return a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height;
}

/** A rect with whole pixels, so equal layouts compare equal. */
export function roundRect(r) {
  if (!r) return null;
  return {
    left: Math.round(r.left),
    top: Math.round(r.top),
    width: Math.round(r.width),
    height: Math.round(r.height),
  };
}

/**
 * The inline style for the layer.
 * @param {Rect | null | undefined} rect
 * @param {boolean} visible
 */
export function slotStyle(rect, visible) {
  const box = rect && rect.width > 0 && rect.height > 0 ? rect : { left: 0, top: 0, width: 640, height: 320 };
  return {
    position: "fixed",
    left: box.left,
    top: box.top,
    width: box.width,
    height: box.height,
    visibility: visible && rect && rect.width > 0 && rect.height > 0 ? "visible" : "hidden",
    pointerEvents: visible ? "auto" : "none",
  };
}
