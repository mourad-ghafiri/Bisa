/**
 * The session each workstream is about, as chosen — a module store the IDE
 * writes (an Agent panel row, a rail agent row, a harness tab brought to
 * the centre) and the pet reads. The rule that turns a choice into the
 * followed row, with its fallback, is `followedSessionModel.mjs`.
 *
 * Session-only, never persisted: a session id does not survive a restart,
 * and the terminal store's `sessionId` is not persisted either. A session
 * that leaves the roster (`session_gone`) is forgotten here too, so a stale
 * choice cannot outlive what it named.
 *
 * In `shell/` because its writers are screens and its reader is an overlay:
 * it sits beside `sessionsStore.ts`, and `pet/` imports nothing from `views/`.
 * The one door back into the IDE — showing the Agent panel for a followed
 * workstream — is a callback the workbench registers, for the same reason.
 */

import { useSyncExternalStore } from "react";
import { subscribe as busSubscribe } from "../bus";

interface State {
  /** The chosen session per workstream, oldest choice first, capped. */
  readonly byWorkstream: Readonly<Record<string, string>>;
}

const MAX_REMEMBERED = 32;

let state: State = { byWorkstream: {} };
const listeners = new Set<() => void>();

function set(next: State): void {
  if (next === state) return;
  state = next;
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** Forget a session everywhere it was chosen — it is gone from the roster. */
function forget(session: string): void {
  if (!Object.values(state.byWorkstream).includes(session)) return;
  const byWorkstream = Object.fromEntries(Object.entries(state.byWorkstream).filter(([, s]) => s !== session));
  set({ byWorkstream });
}

let watching = false;
function watch(): void {
  if (watching) return;
  watching = true;
  busSubscribe({ stream: "engine" }, (frame) => {
    if (frame.stream !== "engine") return;
    const p = frame.payload.payload;
    if (p.type === "session_gone") forget(p.live_run);
  });
}

/** Choose the session a workstream is about. */
export function followSession(workstream: string, session: string): void {
  if (state.byWorkstream[workstream] === session) return;
  watch();
  const rest = Object.entries(state.byWorkstream).filter(([w]) => w !== workstream);
  const entries = [...rest, [workstream, session] as const].slice(-MAX_REMEMBERED);
  set({ byWorkstream: Object.fromEntries(entries) });
}

/** The chosen session of a workstream — a read for non-render code. */
function followedSessionId(workstream: string | null): string | null {
  return workstream ? (state.byWorkstream[workstream] ?? null) : null;
}

/** The chosen session of a workstream, as a subscription. */
export function useFollowedSession(workstream: string | null): string | null {
  return useSyncExternalStore(
    subscribe,
    () => followedSessionId(workstream),
    () => followedSessionId(workstream),
  );
}

/**
 * The way from the pet into the IDE: whoever can show the Agent panel for
 * a workstream registers here while it is mounted. A click on a following
 * pet goes through it; with nobody registered the click only navigates.
 */
type Opener = (workstream: string) => void;
let opener: Opener | null = null;

export function registerFollowedOpener(cb: Opener): () => void {
  opener = cb;
  return () => {
    if (opener === cb) opener = null;
  };
}

export function openFollowed(workstream: string): void {
  opener?.(workstream);
}
