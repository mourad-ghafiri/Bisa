/**
 * The project rail (the workbench's left column of projects and their
 * workstreams): whether it is showing. Window furniture, in `localStorage`,
 * the twin of `rightPanelStore.ts`'s open flag — one module store rather
 * than component state because the keymap toggles it from anywhere in a
 * root (`⌘B`) and the top bar's button does the same.
 */

import { useSyncExternalStore } from "react";
import { readPref, switchWord, webStorage, writePref } from "../../shell/storedPrefModel.mjs";

interface State {
  /** Whether the rail's column is showing. */
  readonly open: boolean;
}

const OPEN_KEY = "bisa.ide.rail.open";

let state: State = {
  open: readPref(webStorage(), OPEN_KEY, (raw) => raw !== "0", true),
};
const listeners = new Set<() => void>();

export function useProjectRail(): State {
  return useSyncExternalStore(subscribe, () => state, () => state);
}

export function setProjectRailOpen(open: boolean): void {
  if (open === state.open) return;
  set({ open });
}

export function toggleProjectRail(): void {
  set({ open: !state.open });
}

function set(next: State): void {
  state = next;
  writePref(webStorage(), OPEN_KEY, switchWord(next.open));
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}
