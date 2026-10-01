/**
 * The turns in flight, as the desktop holds them (13 — Conversations §The
 * reply streams): the bus's `agent_streamed` frames add up to each agent's
 * words, thinking and tool line per scope; `agent_replied` settles the turn
 * with the message it became, and the timeline retires it once that message
 * is in its page (`retireLandedTurns`); a timeline that joins mid-turn is
 * primed once from the node (`GET /conversations/{id}/live`) — and primed
 * again for every scope with a turn in flight when the bus comes back after
 * the node was away: what the node finished or lost in the gap reached
 * nobody by a frame. A turn that settles in a scope no timeline reads is
 * cleared, not frozen (`watching`): the store holds the turns in flight and
 * the settled ones a page is about to draw, and nothing else. One subscription to the engine stream, held only while
 * a timeline reads.
 *
 * Two readings, so a frame re-renders one row and not the timeline: the
 * agents with a row (`useLiveTurnAgents`, a stable array until the set of
 * agents changes) and one agent's turn (`useLiveTurn`, the same object until
 * that turn moves). The rules are `liveTurnModel.mjs`'s.
 */

import { useEffect, useSyncExternalStore } from "react";
import { api } from "../../api";
import { subscribe as busSubscribe, watchConnection } from "../../bus";
import { reloadOnReconnect } from "../../shell/workspaceLoadModel.mjs";
import type { EnginePayload } from "../../types";
import { NO_TURNS, agentsOf, applyStreamed, dropSettled, landedOf, primeTurns, retireLanded, sameAgents, settleTurn } from "./liveTurnModel.mjs";
import type { LiveTurn, LiveTurns } from "./liveTurnModel.mjs";

let turns: LiveTurns = NO_TURNS;
/** The agents with a row per scope, rebuilt only when the scope's turns moved and the agents changed. */
let agentsByScope = new Map<string, readonly string[]>();
const listeners = new Set<() => void>();
let unsubscribeBus: (() => void) | null = null;
let unwatchConnection: (() => void) | null = null;

const NO_AGENTS: readonly string[] = Object.freeze([]);

/**
 * The scopes a timeline is reading, each with how many. A turn that settles
 * in a scope nobody reads is cleared rather than frozen — no page would ever
 * retire it — so what the store holds is bounded by the turns in flight and
 * the scopes on screen, never by every reply the bus has carried.
 */
const watching = new Map<string, number>();

function watch(scope: string): () => void {
  watching.set(scope, (watching.get(scope) ?? 0) + 1);
  return () => {
    const left = (watching.get(scope) ?? 1) - 1;
    if (left > 0) {
      watching.set(scope, left);
      return;
    }
    watching.delete(scope);
    set(dropSettled(turns, scope));
  };
}

function set(next: LiveTurns): void {
  if (next === turns) return;
  const before = turns;
  turns = next;
  const rebuilt = new Map<string, readonly string[]>();
  for (const scope of next.keys()) {
    const kept = agentsByScope.get(scope);
    if (kept && next.get(scope) === before.get(scope)) {
      rebuilt.set(scope, kept);
      continue;
    }
    const fresh = agentsOf(next, scope);
    rebuilt.set(scope, kept && sameAgents(kept, fresh) ? kept : Object.freeze(fresh));
  }
  agentsByScope = rebuilt;
  for (const l of listeners) l();
}

function onPayload(p: EnginePayload): void {
  if (p.type === "agent_streamed") set(applyStreamed(turns, p));
  else if (p.type === "agent_replied") set(settleTurn(turns, p.scope, p.agent, landedOf(p), watching.has(p.scope)));
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  unsubscribeBus ??= busSubscribe({ stream: "engine" }, (f) => {
    if (f.stream === "engine") onPayload(f.payload.payload);
  });
  // The node came back: every scope with a turn in flight is given the node's whole turns again, as a reader joining mid-turn is.
  unwatchConnection ??= reloadOnReconnect(watchConnection, () => {
    for (const scope of turns.keys()) void primeLiveTurns(scope);
  });
  return () => {
    listeners.delete(l);
    if (listeners.size === 0 && unsubscribeBus) {
      unsubscribeBus();
      unsubscribeBus = null;
      unwatchConnection?.();
      unwatchConnection = null;
      set(NO_TURNS);
    }
  };
}

/** The agents with a row in flight in a scope, oldest turn first — a stable array until the set changes. */
export function useLiveTurnAgents(scope: string): readonly string[] {
  // The timeline that draws a scope's turns is what reads the scope.
  useEffect(() => watch(scope), [scope]);
  return useSyncExternalStore(
    subscribe,
    () => agentsByScope.get(scope) ?? NO_AGENTS,
    () => NO_AGENTS,
  );
}

/** One agent's turn in flight in a scope — the same object until that turn moves; undefined when none. */
export function useLiveTurn(scope: string, agent: string): LiveTurn | undefined {
  return useSyncExternalStore(
    subscribe,
    () => turns.get(scope)?.get(agent),
    () => undefined,
  );
}

/** The messages a timeline now draws: every settled turn among them gives way to its message. */
export function retireLandedTurns(messageIds: Iterable<string>): void {
  set(retireLanded(turns, new Set(messageIds)));
}

/**
 * What a reader that joins mid-turn is given: the node's whole turns for the
 * conversation, replacing the frames the bus said since the reader came. A
 * scope that is not a conversation, or a node that cannot answer, leaves
 * the bus's account standing.
 */
export async function primeLiveTurns(conversation: string, signal?: AbortSignal): Promise<void> {
  try {
    const { turns: rows } = await api.conversationLive(conversation, signal);
    set(primeTurns(turns, conversation, rows));
  } catch {
    // The bus keeps saying what it hears; nothing to add.
  }
}
