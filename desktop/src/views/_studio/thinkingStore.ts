/**
 * How the agents' thinking is shown above their replies — *auto*, *shown*
 * or *hidden* — the person's choice for every timeline, in `localStorage`
 * under `bisa.chat.thinking`, auto before a choice (13 — Conversations §The
 * reply streams). One block can still be opened or closed on its own; this
 * is the rule every block follows. The words are `liveTurnModel.mjs`'s.
 */

import { readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { useSyncExternalStore } from "react";
import { thinkingMode } from "./liveTurnModel.mjs";
import type { ThinkingMode } from "./liveTurnModel.mjs";

const KEY = "bisa.chat.thinking";

function read(): ThinkingMode {
  return readPref(webStorage(), KEY, thinkingMode, thinkingMode(null));
}

let mode: ThinkingMode = read();
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The mode every thinking block follows, as a subscription. */
export function useThinkingMode(): ThinkingMode {
  return useSyncExternalStore(subscribe, () => mode, () => mode);
}

/** The mode chosen in the timeline's control — remembered, and every block follows. */
export function setThinkingMode(next: ThinkingMode): void {
  const chosen = thinkingMode(next);
  if (chosen === mode) return;
  mode = chosen;
  // If storage is denied, the choice still holds this session.
  writePref(webStorage(), KEY, chosen);
  for (const l of listeners) l();
}
