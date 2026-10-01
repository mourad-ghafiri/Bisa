/**
 * The session vocabulary — the nine words `presence.rs` folds a harness
 * session into, and what each means on a screen: its label, its tone, its
 * glyph, and where it ranks when one dot must speak for many sessions.
 *
 * One table for the rail, the Agents pane, the Agents screen, the pet, the
 * feed and the notifications, so *needs you* is the same colour everywhere
 * and a state nobody can produce is not in the list. The test reads the Rust
 * source and fails the moment the engine's enum and this list disagree.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * Every state, in the one order a mark and a line both read: what needs
 * you first (*waiting*, *failed*, *aborted*), then what is happening — a
 * running tool before *thinking*, since a tool has a name and arguments —
 * then what happened (*done*), then the rest. A loader beside a tick is the
 * loader; the tick comes when the work is over.
 */
export const STATES = Object.freeze(["waiting", "failed", "aborted", "running", "thinking", "starting", "done", "idle", "parked"]);

const RANK = Object.freeze(Object.fromEntries(STATES.map((s, i) => [s, i + 1])));

const TABLE = Object.freeze({
  waiting: { label: t("ui-chip-waiting"), tone: "accent", icon: "icon:permission" },
  failed: { label: t("ui-session-state-failed"), tone: "danger", icon: "icon:danger" },
  aborted: { label: t("ui-session-state-aborted"), tone: "danger", icon: "icon:terminate" },
  done: { label: t("ui-session-state-done"), tone: "ok", icon: "icon:ok" },
  thinking: { label: t("ui-session-state-thinking"), tone: "working", icon: "icon:thinking" },
  running: { label: t("ui-session-state-running"), tone: "working", icon: "icon:working" },
  starting: { label: t("ui-session-state-starting"), tone: "dim", icon: "icon:working" },
  idle: { label: t("ui-session-state-idle"), tone: "dim", icon: "icon:agent" },
  parked: { label: t("ui-session-state-parked"), tone: "dim", icon: "icon:parked" },
});

/** The word of a `SessionState` value (the serde tag), or the word itself. */
export function stateOf(state) {
  if (!state) return "idle";
  if (typeof state === "string") return STATES.includes(state) ? state : "idle";
  return STATES.includes(state.state) ? state.state : "idle";
}

/** `{label, tone, icon, rank}` for a state. Unknown words read as idle. */
export function describe(state) {
  const word = stateOf(state);
  return { ...TABLE[word], rank: RANK[word] };
}

/** Where a state ranks: 1 is the loudest. */
export function attentionRank(state) {
  return RANK[stateOf(state)];
}

export function toneOf(state) {
  return TABLE[stateOf(state)].tone;
}

export function iconOf(state) {
  return TABLE[stateOf(state)].icon;
}

/**
 * Still going: drawn live, its clock counting up. Not the abort rule — an
 * idle session is alive between turns and nothing to stop (`isStoppable`).
 */
export function isLive(state) {
  return ["starting", "idle", "thinking", "running", "waiting"].includes(stateOf(state));
}

/**
 * A person may end it: it is doing something — starting, thinking, running a
 * tool — or blocked on them (a wait; an auth wait has no gate to answer).
 * Never a session idle between turns, parked, or ended: there is nothing
 * running to stop, and the button would end a session that was merely
 * waiting for the next message. The one rule every Stop, Abort and stop
 * mark on the desktop reads.
 */
export function isStoppable(state) {
  return ["starting", "thinking", "running", "waiting"].includes(stateOf(state));
}

/** Wants a person: the states that interrupt. */
export function isAttention(state) {
  const w = stateOf(state);
  return w === "waiting" || w === "failed";
}

/** A finished session — kept a while, then gone. */
export function isEnded(state) {
  const w = stateOf(state);
  return w === "done" || w === "aborted" || w === "failed";
}

/**
 * The sentence a row prints for its state: *running Edit*, *waiting on you —
 * permission: Bash*, *failed: the reason*.
 * @param {object | string} state a `SessionState` value
 */
export function label(state) {
  if (!state || typeof state === "string") return TABLE[stateOf(state)].label;
  switch (state.state) {
    case "running":
      return state.tool ? t("ui-session-state-running-tool", { tool: state.tool }) : t("ui-session-state-running");
    case "waiting": {
      const on = state.on;
      if (!on) return t("ui-chip-waiting");
      if (on.on === "permission") return t("ui-session-state-waiting-permission", { tool: on.tool });
      if (on.on === "question") return t("ui-session-state-waiting", { on: on.text });
      if (on.on === "gate") return t("ui-session-state-waiting-gate", { gate: on.gate });
      if (on.on === "auth") return t("ui-session-state-waiting-sign", { provider: on.provider });
      return t("ui-chip-waiting");
    }
    case "failed":
      return state.reason ? t("ui-session-state-failed-reason", { reason: state.reason }) : t("ui-session-state-failed");
    default:
      return TABLE[stateOf(state)].label;
  }
}

/**
 * What a waiting session waits on, in its own words — *permission: Bash*,
 * the question, *approval gate*, *sign in to GitHub* — with no *waiting on
 * you* before them: for a surface that has already said so, as a
 * notification's title has. Nothing for a session that does not wait, or
 * that says no more than that it does.
 * @param {object | string | null | undefined} state a `SessionState` value
 * @returns {string | null}
 */
export function waitWords(state) {
  if (!state || typeof state === "string" || state.state !== "waiting") return null;
  const on = state.on;
  if (!on) return null;
  if (on.on === "permission") return t("ui-session-state-wait-permission", { tool: on.tool });
  if (on.on === "question") return typeof on.text === "string" && on.text ? on.text : null;
  if (on.on === "gate") return t("ui-session-state-wait-gate", { gate: on.gate });
  if (on.on === "auth") return t("ui-session-state-wait-sign", { provider: on.provider });
  return null;
}

/**
 * The sub-agents a session still shows: one that is live, and one that
 * failed — kept red until the engine's next turn boundary drops it. One that
 * finished or was aborted has left, whatever a stale frame still lists; and
 * a session that has ended shows none — its children went with it.
 * The one rule the rail's rows, the pulse's subjects and the `↳ N` count read.
 * @param {{state: object, children?: {state: object}[]} | null | undefined} row
 */
export function liveChildren(row) {
  if (!row || !isLive(row.state)) return [];
  return (row.children ?? []).filter((c) => isLive(c.state) || stateOf(c.state) === "failed");
}

/**
 * A child speaks only to ask: a sub-agent waiting on a person is the session
 * waiting on a person, and a failed one is the session's failure. A child
 * that merely works, or finished, says nothing for its parent — the parent's
 * own word already reads *running sub-agent* while its children are the work
 * — and no child speaks for a parent that has ended.
 */
function childWords(row) {
  return liveChildren(row).map((c) => c.state).filter(isAttention);
}

/**
 * What a set of rows adds up to — the sessions themselves; a child counts
 * only when it wants a person.
 * @param {{state: object, children?: {state: object}[]}[]} rows
 */
export function counts(rows) {
  const out = { waiting: 0, working: 0, done: 0, failed: 0, live: 0 };
  const tally = (state) => {
    const w = stateOf(state);
    if (w === "waiting") out.waiting += 1;
    else if (w === "thinking" || w === "running" || w === "starting") out.working += 1;
    else if (w === "done") out.done += 1;
    else if (w === "failed" || w === "aborted") out.failed += 1;
  };
  for (const r of rows ?? []) {
    tally(r.state);
    if (isLive(r.state)) out.live += 1;
    for (const state of childWords(r)) tally(state);
  }
  return out;
}

/** The loudest state among rows — and their children, where a child asks — what one dot says. */
export function loudest(rows) {
  let best = null;
  const consider = (state) => {
    const w = stateOf(state);
    if (best === null || RANK[w] < RANK[best]) best = w;
  };
  for (const r of rows ?? []) {
    consider(r.state);
    for (const state of childWords(r)) consider(state);
  }
  return best ?? "idle";
}

/** Rows by attention, then newest first. Stable, never reshuffles under equal keys. */
export function sortRows(rows) {
  return [...(rows ?? [])].sort((a, b) => attentionRank(a.state) - attentionRank(b.state) || (b.since ?? 0) - (a.since ?? 0));
}

/**
 * Whether a transition is a moment worth an OS notification: an edge *into*
 * waiting, failed or done. A session that is still waiting is not news
 * twice, and a session appearing is not news. Which of these the person
 * wants is the notifications model's (`allowed`), never decided here.
 * @param {object | string | null} prev
 * @param {object | string} next
 */
export function isNotifiable(prev, next) {
  const from = prev ? stateOf(prev) : null;
  const to = stateOf(next);
  if (from === to) return false;
  return to === "waiting" || to === "failed" || to === "done";
}

/**
 * The gate a waiting state can be answered through in the Inbox, or null —
 * a gate of its own, or a permission or question the engine escalated to
 * one. A harness's own prompt (no gate id) is answered where it asked.
 * @param {StateLike} state
 */
export function gateOf(state) {
  if (!state || typeof state !== "object" || state.state !== "waiting" || !state.on) return null;
  const on = state.on;
  if (on.on === "gate" || on.on === "permission" || on.on === "question") return on.gate_id ?? null;
  return null;
}
