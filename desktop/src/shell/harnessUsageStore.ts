/**
 * What each harness's account has left — one module store keyed by harness
 * id, on the `statsStore` pattern: read on demand, when a row shows its
 * usage line; refreshed on a slow timer while any row shows it (the node
 * holds a report for `cache.harness_usage.ttl_ms`, so the timer is that
 * long); paused when nothing reads it or the window is hidden. A row that
 * hides its usage stops asking; the rail's row and the panel's row for the
 * same harness share one entry.
 *
 * A re-read that fails — the node saying *failed*, or the node unreachable —
 * keeps the last report for an hour and records why (`settled`, the one
 * rule in `harnessUsageModel.mjs`), so a rate-limited endpoint dims the
 * numbers instead of blanking them, as Claude Code's own `/usage` does.
 *
 * The node does the reading with the harness's own sign-in; what arrives
 * here is percentages, labels and reset times — never a credential.
 */

import { useEffect, useSyncExternalStore } from "react";
import { api } from "../api";
import type { UsageState } from "../types";
import { settled } from "./harnessUsageModel.mjs";
import { isHidden, onVisibilityChange } from "./visibility";
import { t } from "../i18n/l10n.mjs";
import { failureReason } from "../ui/failure";

/** The node holds a report three minutes; asking sooner only reads the cache. */
const USAGE_POLL_MS = 180_000;

export interface HarnessUsageEntry {
  /** The node's answer — or the kept report — or null before the first. */
  readonly state: UsageState | null;
  /** Unix seconds the answer arrived, 0 before any. */
  readonly readAt: number;
  readonly loading: boolean;
  /** Why the last re-read failed while `state` is the kept report; null otherwise. */
  readonly stale: string | null;
}

const EMPTY: HarnessUsageEntry = { state: null, readAt: 0, loading: false, stale: null };

let entries: Readonly<Record<string, HarnessUsageEntry>> = {};
const listeners = new Set<() => void>();
/** How many rows show each harness's usage right now. */
const shown = new Map<string, number>();
const inFlight = new Set<string>();
/** A refresh asked for while a read was out: the source is asked again once it lands, never dropped. */
const again = new Set<string>();
let timer: ReturnType<typeof setInterval> | null = null;

function set(id: string, patch: Partial<HarnessUsageEntry>) {
  entries = { ...entries, [id]: { ...(entries[id] ?? EMPTY), ...patch } };
  for (const l of listeners) l();
}

async function read(id: string, refresh: boolean): Promise<void> {
  if (inFlight.has(id)) {
    // A press on Refresh while the poll's read is out is not a press that does nothing.
    if (refresh) again.add(id);
    return;
  }
  inFlight.add(id);
  set(id, { loading: true });
  let answer: UsageState;
  try {
    answer = (await api.harnessUsage(id, refresh)).usage;
  } catch (e) {
    answer = { state: "failed", reason: t("shell-harness-usage-store-node-unreached", { error: failureReason("usage", "a harness's usage could not be read", e) }) }; // for the log
  }
  set(id, { ...settled(entries[id] ?? EMPTY, answer, Math.floor(Date.now() / 1000)), loading: false });
  inFlight.delete(id);
  if (again.delete(id)) void read(id, true);
}

function tick(refresh = false): void {
  if (isHidden()) return;
  for (const id of shown.keys()) void read(id, refresh);
}

function schedule(): void {
  const run = shown.size > 0 && !isHidden();
  if (run && !timer) {
    timer = setInterval(() => tick(), USAGE_POLL_MS);
  } else if (!run && timer) {
    clearInterval(timer);
    timer = null;
  }
}

onVisibilityChange(() => {
  schedule();
  // Back on screen: anything a row shows is read again if it is stale.
  if (!isHidden()) {
    const now = Math.floor(Date.now() / 1000);
    for (const id of shown.keys()) {
      if (now - (entries[id]?.readAt ?? 0) >= USAGE_POLL_MS / 1000) void read(id, false);
    }
  }
});

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** Ask the node to read the harness's source again, now. */
export function refreshHarnessUsage(id: string): void {
  void read(id, true);
}

/**
 * The harness's usage, read while `shown`. A row that hides its line passes
 * `false` and stops counting as a reader; the entry stays for the next show.
 */
export function useHarnessUsage(id: string | null, shown: boolean): HarnessUsageEntry {
  useEffect(() => {
    if (!id || !shown) return;
    shownCount(id, 1);
    if ((entries[id]?.readAt ?? 0) === 0) void read(id, false);
    schedule();
    return () => {
      shownCount(id, -1);
      schedule();
    };
  }, [id, shown]);
  return useSyncExternalStore(subscribe, () => (id ? entries[id] ?? EMPTY : EMPTY), () => EMPTY);
}

function shownCount(id: string, delta: number) {
  const next = (shown.get(id) ?? 0) + delta;
  if (next <= 0) shown.delete(id);
  else shown.set(id, next);
}
