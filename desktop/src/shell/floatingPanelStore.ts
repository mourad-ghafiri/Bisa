/**
 * The floating panels' widths, as each says it while it floats (the Notes
 * panel; 19 — Drawings): what another floating panel reads to stand beside
 * it rather than on it (`draw/besideModel.mjs`). Window furniture of the
 * moment — never stored; a panel that shuts, maximizes or unmounts says
 * `null`.
 */

import { useEffect, useSyncExternalStore } from "react";

const widths = new Map<string, number>();
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** A panel's word: its width while it floats, `null` when it does not. */
function publishFloatingPanel(id: string, width: number | null): void {
  if (width === null ? !widths.has(id) : widths.get(id) === width) return;
  if (width === null) widths.delete(id);
  else widths.set(id, width);
  for (const l of listeners) l();
}

/** Say the width while `width` is a number, and nothing once the panel goes. */
export function usePublishFloatingPanel(id: string, width: number | null): void {
  useEffect(() => publishFloatingPanel(id, width), [id, width]);
  useEffect(() => () => publishFloatingPanel(id, null), [id]);
}

/** Another panel's width while it floats, `null` when it does not. */
export function useFloatingPanelWidth(id: string): number | null {
  return useSyncExternalStore(subscribe, () => widths.get(id) ?? null, () => null);
}

/** The window's width, read again as it resizes. */
export function useViewportWidth(): number {
  return useSyncExternalStore(
    (l) => {
      window.addEventListener("resize", l);
      return () => window.removeEventListener("resize", l);
    },
    () => window.innerWidth,
    () => 0,
  );
}
