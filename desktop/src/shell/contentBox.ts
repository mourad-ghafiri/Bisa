/**
 * The content column's box — the room right of the sidebar, between the
 * header and the footer — as one module store `App.tsx` publishes from the
 * resize observer it already keeps on that column (19 — Drawings). A panel
 * that maximizes fills exactly this box: not `layerSlots`, whose slots carry
 * a native layer's semantics, and not a DOM query from the panel, which would
 * miss a move that did not resize. Notifies only on a change (`sameRect`).
 */

import { useSyncExternalStore } from "react";
import { roundRect, sameRect } from "./centerSlotModel.mjs";
import type { Rect } from "./centerSlotModel.mjs";

let box: Rect | null = null;
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The content column's box, in viewport pixels; `null` before it is measured. */
export function publishContentBox(rect: { left: number; top: number; width: number; height: number } | null): void {
  const next = roundRect(rect);
  if (sameRect(next, box)) return;
  box = next;
  for (const l of listeners) l();
}

export function useContentBox(): Rect | null {
  return useSyncExternalStore(subscribe, () => box, () => box);
}
