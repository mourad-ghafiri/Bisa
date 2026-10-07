/**
 * The engine's session roster — every session's presence (`presence.rs`),
 * seeded once from `GET /sessions` and moved by the bus: a `session_state`
 * frame upserts a row, a `session_gone` frame drops it. No polling drives it;
 * a slow safety refetch catches a frame the stream lost. Every surface that
 * shows a session — the rail, the Agents pane, the Agents screen, the pet,
 * the notifications — reads this one store.
 */

import { useEffect, useSyncExternalStore } from "react";
import { api } from "../api";
import { errorFields, log } from "../log";
import type { SessionRow, SessionState } from "../types";
import { subscribe as busSubscribe, watchConnection } from "../bus";
import { reloadOnReconnect } from "./workspaceLoadModel.mjs";
import { isHidden, onVisibilityChange } from "./visibility";
import { sameJsonList } from "./snapshotEqual.mjs";
import { dropped, landedRead, latest, stoppedAlready, transitionsBetween, upserted, type StopOutcome } from "./sessionRosterModel.mjs";
import { endedOf, type StopOutcomeBlock } from "./stopOutcomeModel.mjs";

/** A frame the stream lost is caught here; nothing else refetches. Tunable via
 *  `cache.desktop.sessions_safety_ms`. */
let safetyMs = 60_000;

/** Retune the safety refetch cadence; restarts the timer at the new interval. */
export function setSessionsSafetyMs(ms: number): void {
  if (!Number.isFinite(ms) || ms <= 0 || ms === safetyMs) return;
  safetyMs = ms;
  if (timer) {
    clearInterval(timer);
    timer = setInterval(() => {
      if (!isHidden()) reloadSessions();
    }, safetyMs);
  }
}

interface State {
  readonly sessions: readonly SessionRow[];
  readonly at: number;
}

/** A row moved from one state to another (or appeared: `prev` null). */
type TransitionListener = (prev: SessionState | null, next: SessionRow) => void;

let state: State = { sessions: [], at: 0 };
const listeners = new Set<() => void>();
const transitions = new Set<TransitionListener>();
let timer: ReturnType<typeof setInterval> | null = null;
let unsubscribe: (() => void) | null = null;
let unwatchConnection: (() => void) | null = null;
let inFlight: AbortController | null = null;

/**
 * The snapshot is the whole `state` object, so a `set` with a fresh reference
 * re-renders every surface that reads the roster — the rail, the footer, the
 * panes, the pet. A duplicate `session_state` frame or an
 * identical safety refetch must not do that, so commit only a roster that
 * differs. The rows are wire DTOs (plain JSON), so a structural compare is exact.
 */
function commit(sessions: readonly SessionRow[]): boolean {
  if (sameJsonList(state.sessions, sessions)) return false;
  state = { sessions, at: Date.now() };
  for (const l of listeners) l();
  return true;
}

/**
 * The rows frames moved while a read of the roster is out — the row, or
 * `null` for one that went — `null` itself while no read is. The read was
 * taken before those frames were said, so it lands under them
 * (`sessionRosterModel.landedRead`).
 */
let since: Map<string, SessionRow | null> | null = null;

/** Re-seed from the node; the bus keeps it moving in between. */
function reloadSessions(): void {
  inFlight?.abort();
  const ac = new AbortController();
  inFlight = ac;
  const heard = new Map<string, SessionRow | null>();
  since = heard;
  api
    .sessions(ac.signal)
    .then((r) => {
      if (ac.signal.aborted) return;
      // A row a re-read moved — a frame the stream lost — is announced as a
      // transition too, so a tab's harness is closed on an `aborted` the
      // frame never brought, and the working dot hears it.
      const before = state.sessions;
      const next = landedRead(r.sessions, heard);
      if (commit(next)) {
        for (const [prev, row] of transitionsBetween(before, next)) {
          for (const t of transitions) t(prev, row);
        }
      }
    })
    .catch((e: unknown) => {
      // The last roster stands; the safety tick asks again.
      if (ac.signal.aborted) return;
      log.debug("sessions", "the roster could not be read; the last one stands", errorFields(e));
    })
    .finally(() => {
      // Only the read that is still the one out stops the listening: a newer read keeps its own.
      if (since === heard) since = null;
    });
}

function upsert(row: SessionRow) {
  // The frames heard while a read is out keep the newest word of each row;
  // a row a frame said was gone stays gone.
  if (since) {
    const heard = since.get(row.id);
    if (heard !== null) since.set(row.id, heard ? latest(heard, row) : row);
  }
  const prev = state.sessions.find((s) => s.id === row.id) ?? null;
  const next = upserted(state.sessions, row);
  // The same array: a frame older than the row held (two reports landing
  // together, in the other order) — not a change, and not a transition.
  if (next === state.sessions) return;
  commit(next);
  if (!prev || prev.state.state !== row.state.state) {
    for (const t of transitions) t(prev?.state ?? null, row);
  }
}

function drop(id: string) {
  since?.set(id, null);
  commit(dropped(state.sessions, id));
}

/**
 * Stop one session by its row — the one door every Stop, Abort and stop mark
 * on the desktop goes through (`POST /sessions/{id}/abort`: the node stops
 * the harness, whatever drives the session). A row the node no longer has
 * answers 404: the session ended and the frame that said so was lost, so the
 * row is dropped here and nothing is thrown — a press on it again would
 * otherwise be an error toast every time, until the safety read.
 */
export async function stopSession(id: string): Promise<{ how: StopOutcome; ended: StopOutcomeBlock | null }> {
  try {
    const answer = await api.abortSession(id);
    // The node answers once the process is gone — or was terminated at the
    // deadline — and says which (`ended`).
    return { how: "stopped", ended: endedOf(answer) };
  } catch (e) {
    if (!stoppedAlready(e)) throw e;
    drop(id);
    return { how: "gone", ended: null };
  }
}

function start() {
  if (timer) return;
  reloadSessions();
  // The safety refetch is a net for a lost frame; a hidden window needs none
  // (the bus keeps the roster live for the moment it is looked at again).
  timer = setInterval(() => {
    if (!isHidden()) reloadSessions();
  }, safetyMs);
  unsubscribe = busSubscribe({ stream: "engine" }, (frame) => {
    if (frame.stream !== "engine") return;
    const p = frame.payload.payload;
    if (p.type === "session_state") upsert(p.presence);
    else if (p.type === "session_gone") drop(p.live_run);
  });
  // The stream came back after a gap: a node that restarted in it forgot
  // every session and said nothing, so the roster is read again whole.
  // (The edge is closed→open with a `connecting` in between; the helper
  // reads it as one gap — an inline `wasClosed` that reset on `connecting`
  // never fired.)
  unwatchConnection = reloadOnReconnect(watchConnection, reloadSessions);
}

// Coming back into view, catch anything the stream missed while hidden.
onVisibilityChange(() => {
  if (!isHidden() && timer) reloadSessions();
});

function stop() {
  if (timer) clearInterval(timer);
  timer = null;
  unsubscribe?.();
  unsubscribe = null;
  unwatchConnection?.();
  unwatchConnection = null;
}

function subscribe(l: () => void) {
  listeners.add(l);
  if (listeners.size === 1) start();
  return () => {
    listeners.delete(l);
    if (listeners.size === 0) stop();
  };
}

/** Hear every state change while subscribed — what the notifications read. */
export function onSessionTransition(l: TransitionListener): () => void {
  transitions.add(l);
  const off = subscribe(() => undefined);
  return () => {
    transitions.delete(l);
    off();
  };
}

/** The roster as it stands — a read for non-render code (a close counting what it ends), not for render. */
export function sessionRows(): readonly SessionRow[] {
  return state.sessions;
}

/** The roster, live while any surface shows it. */
export function useSessions(): readonly SessionRow[] {
  const s = useSyncExternalStore(subscribe, () => state, () => state);
  useEffect(() => {
    if (state.at === 0) reloadSessions();
  }, []);
  return s.sessions;
}

