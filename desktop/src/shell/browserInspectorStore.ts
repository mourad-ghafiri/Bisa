/**
 * What a browser tab's page said to the inspector (ide/18), per tab:
 * whether the wand is on, and the badges the page lost. `BrowserPanel.tsx`
 * writes it from `browser:message` through the bridge; `useBrowserAnnotation`
 * reads it and hears the notes typed in the page's own box
 * (`listenBrowserNotes`). A tab closed is forgotten.
 */

import { useSyncExternalStore } from "react";
import type { InspectorMessage, InspectorNote } from "../ui/artifact/pageInspector.mjs";

export interface TabInspector {
  readonly inspecting: boolean;
  readonly lost: readonly number[];
}

const NONE: readonly number[] = Object.freeze([]);
const EMPTY: TabInspector = Object.freeze({ inspecting: false, lost: NONE });

let state: Readonly<Record<string, TabInspector>> = {};
const listeners = new Set<() => void>();
const noteListeners = new Map<string, Set<(m: InspectorNote) => void>>();

function set(key: string, next: TabInspector): void {
  const was = state[key] ?? EMPTY;
  if (was.inspecting === next.inspecting && was.lost === next.lost) return;
  state = { ...state, [key]: next };
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useTabInspector(key: string): TabInspector {
  return useSyncExternalStore(subscribe, () => state[key] ?? EMPTY, () => EMPTY);
}

export function setBrowserInspecting(key: string, inspecting: boolean): void {
  set(key, { ...(state[key] ?? EMPTY), inspecting });
}

/** The annotations left the tray: the wand off, nothing lost any more. */
export function browserAnnotationsDone(key: string): void {
  set(key, { inspecting: false, lost: NONE });
}

/** Hear the notes typed in this tab's page; the return stops listening. */
export function listenBrowserNotes(key: string, on: (m: InspectorNote) => void): () => void {
  const ears = noteListeners.get(key) ?? new Set<(m: InspectorNote) => void>();
  ears.add(on);
  noteListeners.set(key, ears);
  return () => {
    ears.delete(on);
    if (ears.size === 0) noteListeners.delete(key);
  };
}

export function forgetBrowserInspector(key: string): void {
  noteListeners.delete(key);
  if (!(key in state)) return;
  const rest = { ...state };
  delete rest[key];
  state = rest;
  for (const l of listeners) l();
}

/** What the page said, as the inspector reads it — a note, lost badges, Escape with no box open. */
export function inspectorSaid(key: string, m: InspectorMessage): void {
  const was = state[key] ?? EMPTY;
  switch (m.type) {
    case "bisa:note":
      for (const on of noteListeners.get(key) ?? []) on(m);
      return;
    case "bisa:marked":
      set(key, { ...was, lost: m.lost });
      return;
    case "bisa:escape":
      set(key, { ...was, inspecting: false });
      return;
    default:
      return;
  }
}
