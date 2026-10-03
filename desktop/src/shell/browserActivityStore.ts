/**
 * Which browser tabs an agent is working in right now (ide/18): the bridge
 * begins a request on a tab and ends it (`browserBridge.ts`, `working`);
 * the footer reads the busy tabs to say *an agent is browsing*. A tab closed is forgotten (`BrowserPanel.tsx`). The rules
 * are `browserActivityModel.mjs`'s.
 */

import { useSyncExternalStore } from "react";
import { NO_ACTIVITY, began, busyKeys, ended, forgotten } from "./browserActivityModel.mjs";
import type { Activity } from "./browserActivityModel.mjs";

let state: Activity = NO_ACTIVITY;
let busy: readonly string[] = Object.freeze([]);
const listeners = new Set<() => void>();

function set(next: Activity): void {
  if (next === state) return;
  state = next;
  busy = Object.freeze(busyKeys(next));
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function beginBrowserWork(key: string): void {
  set(began(state, key));
}

export function endBrowserWork(key: string): void {
  set(ended(state, key));
}

export function forgetBrowserWork(key: string): void {
  set(forgotten(state, key));
}

/** The keys of the tabs an agent is working in — a stable array until it changes. */
export function useBusyBrowserTabs(): readonly string[] {
  return useSyncExternalStore(subscribe, () => busy, () => busy);
}
