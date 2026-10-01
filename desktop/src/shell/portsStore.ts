/**
 * The listening ports the desktop shell can trace to a rail's shell or to a
 * harness the node runs — one module store, polled, read by the
 * rail through `usePorts()`.
 *
 * Polled rather than pushed: a port is opened by a process the app never
 * spawned (a dev server two forks under a login shell), so no event tells the
 * app about it; the OS's socket table is the only witness, and asking it every
 * few seconds while anything is live costs tens of milliseconds. The poll
 * sleeps when nothing can own a port — no live shell, no session with a pid —
 * and outside the desktop shell there is no scanner at all, so the store is
 * empty and the rail draws nothing.
 *
 * The roots the scan is told about are the harness pids the roster carries
 * (`SessionRow.pid`, set by the node when the adapter's process starts); the
 * shells' pids the Tauri side already knows from its own registry.
 */

import { log } from "../log";
import { useEffect, useSyncExternalStore } from "react";
import { listeningPorts, terminalAvailable, type ListeningPort } from "../terminal/session";
import type { SessionRow } from "../types";
import { useSessions } from "./sessionsStore";
import { isLive as shellIsLive } from "./terminalsModel.mjs";
import { isHidden, onVisibilityChange } from "./visibility";
import { sameJsonList } from "./snapshotEqual.mjs";
import { useTerminals } from "./useTerminals";

/** How often the socket table is read while something can own a port.
 *  Tunable via `cache.desktop.ports_poll_ms`. */
let pollMs = 5_000;

/** Retune the scan cadence; restarts the timer at the new interval. */
export function setPortsPollMs(ms: number): void {
  if (!Number.isFinite(ms) || ms <= 0 || ms === pollMs) return;
  pollMs = ms;
  if (timer) {
    clearInterval(timer);
    timer = null;
    schedule();
  }
}

interface State {
  readonly ports: readonly ListeningPort[];
  readonly at: number;
  /** Why the last scan failed while these ports stand; `null` while the scans answer. */
  readonly stale: string | null;
}

let state: State = { ports: [], at: 0, stale: null };
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;
let roots: readonly number[] = [];
let awake = false;
let inFlight = false;

function set(next: State) {
  state = next;
  for (const l of listeners) l();
}

/**
 * The snapshot is the whole `state`, so replacing it on every 5 s scan
 * re-renders the rail even when the socket table has not moved.
 * Commit — and notify — only a genuinely different set of ports. `ListeningPort`
 * is a wire DTO, so a structural compare is exact.
 */
function commitPorts(ports: readonly ListeningPort[]): void {
  if (sameJsonList(state.ports, ports)) {
    if (state.stale !== null) set({ ...state, stale: null });
    return;
  }
  set({ ports, at: Date.now(), stale: null });
}

async function scan(): Promise<void> {
  if (inFlight || !terminalAvailable()) return;
  inFlight = true;
  try {
    const ports = await listeningPorts(roots);
    commitPorts(ports);
  } catch (e) {
    // The last answer stands, marked as kept; the next tick asks again.
    const why = e instanceof Error ? e.message : String(e);
    if (state.stale !== why) {
      log.warn("ports", "the ports could not be scanned; the last answer stands", { reason: why, kept: state.ports.length });
      set({ ...state, stale: why });
    }
  } finally {
    inFlight = false;
  }
}

/** Ask now — after a *Stop*, so the chip goes with the process rather than five seconds later. */
export function rescanPorts(): void {
  void scan();
}

function schedule(): void {
  const shouldRun = awake && listeners.size > 0 && !isHidden();
  if (shouldRun && !timer) {
    void scan();
    timer = setInterval(() => void scan(), pollMs);
  } else if (!shouldRun && timer) {
    clearInterval(timer);
    timer = null;
    if (state.ports.length > 0) set({ ports: [], at: Date.now(), stale: null });
  }
}

// A hidden window scans nothing; on becoming visible the scan runs at once.
onVisibilityChange(schedule);

function subscribe(l: () => void) {
  listeners.add(l);
  schedule();
  return () => {
    listeners.delete(l);
    schedule();
  };
}

/** The harness pids the roster knows — the roots the scan is told about. */
export function rootPids(sessions: readonly SessionRow[]): number[] {
  const out: number[] = [];
  for (const s of sessions) {
    const pid = (s as { pid?: unknown }).pid;
    if (typeof pid === "number" && Number.isInteger(pid) && pid > 0) out.push(pid);
  }
  return out;
}

/**
 * Every attributable listening port, live while a rail shows them. The store
 * wakes when a shell is live or a session has a pid, and sleeps otherwise.
 */
export function usePorts(): readonly ListeningPort[] {
  const sessions = useSessions();
  const terminals = useTerminals();
  const nextRoots = rootPids(sessions);
  const anyShell = terminals.sessions.some(shellIsLive);
  const rootsKey = nextRoots.join(",");
  useEffect(() => {
    roots = nextRoots;
    awake = anyShell || nextRoots.length > 0;
    schedule();
    // A new root is worth an immediate look: a harness that just started may
    // already be serving.
    if (awake && timer) void scan();
    // `nextRoots` is a fresh array every render; `rootsKey` is its text.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rootsKey, anyShell]);
  const s = useSyncExternalStore(subscribe, () => state, () => state);
  return s.ports;
}

/** Why the ports shown are the last ones read rather than the ones open now; `null` while the scans answer. */
export function usePortsStale(): string | null {
  return useSyncExternalStore(subscribe, () => state.stale, () => null);
}
