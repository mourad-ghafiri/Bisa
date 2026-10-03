/**
 * The project rail (the workbench's left column of projects and their
 * workstreams): whether it is showing. Window furniture, in `localStorage`,
 * the twin of `rightPanelStore.ts`'s open flag — one module store rather
 * than component state because the keymap toggles it from anywhere in a
 * root (`⌘B`) and the top bar's button does the same.
 *
 * Open is the person's switch; **folded** is the window's fact — open, and
 * no room for it beside the centre and the right panel (`ideColumnsModel`),
 * which the workbench says as it measures. Never stored. Asking for a folded
 * rail makes its room: the right panel shuts, the last ask winning.
 */

import { useSyncExternalStore } from "react";
import { readPref, switchWord, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { setRightPanelOpen } from "./rightPanelStore";

interface State {
  /** Whether the rail's column is showing — the person's switch. */
  readonly open: boolean;
  /** Open, and no room for it at this window's width: drawn shut until there is. */
  readonly folded: boolean;
}

const OPEN_KEY = "bisa.ide.rail.open";

let state: State = {
  open: readPref(webStorage(), OPEN_KEY, (raw) => raw !== "0", true),
  folded: false,
};
const listeners = new Set<() => void>();

export function useProjectRail(): State {
  return useSyncExternalStore(subscribe, () => state, () => state);
}

export function setProjectRailOpen(open: boolean): void {
  // Asked for while folded: the room is made, the right panel shutting.
  if (open && state.open && state.folded) {
    setRightPanelOpen(false);
    return;
  }
  if (open === state.open) return;
  set({ ...state, open });
}

/** The chord's toggle: what shows is what flips — a folded rail is shown, never shut unseen. */
export function toggleProjectRail(): void {
  setProjectRailOpen(!state.open || state.folded);
}

/** The workbench's word as it measures: whether the open rail has room at this width. */
export function setProjectRailFolded(folded: boolean): void {
  if (folded === state.folded) return;
  state = { ...state, folded };
  for (const l of listeners) l();
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
