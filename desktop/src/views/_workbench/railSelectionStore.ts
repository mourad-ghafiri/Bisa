/**
 * What a person selected in the project rail — the fact the Board narrows
 * to (ide/16). One module store rather than the rail's own state because the
 * Board is another centre reading it. The rail publishes on a person's act
 * — a click, the keyboard's cursor — and never on the reveal that follows
 * the route, so opening the Board shows every workstream until a row is
 * picked.
 *
 * A selection is how the rail stood: it is kept under the rail's place in
 * the view memory (`shell/viewMemoryStore.ts`), quietly, and read once when
 * this store loads — so a restart opens the Board narrowed as it was left.
 * What it names may have gone while the app was closed: once the workspace
 * has said what it lists, a selection of what is not there selects nothing
 * (`keepListedSelection`).
 */

import { useSyncExternalStore } from "react";
import { viewState } from "../../shell/viewMemoryStore";
import { RAIL_PLACE } from "./idePlacesModel.mjs";
import { listedSelection, parseSelection, sameSelection } from "./railSelectionModel.mjs";
import type { RailSelection } from "./railSelectionModel.mjs";

/** The one name the selection is kept under, in the rail's place. */
const SELECTION = "selection";

interface State {
  readonly selection: RailSelection | null;
}

let state: State = { selection: parseSelection(viewState.read(RAIL_PLACE, SELECTION)) };
const listeners = new Set<() => void>();

export function useRailSelection(): RailSelection | null {
  return useSyncExternalStore(subscribe, () => state.selection, () => state.selection);
}

/** The rail's cursor landed on a row a person chose. */
export function setRailSelection(selection: RailSelection | null): void {
  if (sameSelection(state.selection, selection)) return;
  set({ selection });
}

/** *All*: the Board's door back. */
export function clearRailSelection(): void {
  setRailSelection(null);
}

/**
 * The workspace said what it lists: a selection that names a project it no
 * longer has is narrowed to what is there, or to nothing.
 */
export function keepListedSelection(listed: Iterable<string>): void {
  setRailSelection(listedSelection(state.selection, listed));
}

function set(next: State): void {
  state = next;
  viewState.keepQuietly(RAIL_PLACE, SELECTION, next.selection);
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}
