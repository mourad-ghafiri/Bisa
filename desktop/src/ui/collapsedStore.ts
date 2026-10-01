/**
 * What is folded, per viewer — one store for every collapsible in the app,
 * persisted under `bisa.collapsed.<id>`. One store rather than one
 * `useState` per component, because the same thing folds in two places: a
 * harness's sub-agents fold under it on the project rail *and* on the
 * Workstreams panel, and a chevron pressed in one must move the other, in
 * the same frame, not on the next visit.
 */

import { useCallback, useMemo, useSyncExternalStore } from "react";
import { readPref, switchWord, webStorage, writePref } from "../shell/storedPrefModel.mjs";

const PREFIX = "bisa.collapsed.";

/** id → collapsed, seeded once from what the last session left behind. */
const folded = new Map<string, boolean>();
const listeners = new Set<() => void>();
/** Bumped on every change; prefix snapshots are cached against it. */
let version = 0;

(function seed() {
  const storage = webStorage();
  if (!storage) return;
  // Every key under the prefix rather than one named key — the enumeration
  // the model has no door for, so this is the one place a store is walked.
  try {
    for (let i = 0; i < storage.length; i++) {
      const k = storage.key(i);
      if (k?.startsWith(PREFIX)) folded.set(k.slice(PREFIX.length), readPref(storage, k, (raw) => raw === "1", false));
    }
  } catch {
    // A store that cannot be walked starts every collapsible open.
  }
})();

function write(id: string, collapsed: boolean): void {
  folded.set(id, collapsed);
  writePref(webStorage(), `${PREFIX}${id}`, switchWord(collapsed));
}

function notify(): void {
  version += 1;
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function isCollapsed(id: string, initial = false): boolean {
  return folded.get(id) ?? initial;
}

export function setCollapsed(id: string, collapsed: boolean): void {
  if (folded.get(id) === collapsed) return;
  write(id, collapsed);
  notify();
}

export function toggleCollapsed(id: string, initial = false): void {
  setCollapsed(id, !isCollapsed(id, initial));
}

/** Open every one of these that is folded — a reveal, never a toggle. */
export function expandAll(ids: readonly string[]): void {
  let changed = false;
  for (const id of ids) {
    if (folded.get(id)) {
      write(id, false);
      changed = true;
    }
  }
  if (changed) notify();
}

/** One collapsible: whether it is folded, and a toggle. */
export function useCollapsed(id: string, initial = false): [boolean, () => void] {
  const collapsed = useSyncExternalStore(
    subscribe,
    () => isCollapsed(id, initial),
    () => initial,
  );
  const toggle = useCallback(() => toggleCollapsed(id, initial), [id, initial]);
  return [collapsed, toggle];
}

/**
 * Every folded id under a prefix, as a Set — what a tree that folds many
 * rows reads. The Set is rebuilt only when something changed, so a caller
 * may hold it in a memo.
 */
export function useCollapsedUnder(prefix: string): Set<string> {
  const v = useSyncExternalStore(
    subscribe,
    () => version,
    () => 0,
  );
  return useMemo(() => {
    const out = new Set<string>();
    for (const [id, c] of folded) if (c && id.startsWith(prefix)) out.add(id);
    return out;
    // `v` is the change signal; the map it reads is module state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefix, v]);
}
