/**
 * One checkout's changed files, read once for everyone who shows them
 * (ide/03 §The Files occupant, ide/04 §The Changes view): the Git panel's
 * list and the explorer's standings are one `GET /git/files` per root, not
 * one each — a module store on the `workstreamStatusStore` pattern, keyed
 * by the root (`rootKey("workstream", wid)`).
 *
 * Read when the first subscriber arrives, again once per burst of
 * `file_changed` frames for that root that touch the tree or the index
 * (`gitChangeModel.readsFor` — a fetch that only moved refs re-lists
 * nothing; coalesced over `GIT_COALESCE_MS`), after the frames that
 * mean something wrote into the tree without a frame of its own
 * (`execution_ended`, `result_accepted`), and on `refreshGitFiles` — the
 * git session's `stale` bump, a Refresh button. A write that answered with
 * rows commits them straight in (`landedGitFiles`), so the panel and the
 * tree agree the moment a hunk is staged. A root nobody subscribes to is
 * not read, and past `MAX_IDLE_ROOTS` of them the oldest let go of their
 * rows: a session that visits many checkouts keeps the few it left last, not
 * every listing it ever read. A node that was away and came back is read
 * again for every root shown (`reloadOnReconnect`): what changed in the
 * tree while it was away was said by no frame. The shape is `useAsync`'s —
 * `data`, `loading`, `error`, `reload` — so a reader that had its own read
 * swaps without a rewrite.
 */

import { useEffect, useSyncExternalStore } from "react";
import { api } from "../../api";
import { subscribe as busSubscribe, watchConnection } from "../../bus";
import { reloadOnReconnect } from "../../shell/workspaceLoadModel.mjs";
import type { GitFileRow, WorkstreamGitFiles } from "../../types";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { GIT_COALESCE_MS, kindsOf, readsFor } from "./gitChangeModel.mjs";
import { failureText } from "../../ui";

/** The frames after which every shown root is read again: something may have written into the tree. */
const WRITE_FRAMES = new Set(["execution_ended", "result_accepted", "workstream_committed"]);
/** How many roots nobody shows keep their last rows, for the return; the oldest past it forget theirs. */
const MAX_IDLE_ROOTS = 8;

export interface GitFilesRead {
  readonly data: WorkstreamGitFiles | null;
  readonly loading: boolean;
  /** A re-read is in flight and `data` is the last answer — `useAsync`'s word. */
  readonly refreshing: boolean;
  readonly error: string | null;
  readonly reload: () => void;
}

interface Entry {
  data: WorkstreamGitFiles | null;
  loading: boolean;
  error: string | null;
  inFlight: boolean;
  again: boolean;
  flush: ReturnType<typeof setTimeout> | null;
  subscribers: number;
  /** The read handed to React, stable while nothing changed. */
  snapshot: GitFilesRead;
}

const entries = new Map<string, Entry>();
const listeners = new Set<() => void>();
let unsubscribeBus: (() => void) | null = null;
let unwatchConnection: (() => void) | null = null;

const EMPTY_READ = (scope: string): GitFilesRead => ({ data: null, loading: true, refreshing: false, error: null, reload: () => void read(scope) });

function entryFor(scope: string): Entry {
  let e = entries.get(scope);
  if (!e) {
    e = { data: null, loading: false, error: null, inFlight: false, again: false, flush: null, subscribers: 0, snapshot: EMPTY_READ(scope) };
    entries.set(scope, e);
  }
  return e;
}

function publish(scope: string): void {
  const e = entryFor(scope);
  e.snapshot = { data: e.data, loading: e.loading, refreshing: e.inFlight && e.data !== null, error: e.error, reload: e.snapshot.reload };
  for (const l of listeners) l();
}

function widOf(scope: string): string {
  return scope.slice(scope.indexOf(":") + 1);
}

async function read(scope: string): Promise<void> {
  const e = entryFor(scope);
  if (e.inFlight) {
    e.again = true;
    return;
  }
  e.inFlight = true;
  e.loading = true;
  publish(scope);
  try {
    e.data = await api.gitFiles(widOf(scope));
    e.error = null;
  } catch (err) {
    e.error = failureText("work", "git-files-store-failed", err);
  } finally {
    e.inFlight = false;
    e.loading = false;
    publish(scope);
    if (e.again) {
      e.again = false;
      void read(scope);
    }
  }
}

/** Ask now — the session's `stale` moved, a Refresh was pressed. */
export function refreshGitFiles(scope: string): void {
  void read(scope);
}

/** A write answered with the fresh rows: they are the read, no round trip — for a root somebody shows or showed. */
export function landedGitFiles(scope: string, files: readonly GitFileRow[]): void {
  const e = entries.get(scope);
  if (!e) return;
  e.data = { workstream: widOf(scope), clean: files.length === 0, files: [...files] };
  e.error = null;
  publish(scope);
}

/** Every root somebody shows, read again — the node came back after being away. */
function readShown(): void {
  for (const [scope, e] of entries) if (e.subscribers > 0) void read(scope);
}

/**
 * The roots nobody shows any more, oldest first past `MAX_IDLE_ROOTS`, let
 * go of their rows. A read in flight is left to land; the next leave prunes it.
 */
function prune(): void {
  const idle = [...entries].filter(([, e]) => e.subscribers === 0 && !e.inFlight && e.flush === null);
  for (const [scope] of idle.slice(0, Math.max(0, idle.length - MAX_IDLE_ROOTS))) entries.delete(scope);
}

function onFrame(type: string | undefined, payload: { scope?: string; id?: string; path?: string; kind?: string }): void {
  if (!type) return;
  if (WRITE_FRAMES.has(type)) {
    for (const [scope, e] of entries) if (e.subscribers > 0) void read(scope);
    return;
  }
  if (type !== "file_changed" || payload.scope !== "workstream" || !payload.id) return;
  if (!readsFor(kindsOf({ path: payload.path ?? "", kind: payload.kind ?? "" })).files) return;
  const scope = rootKey("workstream", payload.id);
  const e = entries.get(scope);
  if (!e || e.subscribers === 0 || e.flush !== null) return;
  e.flush = setTimeout(() => {
    e.flush = null;
    void read(scope);
  }, GIT_COALESCE_MS);
}

function ensureBus(): void {
  unsubscribeBus ??= busSubscribe({ stream: "engine" }, (f) => {
    if (f.stream !== "engine") return;
    const p = f.payload.payload as { type?: string; scope?: string; id?: string; path?: string; kind?: string };
    onFrame(p.type, p);
  });
  unwatchConnection ??= reloadOnReconnect(watchConnection, readShown);
}

function subscribeTo(scope: string): () => void {
  const e = entryFor(scope);
  e.subscribers += 1;
  ensureBus();
  if (e.subscribers === 1 && e.data === null && !e.inFlight) void read(scope);
  return () => {
    e.subscribers -= 1;
    if (e.subscribers === 0 && e.flush !== null) {
      clearTimeout(e.flush);
      e.flush = null;
    }
    if (e.subscribers === 0) prune();
  };
}

const IDLE: GitFilesRead = { data: null, loading: false, refreshing: false, error: null, reload: () => undefined };

/** One checkout's changed files, live while something shows them; `null` asks for nothing and reads idle. */
export function useGitFiles(wid: string | null): GitFilesRead {
  const scope = wid === null ? null : rootKey("workstream", wid);
  useEffect(() => (scope === null ? undefined : subscribeTo(scope)), [scope]);
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    () => (scope === null ? IDLE : entryFor(scope).snapshot),
    () => (scope === null ? IDLE : entryFor(scope).snapshot),
  );
}
