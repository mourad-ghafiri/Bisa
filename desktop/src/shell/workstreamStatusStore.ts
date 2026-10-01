/**
 * Every workstream's live status — one module store on the `portsStore`
 * pattern, so a checkout's dirty counts, its pull request and its record's
 * state are on hand wherever a screen needs them: the project rail's rows,
 * the right panel's rail marks, whichever occupant is showing.
 *
 * One read for all of them (`GET /workstreams/status`, cached two seconds on
 * the node), polled while something reads it and the window is visible,
 * asked again at once when a workstream's record or its project's moves
 * (`workstreamFramesModel.movesStatuses` — the one list the Workstreams
 * panel's hook reads too) and once per burst of `file_changed` frames — a
 * build touching files is one refetch, not one per write. A git act that
 * changes the index without a frame — a stage, a discard — calls
 * `refreshWorkstreamStatuses()` itself.
 *
 * A node that was away and came back is read at once
 * (`reloadOnReconnect`): what it forgot in a restart reaches the rail and the
 * Board by no frame, and the next tick may be fifteen seconds off.
 *
 * Snapshots commit only when the list differs, so a poll that found nothing
 * moved re-renders nobody.
 */

import { log } from "../log";
import { useEffect, useSyncExternalStore } from "react";
import { api } from "../api";
import { subscribe as busSubscribe, watchConnection } from "../bus";
import type { WorkstreamStatus } from "../types";
import { sameJsonList } from "./snapshotEqual.mjs";
import { movesStatuses } from "./workstreamFramesModel.mjs";
import { reloadOnReconnect } from "./workspaceLoadModel.mjs";
import { isHidden, onVisibilityChange } from "./visibility";

/** How often the statuses are read while something shows them. */
export const STATUS_POLL_MS = 15_000;
/** How long a burst of watcher frames is gathered before one refetch. */
export const STATUS_COALESCE_MS = 150;

interface State {
  readonly statuses: readonly WorkstreamStatus[];
  readonly at: number;
  /** Why the last read failed while these statuses stand; `null` while the reads answer. */
  readonly stale: string | null;
}

const EMPTY: readonly WorkstreamStatus[] = [];
let state: State = { statuses: EMPTY, at: 0, stale: null };
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;
let flush: ReturnType<typeof setTimeout> | null = null;
let unsubscribeBus: (() => void) | null = null;
let unwatchConnection: (() => void) | null = null;
let inFlight = false;
/** A refresh asked for while one was in flight: ask again when it lands. */
let again = false;

function set(next: State): void {
  state = next;
  for (const l of listeners) l();
}

function commit(statuses: readonly WorkstreamStatus[]): void {
  if (sameJsonList(state.statuses, statuses)) {
    if (state.stale !== null) set({ ...state, stale: null });
    return;
  }
  set({ statuses, at: Date.now(), stale: null });
}

async function read(): Promise<void> {
  if (inFlight) {
    again = true;
    return;
  }
  inFlight = true;
  try {
    const { statuses } = await api.allWorkstreamStatuses();
    commit(statuses);
  } catch (e) {
    // The last answer stands, marked as kept; the next tick asks again.
    const why = e instanceof Error ? e.message : String(e);
    if (state.stale !== why) {
      log.debug("workstreams", "the statuses could not be read; the last answer stands", { reason: why });
      set({ ...state, stale: why });
    }
  } finally {
    inFlight = false;
    if (again) {
      again = false;
      void read();
    }
  }
}

/** Ask now — after a stage, a discard, a commit made here, so the marks move with the act. */
export function refreshWorkstreamStatuses(): void {
  void read();
}

function onFrame(type: string | undefined): void {
  if (!type) return;
  if (movesStatuses(type)) {
    void read();
    return;
  }
  if (type === "file_changed" && flush === null) {
    flush = setTimeout(() => {
      flush = null;
      void read();
    }, STATUS_COALESCE_MS);
  }
}

function schedule(): void {
  const shouldRun = listeners.size > 0 && !isHidden();
  if (shouldRun && !timer) {
    void read();
    timer = setInterval(() => void read(), STATUS_POLL_MS);
    unsubscribeBus ??= busSubscribe({ stream: "engine" }, (f) => {
      if (f.stream !== "engine") return;
      onFrame((f.payload.payload as { type?: string }).type);
    });
    unwatchConnection ??= reloadOnReconnect(watchConnection, () => void read());
  } else if (!shouldRun && timer) {
    clearInterval(timer);
    timer = null;
    unsubscribeBus?.();
    unsubscribeBus = null;
    unwatchConnection?.();
    unwatchConnection = null;
    if (flush !== null) {
      clearTimeout(flush);
      flush = null;
    }
  }
}

// A hidden window reads nothing; on becoming visible the read runs at once.
onVisibilityChange(schedule);

function subscribe(l: () => void): () => void {
  listeners.add(l);
  schedule();
  return () => {
    listeners.delete(l);
    schedule();
  };
}

/** Every workstream's status, live while something shows them. */
export function useWorkstreamStatuses(): readonly WorkstreamStatus[] {
  return useSyncExternalStore(subscribe, () => state.statuses, () => EMPTY);
}

/** One workstream's status, or null until the first read answers or when the node does not know it. */
export function useWorkstreamStatus(wid: string | null): WorkstreamStatus | null {
  const statuses = useWorkstreamStatuses();
  // A new root is worth an immediate look rather than a wait for the tick.
  useEffect(() => {
    if (wid && !statuses.some((s) => s.workstream === wid)) void read();
    // Once per root: following `statuses` would ask again on every tick for
    // a workstream the node does not know.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wid]);
  return wid ? (statuses.find((s) => s.workstream === wid) ?? null) : null;
}
