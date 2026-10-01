/**
 * The Board's window furniture: which columns are folded and whether
 * Archived shows — per viewer, in `localStorage`, the way the right panel
 * keeps its occupant. What the Board *shows* comes from the node, and which
 * projects it is narrowed to is the rail's selection (`railSelectionStore`);
 * this is only how you look at it.
 */

import { useSyncExternalStore } from "react";
import { jsonPref, readPref, switchWord, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { COLUMNS, type Column } from "./boardModel.mjs";

const COLLAPSED_KEY = "bisa.board.collapsed";
const ARCHIVED_KEY = "bisa.board.archived";

interface State {
  readonly collapsed: readonly Column[];
  /** `null` until a person chose; the setting decides then. */
  readonly archived: boolean | null;
}

function readFlag(key: string): boolean | null {
  return readPref(webStorage(), key, (raw) => raw === "1", null);
}

function readCollapsed(): Column[] {
  return readPref(
    webStorage(),
    COLLAPSED_KEY,
    (raw) => {
      const list = jsonPref(raw);
      return Array.isArray(list) ? list.filter((c): c is Column => typeof c === "string" && (COLUMNS as readonly string[]).includes(c)) : [];
    },
    [],
  );
}

let state: State = {
  collapsed: readCollapsed(),
  archived: readFlag(ARCHIVED_KEY),
};
const listeners = new Set<() => void>();

function set(next: State): void {
  state = next;
  const storage = webStorage();
  writePref(storage, COLLAPSED_KEY, next.collapsed);
  writePref(storage, ARCHIVED_KEY, next.archived === null ? null : switchWord(next.archived));
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useBoardView(): State {
  return useSyncExternalStore(subscribe, () => state, () => state);
}

export function toggleColumnFolded(column: Column): void {
  const folded = state.collapsed.includes(column);
  set({ ...state, collapsed: folded ? state.collapsed.filter((c) => c !== column) : [...state.collapsed, column] });
}

export function setBoardArchived(on: boolean): void {
  if (state.archived === on) return;
  set({ ...state, archived: on });
}
