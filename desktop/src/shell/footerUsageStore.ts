/**
 * Which harness's usage the footer shows — the person's pin, window
 * furniture in `localStorage`, the way the IDE remembers a root's mode
 * (`ideModeStore.ts`). The rule that turns the pin into the harness shown
 * — still installed, else the first — is `footerUsageModel.pinnedHarness`;
 * this file only keeps.
 */

import { useSyncExternalStore } from "react";
import { readPref, webStorage, writePref } from "./storedPrefModel.mjs";

const PIN_KEY = "bisa.footer.usage";

function read(): string | null {
  return readPref(webStorage(), PIN_KEY, (raw) => (raw.trim() ? raw : null), null);
}

let pinned: string | null = read();
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The pinned harness id, as a subscription; null when the person never chose. */
export function usePinnedUsage(): string | null {
  return useSyncExternalStore(subscribe, () => pinned, () => pinned);
}

/** Pin a harness: the footer shows this one from now on. */
export function pinUsage(id: string): void {
  if (id === pinned) return;
  pinned = id;
  writePref(webStorage(), PIN_KEY, id);
  for (const l of listeners) l();
}
