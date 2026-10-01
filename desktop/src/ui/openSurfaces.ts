/**
 * How many surfaces are open right now — dialogs, popovers, menus, context
 * menus, the link card, the artifact stage, the palette — so a native layer
 * drawn over the page (the browser tab's webview, `shell/BrowserPanel.tsx`)
 * can yield to them: a native view paints above every DOM element, so a
 * dialog opened over it would be hidden by it. Each surface says when it
 * opens and leaves (`useSurface`, or `useOpenSurface` for one that owns its
 * open state); the layer asks whether it may show (`useLayerMayShow`), and
 * whoever waits for the layer to show listens (`subscribeSurfaces`). A
 * tooltip is not a surface: hover-driven and `pointer-events-none`,
 * counting it would blank the page on every hover. The count is
 * `surfacesModel.mjs`'s.
 */

import { useEffect, useState, useSyncExternalStore } from "react";
import { layerMayShow, surfaces } from "./surfacesModel.mjs";

let count = 0;
const listeners = new Set<() => void>();

function set(next: number): void {
  if (next === count) return;
  count = next;
  for (const l of listeners) l();
}

/** Hear the count change — what a wait for the layer to show listens to. */
export function subscribeSurfaces(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** A surface opened. */
function surfaceOpened(): void {
  set(surfaces(count, 1));
}

/** A surface left. */
function surfaceClosed(): void {
  set(surfaces(count, -1));
}

/** A surface's own effect: counted while `open`. */
export function useSurface(open: boolean): void {
  useEffect(() => {
    if (!open) return;
    surfaceOpened();
    return () => surfaceClosed();
  }, [open]);
}

/**
 * The open state a surface owns — a menu, a context menu — counted while
 * open: hand the setter to the component's `onOpenChange`.
 */
export function useOpenSurface(): [boolean, (open: boolean) => void] {
  const [open, setOpen] = useState(false);
  useSurface(open);
  return [open, setOpen];
}

/** Whether a native layer may show: no surface is open. */
export function useLayerMayShow(): boolean {
  return useSyncExternalStore(subscribeSurfaces, () => layerMayShow(count), () => true);
}

/** The count, for a test or a log. */
export function openSurfaces(): number {
  return count;
}

/** Whether a native layer may show right now, outside React. */
export function layerMayShowNow(): boolean {
  return layerMayShow(count);
}
