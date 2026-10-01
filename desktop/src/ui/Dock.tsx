/**
 * A thing that floats over the app and can be dragged anywhere.
 *
 * The notes dock had this inline. A second one — the pet — is the moment it
 * stops being about notes, and two of these must not be allowed to drift:
 * every rule below is a *correctness* rule rather than a styling one, and the
 * `assigneeWire` note in `views/_work/types.ts` describes exactly what happens
 * to a pair of copies where one learns something and the other does not.
 *
 * It is not the pointer loop in `SplitPane` re-abstracted. That one resizes a
 * pane along one axis against a neighbour; this one moves a free object in two.
 * They look alike for about six lines and then stop.
 *
 * # The rules it exists to hold
 *
 * - **A drag and a click are the same gesture until they are not.** Something
 *   that acted on `pointerup` would act every time you finished dragging it.
 *   The threshold is in pixels moved rather than milliseconds held, because a
 *   slow careful drag is still a drag.
 * - **Anchored to the nearest edges** (`dockModel.mjs`): a dock keeps its
 *   distance from the edges it lives near, so a maximize and a restore put it
 *   back exactly, and a dock in a corner stays in that corner.
 * - **Re-clamped on resize, for the paint only**, or a dock dragged to the
 *   edge of a wide window is unreachable once the window narrows, with no way
 *   to get it back; the stored placement is left alone so growing the window
 *   again restores it.
 * - **A drag begins from the painted box**, not the stored placement, so a
 *   dock the window has clamped does not jump the moment it is grabbed.
 * - **Kept clear of the top chrome**, which carries `data-tauri-drag-region`:
 *   a control there would move the OS window instead of itself. Its height
 *   is the theme's, read from `--spacing-chrome`.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Box, Placement, Viewport } from "./dockModel.mjs";
import { placementOf } from "./dockModel.mjs";
import { useTokenPx } from "./useTokenPx";

/** Pointer travel, in px, past which a gesture is a drag and not a click. */
const DRAG_THRESHOLD = 4;

export interface DockDrag {
  /** Put this on the draggable element. */
  onPointerDown: (e: React.PointerEvent) => void;
  /** Wrap the element's click in this; it returns false when it was a drag. */
  wasClick: () => boolean;
  dragging: boolean;
  /** How far the pointer has travelled horizontally this gesture. */
  dx: number;
}

/**
 * The size a square dock is laid out at, so the clamp and the box agree.
 *
 * 48 rather than 40: at 40 with a 16px glyph the notes dock was a low-contrast
 * channel that disappeared into a dark background, and the fix is partly size —
 * a floating control with nothing around it needs more presence than a button
 * in a toolbar, where its neighbours do the work of saying where it is.
 *
 * Anything that clamps against this must be *told* it — the model's own
 * default is a fallback, not a second copy of this number.
 */
export const DOCK_SIZE = 48;

/**
 * The chrome's height for the frame before `--spacing-chrome` has resolved —
 * the token's value at scale 1, the same doctrine `useTokenPx` documents.
 * Nothing is stored on that frame, so at worst a top-anchored dock paints a
 * few px off once.
 */
export const CHROME_FALLBACK = 40;

/**
 * The window a dock is placed in: its size, live across a resize; the chrome
 * it must stay under; and the dock's own extent. The one place a viewport is
 * built, so the paint, the drag and the clamp all measure the same window.
 */
export function useDockViewport(size: number | { width: number; height: number }): Viewport {
  // A resize re-renders the consumer so its paint is re-clamped against the
  // new window — without touching what it has stored.
  const [, onResizeTick] = useState(0);
  useEffect(() => {
    const onResize = () => onResizeTick((n) => n + 1);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  const top = useTokenPx("--spacing-chrome", CHROME_FALLBACK);
  const width = typeof window === "undefined" ? 0 : window.innerWidth;
  const height = typeof window === "undefined" ? 0 : window.innerHeight;
  const { width: sizeX, height: sizeY } = typeof size === "number" ? { width: size, height: size } : size;
  return useMemo(() => ({ width, height, sizeX, sizeY, top }), [width, height, sizeX, sizeY, top]);
}

/**
 * Drag a dock from where it is painted.
 *
 * `box` is the painted, clamped box (`dockBox`), and the gesture begins there;
 * `onMove` receives the placement of the box the pointer moved to — clamped
 * and anchored by the model — so what a caller stores can never strand the
 * dock and never jumps from where the person grabbed it.
 */
export function useDockDrag(box: Box, viewport: Viewport, onMove: (next: Placement) => void): DockDrag {
  const [dragging, setDragging] = useState(false);
  const [dx, setDx] = useState(0);
  /** Where the pointer and the box were when this gesture began. */
  const start = useRef({ px: 0, py: 0, left: 0, top: 0, moved: false });
  const move = useRef(onMove);
  move.current = onMove;
  // Refreshed every render: a resize mid-drag re-renders the caller, so the
  // window the pointer is measured against is never stale.
  const vp = useRef(viewport);
  vp.current = viewport;

  const onPointerMove = useCallback((e: PointerEvent) => {
    const ddx = e.clientX - start.current.px;
    const ddy = e.clientY - start.current.py;
    if (Math.abs(ddx) > DRAG_THRESHOLD || Math.abs(ddy) > DRAG_THRESHOLD) {
      start.current.moved = true;
    }
    setDx(ddx);
    move.current(placementOf({ left: start.current.left + ddx, top: start.current.top + ddy }, vp.current));
  }, []);

  useEffect(() => {
    if (!dragging) return;
    const up = () => {
      setDragging(false);
      setDx(0);
    };
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", up);
    // A pointer the system took back — a gesture, a window switch — ends the
    // drag too, or the box would follow the next pointer forever.
    window.addEventListener("pointercancel", up);
    const prev = document.body.style.cursor;
    document.body.style.cursor = "grabbing";
    document.body.style.userSelect = "none";
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      document.body.style.cursor = prev;
      document.body.style.userSelect = "";
    };
  }, [dragging, onPointerMove]);

  return {
    dragging,
    dx,
    onPointerDown: (e) => {
      // Left button only: a right-click here is the platform's menu.
      if (e.button !== 0) return;
      e.preventDefault();
      start.current = { px: e.clientX, py: e.clientY, left: box.left, top: box.top, moved: false };
      setDragging(true);
    },
    wasClick: () => {
      if (start.current.moved) {
        start.current.moved = false;
        return false;
      }
      return true;
    },
  };
}

export { dockBox, placementFrom, placementOf, samePlacement, type Box, type Placement, type Viewport } from "./dockModel.mjs";

/**
 * Where a dock is painted, and how big it is.
 *
 * `size` takes a pair as well as a number, because **the box has to be the
 * thing inside it**. A 40×40 button around a 96×104 pet left most of the pet
 * outside its own hit area — it could not be dragged except by one corner, and
 * the clamp that keeps it on screen was measuring the wrong rectangle.
 */
export function dockStyle(
  box: Box,
  size: number | { width: number; height: number } = DOCK_SIZE,
): React.CSSProperties {
  const { width, height } = typeof size === "number" ? { width: size, height: size } : size;
  return { left: box.left, top: box.top, width, height };
}
