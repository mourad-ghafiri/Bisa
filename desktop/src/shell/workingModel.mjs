/**
 * Who is mid-turn where — the *agent writing* dot the top bar, a goal's
 * header, its card, the sidebar, the tray and the pet read (`useWorkspace().working`).
 *
 * The bus says it first: `agent_thinking` on the first token, `agent_replied`
 * when the turn is over. One lost frame left the dot on until a reload, so
 * the roster is the truth the hint is held to: a row that ended, parked or
 * went idle clears its agent from its scope, and a roster read whole rebuilds
 * the map from its busy rows alone. The scope is the string the bus spells —
 * a turn's own on its row (`origin.turn.scope`), else the conversation or the
 * goal it names (`sessionCountsModel.scopeOf`).
 */

import { isBusy, scopeOf } from "./sessionCountsModel.mjs";

/** The bus's hint: an agent began a turn in a scope. Idempotent. */
export function withThinking(working, scope, agent) {
  if (!scope || !agent) return working;
  if (working[scope]?.includes(agent)) return working;
  return { ...working, [scope]: [...(working[scope] ?? []), agent] };
}

/** The bus's word: an agent's turn in a scope is over. The same map when nothing changes. */
export function withReplied(working, scope, agent) {
  if (!scope) return working;
  const before = working[scope] ?? [];
  const after = before.filter((a) => a !== agent);
  return after.length === before.length ? working : { ...working, [scope]: after };
}

/**
 * The roster's word on one row: ended, parked or idle, its agent is no
 * longer mid-turn in its scope — whatever frame was lost.
 * @param {Readonly<Record<string, readonly string[]>>} working
 * @param {object | null | undefined} row a `SessionRow`
 */
export function clearedByRow(working, row) {
  const scope = scopeOf(row);
  if (!scope || !row?.agent) return working;
  const w = row.state?.state;
  const over = w === "done" || w === "failed" || w === "aborted" || w === "parked" || w === "idle";
  return over ? withReplied(working, scope, row.agent) : working;
}

/**
 * The map rebuilt from a roster read whole: a scope is working where a busy
 * row with an agent stands, and nowhere else — the read is the truth, and a
 * hint it cannot confirm was a frame whose end was lost.
 * @param {readonly object[] | null | undefined} rows
 * @returns {Record<string, string[]>}
 */
export function rebuilt(rows) {
  const next = {};
  for (const r of rows ?? []) {
    const scope = scopeOf(r);
    if (!scope || !r.agent || !isBusy(r)) continue;
    if (!next[scope]?.includes(r.agent)) next[scope] = [...(next[scope] ?? []), r.agent];
  }
  return next;
}

/** Whether two maps say the same — so a rebuild that changes nothing commits nothing. */
export function sameWorking(a, b) {
  const ka = Object.keys(a ?? {}).filter((k) => (a[k] ?? []).length > 0);
  const kb = Object.keys(b ?? {}).filter((k) => (b[k] ?? []).length > 0);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => {
    const x = [...(a[k] ?? [])].sort();
    const y = [...(b?.[k] ?? [])].sort();
    return x.length === y.length && x.every((v, i) => v === y[i]);
  });
}
