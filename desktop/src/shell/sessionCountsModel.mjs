/**
 * One counting rule for the sessions every surface tallies — the footer's
 * harnesses, the node overlay's live sessions, the resource overlay's
 * processes, the tray's and the pet's agents at work — so two surfaces never
 * say two numbers about one roster.
 *
 * The footer lists every harness process the engine drives or a tab claims:
 * a step's worker, the Workflow Agent's design wake and a one-shot ask (the
 * classifier reading, the Decision-Making Agent judging, a commit message
 * suggested) stand in no checkout and have no rail row to be found by, so
 * the footer is where a person sees them — a conversation's turn stays its
 * conversation's (ide/09). The rail's own rule (`isDrawn`) is untouched.
 */

import { counts, isLive } from "../ui/sessionState.mjs";
import { claimedSessions } from "../views/_workbench/workstreamSessionsModel.mjs";

/** The kinds the engine drives with no checkout of their own. */
export const OFF_CHECKOUT_KINDS = Object.freeze(["guided", "ask"]);

/**
 * Whether a roster row is a harness process a person sees in the footer: a
 * worker, a design wake, a one-shot ask, or a terminal a tab claims.
 * @param {object | null | undefined} row
 * @param {Map<string, string>} claimed session id → tab key (`claimedSessions`)
 */
export function isHarnessProcess(row, claimed) {
  if (!row) return false;
  if (row.kind === "worker" || OFF_CHECKOUT_KINDS.includes(row.kind)) return true;
  return row.kind === "terminal" && claimed.has(row.id);
}

/** The footer's rows: live harness processes, idle ones included. */
export function harnessRows(sessions, terminals) {
  const claimed = claimedSessions(sessions ?? [], terminals ?? []);
  return (sessions ?? []).filter((s) => isHarnessProcess(s, claimed) && isLive(s.state));
}

/** Every live row — what the node counts as its live sessions. */
export function liveRows(sessions) {
  return (sessions ?? []).filter((s) => isLive(s.state));
}

/** The live rows with a process behind them — what the resource overlay attributes. */
export function processRows(sessions) {
  return liveRows(sessions).filter((s) => typeof s.pid === "number");
}

/**
 * The scope a row works in — the thread, the conversation, the goal — as
 * the bus spells it in `agent_thinking`, so a hint and a row meet.
 * @param {object | null | undefined} row
 */
export function scopeOf(row) {
  const o = row?.origin;
  if (o && o.origin === "turn" && typeof o.scope === "string" && o.scope) return o.scope;
  return row?.conversation ?? row?.goal ?? null;
}

/** Whether a row is mid-turn: thinking or running a tool. */
export function isBusy(row) {
  const w = row?.state?.state;
  return w === "thinking" || w === "running";
}

/** The scopes busy rows name. */
export function busyScopes(sessions) {
  return new Set((sessions ?? []).filter(isBusy).map(scopeOf).filter(Boolean));
}

/**
 * How many agents are at work: the roster's busy sessions, plus the scopes
 * mid-turn by the bus's hint (`useWorkspace().working`) that no busy row
 * already names — a turn is counted once, by its row.
 * @param {readonly object[] | null | undefined} sessions
 * @param {Readonly<Record<string, readonly string[]>> | null | undefined} working
 */
export function workingCount(sessions, working) {
  const rows = Array.isArray(sessions) ? sessions : [];
  const named = busyScopes(rows);
  const hinted = Object.entries(working ?? {}).filter(([scope, who]) => Array.isArray(who) && who.length > 0 && !named.has(scope)).length;
  return counts(rows).working + hinted;
}

/**
 * The four numbers at once, from one roster.
 * @param {readonly object[] | null | undefined} sessions
 * @param {readonly object[] | null | undefined} terminals
 * @param {Readonly<Record<string, readonly string[]>> | null | undefined} [working]
 */
export function sessionCounts(sessions, terminals, working) {
  return {
    live: liveRows(sessions).length,
    harnesses: harnessRows(sessions, terminals).length,
    processes: processRows(sessions).length,
    working: workingCount(sessions, working),
  };
}
