/**
 * A count of one kind of record the bus keeps fresh — what the notes and the
 * drawings docks wear as their badge (`notes/noteCount.ts`,
 * `draw/drawingCount.ts`).
 *
 * A count is a fact about the workspace, not about a panel. The panels' own
 * lists are read only while they are open and per tab, so a badge fed from
 * them was absent after a launch until the panel was opened once, frozen
 * while it was closed, and one tab's number rather than every record's. This
 * store reads the whole listing once when something first shows the count,
 * reads it again — once per burst, `COUNT_COALESCE_MS` — on the frames the
 * caller's `moves` names, and again when the bus comes back after the node
 * was away (`reloadOnReconnect`: what was written while it was away was said
 * by no frame, and the first read of a launch that outran the node lands
 * then). Nothing shows it: nothing is heard, and **the last count stays**,
 * so a dock hidden and shown again paints it at once, then reads again.
 *
 * A module store on `sessionsStore`'s pattern (the first listener starts it,
 * the last stops it) and `workstreamStatusStore`'s (one read per burst of
 * frames). Answers land in the order they were asked for (`createLatest`):
 * an older read landing after a newer one draws nothing, and `close`/`open`
 * around stop and start hold the rule through React's strict-mode
 * subscribe → unsubscribe → subscribe. A read that fails keeps the last count
 * — `null` before any — and says so at `debug`; the next frame or the bus
 * coming back reads again.
 *
 * The count is a listing's length: `GET /notes` and `GET /drawings` with no
 * query list every scope (the *All* tab's own read). A notes listing carries
 * bodies and a drawings listing is the index alone — fine for a scratchpad
 * behind a coalesced read; the node's index already answers the count should
 * a route for it ever be wanted.
 */

import { useSyncExternalStore } from "react";
import { subscribe as busSubscribe, watchConnection } from "../bus";
import { errorFields, log } from "../log";
import { createLatest } from "./latestModel.mjs";
import { reloadOnReconnect } from "./workspaceLoadModel.mjs";

/** How long a burst of frames is gathered before one read — an agent appending to several notes is one. */
const COUNT_COALESCE_MS = 300;

export interface LiveCountSpec {
  /** The log's name for this count — `notes`, `drawings`. */
  readonly what: string;
  /** Read the count now; the signal ends a read nobody waits for. */
  readonly read: (signal: AbortSignal) => Promise<number>;
  /** Whether an engine frame of this `type` moves the count. */
  readonly moves: (type: unknown) => boolean;
}

export interface LiveCount {
  /** The count while something shows it — `null` until the first answer. */
  useCount(): number | null;
}

export function createLiveCount({ what, read, moves }: LiveCountSpec): LiveCount {
  let count: number | null = null;
  const listeners = new Set<() => void>();
  const latest = createLatest();
  let flush: ReturnType<typeof setTimeout> | null = null;
  let inFlight: AbortController | null = null;
  let unsubscribeBus: (() => void) | null = null;
  let unwatchConnection: (() => void) | null = null;

  function set(next: number): void {
    if (next === count) return;
    count = next;
    for (const l of listeners) l();
  }

  async function load(): Promise<void> {
    // A read asked for while one is out replaces it: the newer answer is the one wanted.
    inFlight?.abort();
    const ctrl = new AbortController();
    inFlight = ctrl;
    const ticket = latest.begin();
    try {
      const n = await read(ctrl.signal);
      if (ctrl.signal.aborted || !latest.lands(ticket)) return;
      set(n);
    } catch (e) {
      if (ctrl.signal.aborted || !latest.lands(ticket)) return;
      // The last count stands; the next frame or the bus coming back asks again.
      log.debug(what, "the count could not be read; the last one stands", errorFields(e));
    } finally {
      if (inFlight === ctrl) inFlight = null;
    }
  }

  /** One read per burst of frames, trailing. */
  function soon(): void {
    if (flush !== null) return;
    flush = setTimeout(() => {
      flush = null;
      void load();
    }, COUNT_COALESCE_MS);
  }

  function start(): void {
    latest.open();
    void load();
    // No goal in the filter: a goal's deletion is a goal-scoped frame, and it moves the count too.
    unsubscribeBus = busSubscribe({ stream: "engine" }, (frame) => {
      if (frame.stream !== "engine") return;
      if (moves((frame.payload.payload as { type?: unknown }).type)) soon();
    });
    unwatchConnection = reloadOnReconnect(watchConnection, () => void load());
  }

  function stop(): void {
    unsubscribeBus?.();
    unsubscribeBus = null;
    unwatchConnection?.();
    unwatchConnection = null;
    // A frame just before the hide must not read for nobody.
    if (flush !== null) {
      clearTimeout(flush);
      flush = null;
    }
    inFlight?.abort();
    inFlight = null;
    latest.close();
  }

  function subscribe(l: () => void): () => void {
    listeners.add(l);
    if (listeners.size === 1) start();
    return () => {
      listeners.delete(l);
      if (listeners.size === 0) stop();
    };
  }

  const snapshot = () => count;

  return {
    useCount() {
      return useSyncExternalStore(subscribe, snapshot, snapshot);
    },
  };
}
