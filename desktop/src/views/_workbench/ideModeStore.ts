/**
 * Which mode each root of the Project IDE is in — Project (documents and
 * terminals in the centre), Agent (the conversation in the centre) or Board
 * (every workstream's card in the centre). Window furniture, in
 * `localStorage`, remembered per root and capped, the way the right panel
 * remembers its occupant (`rightPanelStore.ts`). The rules are
 * `ideModeModel.mjs`; this file only keeps.
 *
 * A root with no memory opens in the default the settings name
 * (`ide.default_mode`), which the caller passes as `fallback`, and the Board
 * is offered only while the caller says the setting has it on — the store
 * never reads settings itself, so the one reader of those keys is the screen.
 */

import { useSyncExternalStore } from "react";
import { jsonPref, readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { modeFor, nextMode, parseRememberedModes, rememberMode, type IdeMode } from "./ideModeModel.mjs";

const MODE_KEY = "bisa.ide.mode";
const MAX_REMEMBERED = 32;

interface State {
  readonly byRoot: Readonly<Record<string, IdeMode>>;
}

function read(): Readonly<Record<string, IdeMode>> {
  return readPref(webStorage(), MODE_KEY, (raw) => parseRememberedModes(jsonPref(raw)), {});
}

let state: State = { byRoot: read() };
const listeners = new Set<() => void>();

function set(next: State): void {
  if (next.byRoot === state.byRoot) return;
  state = next;
  writePref(webStorage(), MODE_KEY, next.byRoot);
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The mode a root is in now — a read for non-render code. */
export function ideModeNow(root: string, fallback: unknown, boardEnabled = true): IdeMode {
  return modeFor(state.byRoot[root], fallback, boardEnabled);
}

/** The mode a root is in, as a subscription. */
export function useIdeMode(root: string, fallback: unknown, boardEnabled = true): IdeMode {
  const remembered = useSyncExternalStore(
    subscribe,
    () => state.byRoot[root],
    () => state.byRoot[root],
  );
  return modeFor(remembered, fallback, boardEnabled);
}

export function setIdeMode(root: string, mode: IdeMode): void {
  set({ byRoot: rememberMode(state.byRoot, root, mode, MAX_REMEMBERED) });
}

/** Cycle a root to the next mode: Project, Agent, Board, round. */
export function toggleIdeMode(root: string, fallback: unknown, boardEnabled = true): IdeMode {
  const next = nextMode(ideModeNow(root, fallback, boardEnabled), boardEnabled);
  setIdeMode(root, next);
  return next;
}
