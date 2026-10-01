/**
 * Which view the IDE's Agent pane shows per root — the conversation it is
 * on, or the list of the checkout's conversations (ide/09 §A checkout is on
 * a conversation). Window furniture, in `localStorage` under
 * `bisa.ide.agents.view`, remembered per root and capped the way the mode is
 * (`ideModeStore.ts`): a person who opened the list and reloaded finds it
 * open; picking a row closes it. The `root` strategy of
 * `_studio/conversationSelection.ts` reads and writes it; the rules are
 * `conversationPaneModel.mjs`'s (`parseRemembered`, `remember`).
 */

import { useSyncExternalStore } from "react";
import { jsonPref, readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { LIST_VIEW, VIEW_KEY, parseRemembered, remember } from "./conversationPaneModel.mjs";

interface State {
  readonly byRoot: Readonly<Record<string, string>>;
}

function read(): Readonly<Record<string, string>> {
  return readPref(webStorage(), VIEW_KEY, (raw) => parseRemembered(jsonPref(raw)), {});
}

let state: State = { byRoot: read() };
const listeners = new Set<() => void>();

function set(next: State): void {
  if (next.byRoot === state.byRoot) return;
  state = next;
  writePref(webStorage(), VIEW_KEY, next.byRoot);
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** Whether a root's pane shows the list, as a subscription. */
export function useListOpen(root: string): boolean {
  const remembered = useSyncExternalStore(
    subscribe,
    () => state.byRoot[root],
    () => state.byRoot[root],
  );
  return remembered === LIST_VIEW;
}

/** Show the list for a root, or leave it (`false`) for the conversation. */
export function setListOpen(root: string, open: boolean): void {
  const next = remember(state.byRoot, root, open ? LIST_VIEW : null);
  if (next !== state.byRoot) set({ byRoot: next });
}
