/**
 * The open documents' buffers, kept outside the components that draw them
 * (ide/03 §Tabs).
 *
 * A pane draws one tab at a time, so an editor that is not on screen is not
 * mounted. While the buffer was the editor's own state, a document that left
 * the screen took its unsaved text with it — and the fact that it *had*
 * unsaved text — so a tab could be closed, or the app quit, with no question
 * and the work gone. Here the buffer is the **tab's**: it lives from the
 * first keystroke until the tab closes, whatever is mounted. The editor reads
 * and writes it through {@link useDocBuffer}; the tab's dot, the close guard
 * and the quit question read {@link useUnsavedKeys} and {@link unsavedNow}.
 *
 * A buffer is forgotten when its tab closes — the workbench's close paths
 * call {@link forgetBuffers} — and never on unmount. Keys are the registry's
 * (`editorRegistry.editorKey`: `<root>|<tab id>`), so one document open in
 * two roots is two buffers, as it is two tabs. What is unsaved, and what a
 * tab coming back on screen keeps, are `editorModel.mjs`'s rules.
 */

import { useCallback, useMemo, useSyncExternalStore } from "react";
import { unsavedKeys } from "./editorModel.mjs";
import type { Buffer } from "./editorModel.mjs";

let buffers: ReadonlyMap<string, Buffer> = new Map();
const listeners = new Set<() => void>();

function set(next: ReadonlyMap<string, Buffer>): void {
  buffers = next;
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The buffer a tab left behind, if it has one. */
export function bufferOf(key: string): Buffer | null {
  return buffers.get(key) ?? null;
}

/**
 * A document's buffer and its setter — `useState`'s shape, with the state in
 * the store. `initial` is what the buffer starts from while the store holds
 * none for the key; it is written on the first change, so a document nobody
 * touched leaves nothing behind.
 */
export function useDocBuffer(key: string, initial: () => Buffer): [Buffer, (next: Buffer | ((current: Buffer) => Buffer)) => void] {
  // One starting buffer per key: the snapshot below must not change identity between renders.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const start = useMemo(initial, [key]);
  const read = useCallback(() => buffers.get(key) ?? start, [key, start]);
  const buffer = useSyncExternalStore(subscribe, read, read);
  const setBuffer = useCallback(
    (next: Buffer | ((current: Buffer) => Buffer)) => {
      const current = buffers.get(key) ?? start;
      const value = typeof next === "function" ? next(current) : next;
      if (value === current && buffers.has(key)) return;
      const map = new Map(buffers);
      map.set(key, value);
      set(map);
    },
    [key, start],
  );
  return [buffer, setBuffer];
}

/**
 * Files renamed on disk: each buffer follows its tab to the new key, its
 * unsaved text intact, under the new path.
 */
export function moveBuffers(moves: readonly { from: string; to: string; path: string }[]): void {
  const held = moves.filter((m) => buffers.has(m.from));
  if (held.length === 0) return;
  const map = new Map(buffers);
  for (const { from, to, path } of held) {
    const buffer = map.get(from);
    if (!buffer) continue;
    map.delete(from);
    map.set(to, { ...buffer, path });
  }
  set(map);
}

/** The tabs closed: their buffers go with them. */
export function forgetBuffers(keys: readonly string[]): void {
  if (!keys.some((k) => buffers.has(k))) return;
  const map = new Map(buffers);
  for (const k of keys) map.delete(k);
  set(map);
}

/**
 * The keys holding unsaved work, computed once per map: every subscriber gets
 * the same `Set` while no buffer changed, so it can sit in an effect's
 * dependencies without running it on every keystroke elsewhere.
 */
let unsavedCache: { map: ReadonlyMap<string, Buffer>; keys: ReadonlySet<string> } | null = null;
function unsavedOf(map: ReadonlyMap<string, Buffer>): ReadonlySet<string> {
  if (unsavedCache?.map === map) return unsavedCache.keys;
  // The same members as before is the same set: a keystroke in a dirty document re-renders nobody.
  const keys = unsavedKeys(map);
  const before = unsavedCache?.keys;
  const same = before !== undefined && before.size === keys.length && keys.every((k) => before.has(k));
  unsavedCache = { map, keys: same ? before : new Set(keys) };
  return unsavedCache.keys;
}

/**
 * The snapshot is the *set*, not the map: its identity moves only when a
 * document becomes unsaved or stops being, so a subscriber — the workbench —
 * is not re-rendered by a keystroke.
 */
const unsavedSnapshot = () => unsavedOf(buffers);

/** The keys of the documents holding unsaved work — on screen or not. */
export function useUnsavedKeys(): ReadonlySet<string> {
  return useSyncExternalStore(subscribe, unsavedSnapshot, unsavedSnapshot);
}

/** The same, read once — for a flow that is not a component (the quit guard). */
export function unsavedNow(): string[] {
  return unsavedKeys(buffers);
}
