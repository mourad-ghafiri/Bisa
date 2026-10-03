/**
 * The settings registry and the resolved values — one module store, kept
 * across screens and panels, on the `sessionsStore` pattern.
 *
 * Before this store every Settings panel and every control that reads a
 * setting mounted its own read from nothing, so a panel switch drew a
 * placeholder and then the same values it had a moment ago. Here the
 * registry is read once per session and kept — it never changes — and the
 * resolved values are kept per key, the workspace's or a project's: read on
 * the first ask, read again on every `settings_changed` frame and on
 * `reloadResolved`, and committed only when a value moved
 * (`settingsSnapshotModel.settled`), so a control memoised on them
 * re-renders only then. A panel that comes back draws its controls at once;
 * the first read of the session is the only one that can show a placeholder,
 * and the kit shows it only past the beat.
 */

import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { api } from "../api";
import { subscribe as busSubscribe, watchConnection } from "../bus";
import { reloadOnReconnect } from "./workspaceLoadModel.mjs";
import type { ResolvedSetting, SettingDef } from "../types";
import { EMPTY_ENTRY, keyOf, reading, refused, settled } from "./settingsSnapshotModel.mjs";
import type { Entry } from "./settingsSnapshotModel.mjs";
import { failureReason } from "../ui/failure";

/** A read as a panel's phase logic sees it (`loadModel.phase`, `readWords`). */
export interface SettingsRead<T> {
  /** The list is the store's and never mutated by a reader; typed as a plain array for the panels' sake. */
  readonly data: { settings: T[] } | null;
  readonly error: string | null;
  readonly loading: boolean;
  readonly refreshing: boolean;
  readonly offline: false;
  readonly at: number | null;
  readonly reload: () => void;
}

let registry: Entry<SettingDef> = EMPTY_ENTRY;
let registryInFlight = false;
const resolved = new Map<string, Entry<ResolvedSetting>>();
const inFlight = new Map<string, AbortController>();
const listeners = new Set<() => void>();
let unsubscribe: (() => void) | null = null;

function notify() {
  for (const l of listeners) l();
}

function entryOf(key: string): Entry<ResolvedSetting> {
  return resolved.get(key) ?? EMPTY_ENTRY;
}

function readRegistry(): void {
  if (registryInFlight) return;
  registryInFlight = true;
  registry = reading(registry);
  notify();
  api
    .settingsRegistry()
    .then((r) => {
      registry = settled(registry, r.settings, Math.floor(Date.now() / 1000));
    })
    .catch((e: unknown) => {
      registry = refused(registry, failureReason("settings", "the settings registry could not be read", e)); // for the log
    })
    .finally(() => {
      registryInFlight = false;
      notify();
    });
}

function readResolved(key: string): void {
  inFlight.get(key)?.abort();
  const ac = new AbortController();
  inFlight.set(key, ac);
  resolved.set(key, reading(entryOf(key)));
  notify();
  api
    .settingsResolved(key === "workspace" ? null : key, ac.signal)
    .then((r) => {
      if (ac.signal.aborted) return;
      resolved.set(key, settled(entryOf(key), r.settings, Math.floor(Date.now() / 1000)));
    })
    .catch((e: unknown) => {
      if (ac.signal.aborted) return;
      resolved.set(key, refused(entryOf(key), failureReason("settings", "the resolved settings could not be read", e))); // for the log
    })
    .finally(() => {
      if (inFlight.get(key) === ac) inFlight.delete(key);
      if (!ac.signal.aborted) notify();
    });
}

/** Read the resolved values again — for `key` (the workspace's, or a project's), or every key kept. */
function reloadResolved(project?: string | null): void {
  if (project === undefined) {
    for (const key of resolved.keys()) readResolved(key);
  } else {
    readResolved(keyOf(project));
  }
}

/** Read the registry again — a fresh session, or a refused first read. */
function reloadRegistry(): void {
  readRegistry();
}

// Another surface — the CLI, an agent, a panel — changed a layer: every
// kept key is read again, so no control shows a value that is no longer true.
function ensureSubscribed(): void {
  if (unsubscribe) return;
  const unframe = busSubscribe({ stream: "engine" }, (f) => {
    if (f.stream === "engine" && f.payload.payload.type === "settings_changed") reloadResolved();
  });
  // A layer written while the node was away — the CLI, a file — came by no frame.
  const unwatch = reloadOnReconnect(watchConnection, reloadResolved);
  unsubscribe = () => {
    unframe();
    unwatch();
  };
}

function subscribe(l: () => void) {
  listeners.add(l);
  ensureSubscribed();
  return () => {
    listeners.delete(l);
  };
}

function asRead<T>(entry: Entry<T>, reload: () => void): SettingsRead<T> {
  return {
    data: entry.data === null ? null : { settings: entry.data as T[] },
    error: entry.error,
    loading: entry.loading,
    refreshing: entry.refreshing,
    offline: false,
    at: entry.at,
    reload,
  };
}

/** The registry, read once and kept. */
export function useSettingsRegistry(): SettingsRead<SettingDef> {
  const entry = useSyncExternalStore(subscribe, () => registry, () => registry);
  useEffect(() => {
    if (registry.data === null && !registryInFlight) readRegistry();
  }, []);
  return useMemo(() => asRead(entry, reloadRegistry), [entry]);
}

/** The resolved values for the workspace or a project, as a panel's read. */
export function useResolvedSettingsRead(project: string | null | undefined): SettingsRead<ResolvedSetting> {
  const key = keyOf(project);
  const entry = useSyncExternalStore(subscribe, () => entryOf(key), () => entryOf(key));
  useEffect(() => {
    if (entryOf(key).data === null && !inFlight.has(key)) readResolved(key);
  }, [key]);
  // A stable door, as `useAsync`'s `reload` is: a caller may put it in an effect's deps.
  const reload = useCallback(() => readResolved(key), [key]);
  return useMemo(() => asRead(entry, reload), [entry, reload]);
}

/**
 * The resolved settings for a project (or the workspace), kept and re-read
 * whenever a layer changes — the one way a control learns its default from a
 * setting. `RegistryPanel` edits the layers; every other screen reads through
 * here and never keeps a copy.
 */
export function useResolvedSettings(project: string | null | undefined): {
  resolved: ResolvedSetting[] | null;
  /** The first read is in flight: a control has no true value to draw yet. */
  loading: boolean;
  /** A re-read is in flight; `resolved` is the last answer. */
  refreshing: boolean;
  error: string | null;
  reload: () => void;
} {
  const read = useResolvedSettingsRead(project);
  return {
    resolved: read.data === null ? null : read.data.settings,
    loading: read.loading,
    refreshing: read.refreshing,
    error: read.error,
    reload: read.reload,
  };
}
