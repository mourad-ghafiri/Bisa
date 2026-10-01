/**
 * The path index per workbench root (ide/12): every file path under the root,
 * fetched once per visit and patched from `file_changed`, so quick open and
 * the Files tab's name search walk the tree once between them rather than
 * per keystroke each. A `rescan` frame forgets the index; the next asker
 * refetches.
 *
 * The fetch is a **caller-independent single flight** (`singleFlight.mjs`):
 * two askers share one request, one of them leaving does not end it, and a
 * flight that is aborted or fails is forgotten so the next asker starts
 * again. That is what lets a component mount, unmount and mount again — as
 * React's strict mode does — and still get its index.
 */

import { useEffect, useState } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { errorFields, log } from "../log";
import type { FileScope } from "../types";
import { patchIndex } from "./quickOpenScore.mjs";
import { Lru } from "./lru.mjs";
import { createSingleFlight } from "./singleFlight.mjs";

interface Entry {
  readonly paths: readonly string[];
  readonly truncated: boolean;
}

/**
 * Each entry holds every path under a root — thousands of strings for a large
 * tree — and a weeks-long session visits many roots. Keeping them all forever
 * is the one genuinely unbounded memory grower, so the cache is an
 * LRU of the few most-recently-touched roots; a root scrolled off is refetched
 * on its next visit, exactly as a first visit is.
 */
const MAX_ROOTS = 8;

const cache = new Lru<Entry>(MAX_ROOTS);
const listeners = new Set<() => void>();
const flights = createSingleFlight();

function notify(): void {
  for (const l of listeners) l();
}

/** The cached index for a root, or null when nobody has fetched it yet. */
export function cachedIndex(scope: FileScope, id: string): Entry | null {
  return cache.get(`${scope}:${id}`) ?? null;
}

/** Retune how many roots are kept (`cache.path_index.max_roots`). */
export function setMaxRoots(max: number): void {
  cache.setCapacity(max);
}

/**
 * Fetch (once) and cache the index for a root. `signal` detaches this caller
 * only; the request itself ends when the last caller has left.
 */
export function loadIndex(scope: FileScope, id: string, signal?: AbortSignal): Promise<Entry> {
  const key = `${scope}:${id}`;
  // `Lru.get` marks the root most-recently-used, so a hit keeps it hot.
  const have = cache.get(key);
  if (have) return Promise.resolve(have);
  return flights.join(
    key,
    (flightSignal) =>
      api.ideIndex(scope, id, flightSignal).then((r) => {
        const entry = { paths: r.paths, truncated: r.truncated };
        cache.set(key, entry);
        notify();
        return entry;
      }),
    signal ?? null,
  );
}

/** A `file_changed` frame: patch the index it is about, or forget it on a rescan. */
function applyFileChange(scope: string, id: string, change: { kind: string; path: string; from?: string | null; dir?: boolean; ignored?: boolean }): void {
  const key = `${scope}:${id}`;
  const have = cache.get(key);
  if (!have) return;
  if (change.kind === "rescan") {
    cache.delete(key);
    notify();
    return;
  }
  const next = patchIndex(have.paths, change);
  // More than a list of files can take from one frame — a tree copied in, a
  // rename out of what was hidden: forgotten, and read again by whoever asks.
  if (next === null) {
    cache.delete(key);
    notify();
    return;
  }
  if (next !== have.paths) {
    cache.set(key, { paths: next, truncated: have.truncated });
    notify();
  }
}

/**
 * The index for a root, fetched on first use and kept current. `null` until
 * it arrives; a root that cannot be indexed (not on disk yet) stays `null`.
 */
export function usePathIndex(scope: FileScope | null, id: string | null): Entry | null {
  const [, bump] = useState(0);
  useEffect(() => {
    const l = () => bump((n) => n + 1);
    listeners.add(l);
    return () => {
      listeners.delete(l);
    };
  }, []);
  useEffect(() => {
    if (!scope || !id) return;
    const ac = new AbortController();
    loadIndex(scope, id, ac.signal).catch((e: unknown) => {
      // No directory to index yet: the caller shows nothing for names until
      // an index lands.
      if (ac.signal.aborted) return;
      log.debug("index", "the path index could not be read; names stay empty until one lands", { scope, id, ...errorFields(e) });
    });
    return () => ac.abort();
  }, [scope, id]);
  useEngineEvents((e) => {
    const p = e.payload;
    // The frame whole: whether the path is a folder and whether the root's
    // ignore rules match it are what the patch decides on (`patchIndex`).
    if (p.type === "file_changed") applyFileChange(p.scope, p.id, { kind: p.kind, path: p.path, from: p.from, dir: p.dir, ignored: p.ignored });
  });
  return scope && id ? cachedIndex(scope, id) : null;
}
