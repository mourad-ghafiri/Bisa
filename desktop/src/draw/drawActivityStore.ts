/**
 * Which drawings an agent is drawing into right now (19 — Drawings): the
 * bridge begins a request on a drawing and ends it; the dock and the footer's
 * Draw switch wear a working dot while any is busy. The counting rules are
 * the browser activity model's — they are about keys, not tabs.
 */

import { useSyncExternalStore } from "react";
import { NO_ACTIVITY, began, busyKeys, ended } from "../shell/browserActivityModel.mjs";
import type { Activity } from "../shell/browserActivityModel.mjs";

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

export function beginDrawWork(drawing: string): void {
  set(began(state, drawing));
}

export function endDrawWork(drawing: string): void {
  set(ended(state, drawing));
}

/** The ids of the drawings an agent is drawing into — a stable array until it changes. */
export function useBusyDrawings(): readonly string[] {
  return useSyncExternalStore(subscribe, () => busy, () => busy);
}
