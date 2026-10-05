/**
 * The pulse of a workstream: the one compact line the rail's
 * workstream row prints under its name so a collapsed row still says what
 * the harness is doing — *claude-code · running Edit · src/cart.rs · 12s*,
 * *↳ explore · waiting on you — permission: Bash · 2m*, *failed: the reason*.
 *
 * Facts only, no React: which session is the subject, what its sentence is,
 * how long it has been in that state, what the person can do about it, and a
 * key that changes exactly when something visible changed — so the view can
 * flash the line once on a transition and never on a token.
 *
 * `workstreamActivityModel.mjs` still owns the dot and the counts; this file
 * owns the words. The two read the same roster and the same vocabulary
 * (`ui/sessionState.mjs`), so the dot and the line never disagree.
 */

import { settledByTab, harnessOf } from "../../shell/terminalsModel.mjs";
import { liveChildren, attentionRank, gateOf, isAttention, isLive, isStoppable, label, stateOf, toneOf } from "../../ui/sessionState.mjs";
import { claimedSessions, isDrawn } from "./workstreamSessionsModel.mjs";
import { durationPrecise, relative } from "../../i18n/format.mjs";
import { terminalState } from "./workstreamActivityModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** How much of a tool's arguments the line shows. */
export const ARGS_CHARS = 40;
/** How much of a failure's reason. */
const REASON_CHARS = 60;
/** How long an ended session's line lingers — the roster drops it anyway; this is the client's cap. */
const LINGER_SECS = 15 * 60;

/** Which subject leads the line: the one order the marks read too (`sessionState.STATES`). */
function rank(state) {
  return attentionRank(state);
}

/** Cut a string with an ellipsis, on a word boundary when one is near. */
export function truncate(text, max) {
  const t = String(text ?? "").trim();
  if (t.length <= max) return t;
  const cut = t.slice(0, max);
  const space = cut.lastIndexOf(" ");
  return `${(space > max * 0.6 ? cut.slice(0, space) : cut).trimEnd()}…`;
}

/**
 * The sentence a row prints, shortened for one line: the tool's arguments
 * and a failure's reason are cut; `thinking` gets its ellipsis.
 * @param {object} state a `SessionState`
 */
export function headlineOf(state) {
  const word = stateOf(state);
  if (word === "running" && state && typeof state === "object") {
    const args = truncate(state.args, ARGS_CHARS);
    return args ? tr("workbench-workstream-pulse-running-tool-args", { tool: state.tool, args }) : tr("workbench-workstream-pulse-running-tool", { tool: state.tool });
  }
  if (word === "failed" && state && typeof state === "object" && state.reason) return tr("workbench-workstream-pulse-failed-reason", { reason: truncate(state.reason, REASON_CHARS) });
  if (word === "thinking") return tr("workbench-workstream-pulse-thinking");
  return label(state);
}

/**
 * Loudest first; among equals the subject that started last, then the id. A
 * steady lead: `last_activity` moves on every token and `since` on every
 * tool call, and a line whose subject swapped between two working sessions
 * at each of them read as a flicker, not as news.
 */
function byLoudness(a, b) {
  return rank(a.state) - rank(b.state) || (b.start ?? 0) - (a.start ?? 0) || String(a.id).localeCompare(String(b.id));
}

/**
 * The one span a line shows: while live, total time since the
 * subject began (`start`, falling back to the state instant when no start is
 * recorded); once ended, how long the run took (`start` → its terminal-state
 * instant `since`), and a relative "5m" when a start is unknown.
 * @param {{start?: number, since: number}} subject
 */
function elapsedOf(subject, live, now) {
  const start = subject.start > 0 ? subject.start : subject.since;
  if (live) return start > 0 ? durationPrecise(now - start) : "";
  if (subject.since <= 0) return "";
  if (subject.start > 0 && subject.since > subject.start) return durationPrecise(subject.since - subject.start);
  return relative(subject.since, now);
}

/**
 * The subjects a workstream offers: every session and every sub-agent as a
 * row that knows its parent, plus a failed shell when no agent is here.
 */
function subjects(sessions, terminals, workstream) {
  const out = [];
  const claimed = claimedSessions(sessions, terminals);
  const tabByKey = new Map((terminals ?? []).map((t) => [t.key, t]));
  for (const raw of sessions ?? []) {
    if (raw.workstream !== workstream || !isDrawn(raw, claimed)) continue;
    // As the claiming tab says: a tab that exited ended the session.
    const s = settledByTab(raw, tabByKey.get(claimed.get(raw.id)));
    const children = liveChildren(s);
    // `start` is the session's registration instant (never reset), the anchor
    // for total open time and a run's completed-in duration.
    out.push({ kind: "session", id: s.id, state: s.state, since: s.since, start: s.started ?? s.since, last_activity: s.last_activity ?? s.since, who: s.agent ?? s.harness, harness: s.harness, parent: null, children });
    for (const c of children) {
      // A sub-agent's `started` is its spawn instant, never reset; `since`
      // the state it is in — the same two clocks as its parent.
      out.push({ kind: "subagent", id: `${s.id}/${c.id}`, state: c.state, since: c.since, start: c.started, last_activity: c.since, who: `↳ ${c.name}`, harness: s.harness, parent: s.id, children: [] });
    }
  }
  if (out.length === 0) {
    for (const t of terminals ?? []) {
      if (t.scope !== "workstream" || t.id !== workstream) continue;
      if (t.sessionId && claimed.has(t.sessionId)) continue;
      const word = terminalState(t);
      if (word !== "failed") continue;
      // A shell's end instant is its `exitedAt`, and its start its `openedAt`:
      // the failed line reads how long it ran.
      out.push({ kind: "shell", id: t.key, state: { state: "failed", reason: tr("workbench-workstream-pulse-shell-exited", { code: t.liveness.code }) }, since: t.exitedAt ?? 0, start: t.openedAt ?? 0, last_activity: t.exitedAt ?? 0, who: harnessOf(t) ?? "shell", harness: harnessOf(t), parent: null, children: [] });
    }
  }
  return out;
}

/**
 * The line for one workstream, or null when nothing is standing here (or
 * everything ended long ago).
 * The workstream's `ports` ride along on whatever line leads — a harness that
 * opened one shows the chip beside its own words; a workstream with only a
 * live shell gets a quiet line of its own so the chip has a home.
 * @param {{sessions: readonly object[], terminals: readonly object[], workstream: string, now: number, ports?: readonly object[]}} input `now` in unix seconds
 */
export function pulseOf({ sessions, terminals, workstream, now, ports = [] }) {
  const all = subjects(sessions, terminals, workstream);
  if (all.length === 0) return liveShellPulse(terminals, workstream, ports, now);
  // The loudest subject leads; a sub-agent leads only when it wants a person
  // (waiting, failed) — a parent is never upstaged by a child that merely works.
  const sorted = [...all].sort(byLoudness);
  let lead = sorted[0];
  if (lead.kind === "subagent" && !isAttention(lead.state)) {
    lead = all.find((s) => s.id === lead.parent) ?? lead;
  }
  const word = stateOf(lead.state);
  const live = isLive(lead.state);
  if (!live && lead.since > 0 && now - lead.since > LINGER_SECS) return null;

  // The rest of the sessions, folded into one more chip — `+2 working` — by
  // the next-loudest bucket: waiting, failed, working (any live word), else the word.
  const bucket = (state) => {
    const w = stateOf(state);
    if (w === "waiting" || w === "failed") return w;
    return isLive(state) ? "working" : w;
  };
  const others = all.filter((s) => s.kind === "session" && s.id !== lead.id && s.id !== lead.parent);
  let more = null;
  if (others.length > 0) {
    const next = bucket([...others].sort(byLoudness)[0].state);
    more = { count: others.filter((s) => bucket(s.state) === next).length, word: next };
  }

  const children = lead.kind === "session" ? lead.children : [];
  const subagents = {
    total: children.length,
    working: children.filter((c) => isLive(c.state) && stateOf(c.state) !== "waiting").length,
    waiting: children.filter((c) => stateOf(c.state) === "waiting").length,
    failed: children.filter((c) => stateOf(c.state) === "failed").length,
    names: children.map((c) => `${c.name} — ${label(c.state)}`),
  };

  const gateId = gateOf(lead.state);
  // Abort only on a session that runs or waits on the person — never on one
  // idle between turns, which is alive but has nothing running to end.
  const cta = gateId ? { kind: "answer", gateId } : lead.kind === "session" && isStoppable(lead.state) ? { kind: "abort", sessionId: lead.id } : null;
  const tier = word === "running" && lead.state && typeof lead.state === "object" ? (lead.state.tier ?? null) : null;
  // The instant a live counter counts up from — the same `start` `elapsedOf`
  // uses. The view ticks it in a leaf so the model needn't rebuild each second.
  const liveFrom = live ? (lead.start > 0 ? lead.start : lead.since) : 0;

  return {
    word,
    tone: toneOf(lead.state),
    tier,
    who: lead.who,
    harness: lead.harness,
    headline: headlineOf(lead.state),
    detail: [
      lead.who,
      label(lead.state),
      word === "running" && lead.state.args ? lead.state.args : null,
      // The current-state timer lives here, on hover, now that the visible
      // time is the total / completed-in span.
      live && lead.since > 0 ? tr("workbench-workstream-pulse-state", { since: durationPrecise(now - lead.since) }) : null,
    ]
      .filter(Boolean)
      .join(" — "),
    since: lead.since,
    // Live: total open time since the session began. Ended: how long the run
    // took (start → the terminal-state instant), else a relative fallback.
    elapsed: elapsedOf(lead, live, now),
    liveStart: liveFrom > 0 ? liveFrom : null,
    more,
    subagents,
    cta,
    ports,
    flashKey: `${lead.id}:${word}:${lead.since}`,
  };
}

/**
 * The quiet line a workstream with only a live shell gets — no agent is here,
 * so the shell is the subject, dim. Its elapsed is how long the shell has been
 * open, and it gives the workstream's ports a home; with no live shell and no
 * ports there is nothing to say. A shell says nothing about what runs in it:
 * a harness that reports is a roster session and leads its own line above.
 * @param {number} now unix seconds
 */
function liveShellPulse(terminals, workstream, ports, now) {
  const live = (terminals ?? []).filter((t) => t.scope === "workstream" && t.id === workstream && t.liveness?.status === "live");
  if (live.length === 0) return null;
  const harness = live.map((t) => harnessOf(t)).find((h) => h) ?? null;
  // The oldest still-open shell anchors the line's clock.
  const openedAt = live.reduce((min, t) => (typeof t.openedAt === "number" && (min === null || t.openedAt < min) ? t.openedAt : min), null);
  const n = live.length;
  return {
    word: "idle",
    tone: "dim",
    tier: null,
    who: harness ?? "shell",
    harness,
    headline: n === 1 ? tr("workbench-workstream-pulse-shell-open") : tr("workbench-workstream-pulse-shells-open", { n }),
    detail: n === 1 ? tr("workbench-workstream-pulse-shell-open-here") : tr("workbench-workstream-pulse-shells-open-here", { n }),
    since: openedAt ?? 0,
    elapsed: openedAt ? durationPrecise(now - openedAt) : "",
    liveStart: openedAt ?? null,
    more: null,
    subagents: { total: 0, working: 0, waiting: 0, failed: 0, names: [] },
    cta: null,
    ports,
    flashKey: `shell:${workstream}:${n}`,
  };
}

/**
 * A collapsed project's line: the loudest of its workstreams' pulses; among
 * equals the live line whose subject started last (`liveStart`, which a
 * tool call never moves), then the one that entered its state last.
 */
export function projectPulse(pulses) {
  const list = (pulses ?? []).filter(Boolean);
  if (list.length === 0) return null;
  return [...list].sort((a, b) => rank(a.word) - rank(b.word) || (b.liveStart ?? 0) - (a.liveStart ?? 0) || b.since - a.since)[0];
}

/** The words for the `+N` chip. */
export function moreLabel(more) {
  if (!more) return "";
  return `+${more.count} ${more.word}`;
}
