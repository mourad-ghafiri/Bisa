/**
 * What the footer says about the editor you are in: the caret's line and
 * column and the document's language. Each mounted editor publishes its own;
 * the footer reads the active one's (`editorRegistry`'s notion of active — the
 * editor that last took focus). A module store, like the other shell stores.
 */

import { useSyncExternalStore } from "react";

export interface EditorStatus {
  path: string;
  line: number;
  column: number;
  /** The language id, or null when nothing knows it. */
  language: string | null;
}

let statuses: ReadonlyMap<string, EditorStatus> = new Map();
let activeKey: string | null = null;
const listeners = new Set<() => void>();

function notify() {
  for (const l of listeners) l();
}

/** An editor's caret moved, or it learned its language. */
export function publishEditorStatus(key: string, status: EditorStatus): void {
  const cur = statuses.get(key);
  if (cur && cur.path === status.path && cur.line === status.line && cur.column === status.column && cur.language === status.language) return;
  const next = new Map(statuses);
  next.set(key, status);
  statuses = next;
  notify();
}

/** The editor the person is in; the footer follows it. */
export function setStatusEditor(key: string | null): void {
  if (activeKey === key) return;
  activeKey = key;
  notify();
}

/** An editor unmounted: its status goes, and the footer goes quiet if it was the one shown. */
export function forgetEditorStatus(key: string): void {
  if (!statuses.has(key)) return;
  const next = new Map(statuses);
  next.delete(key);
  statuses = next;
  if (activeKey === key) activeKey = null;
  notify();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

const snapshot = () => (activeKey ? (statuses.get(activeKey) ?? null) : null);

/** The active editor's status, or null when no editor is up. */
export function useEditorStatus(): EditorStatus | null {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
