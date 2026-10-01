/**
 * The explorer's clipboard, held for the app: a cut in one Files panel pastes
 * in the same root's panel in another window of the workbench. Module state
 * behind `useSyncExternalStore`; the facts are `fileClipboard.mjs`.
 */

import { useSyncExternalStore } from "react";
import type { Clip } from "./fileClipboard.mjs";

let clip: Clip | null = null;
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useFileClipboard(): Clip | null {
  return useSyncExternalStore(subscribe, () => clip, () => clip);
}

export function setFileClipboard(next: Clip | null): void {
  clip = next;
  for (const l of listeners) l();
}
