/**
 * Who is standing in a workstream: its open terminals, and the **work
 * sessions** running in it — a step's worker, a harness a person opened in a
 * terminal — each followed by its sub-agents. A conversation's turn stands in
 * the checkout too, but it is the conversation's, reached through the Agent
 * panel; it is never a row of the checkout (ide/07, 13 — Conversations).
 *
 * The one builder for this. The rail's left panel and the right panel's
 * Workstreams occupant both draw the same live picture — the same
 * `SessionMark` words, the same order — so extracting it here is what keeps the
 * two from drifting apart. Facts only, no React: the state each row carries is
 * the live `SessionState`/liveness from the roster; the callers add whatever
 * layout they need (the rail its depth and project).
 *
 * A harness a person opened in a terminal reports as a roster session, so it
 * arrives in `sessions` with a real state and its sub-agents. Its terminal tab
 * carries that session's id: the two are one thing, drawn once — as the agent
 * row, which remembers the tab (`terminalKey`) so a click lands in it. A
 * terminal with no session — a plain shell, a harness that cannot report — is
 * a terminal row that knows only whether it is alive.
 */

import { harnessOf, settledByTab, shellWord } from "../../shell/terminalsModel.mjs";
import { gateOf, liveChildren, label as stateLabel } from "../../ui/sessionState.mjs";

/**
 * Which roster sessions a terminal tab **claims**: the tab's `sessionId`
 * names a session the roster knows. The map is session id → tab key.
 *
 * The rule every surface reads (the rail, the Workstreams panel, the
 * activity fold): a harness a person opened in a terminal
 * (`kind: "terminal"`) is drawn and counted **only through the tab that
 * claims it** — as the agent row, which remembers the tab. A terminal
 * row no tab claims is not drawn at all: it is mid-open (the node registers
 * the row before the PTY exists) or an orphan another host left behind, and
 * either way it is not something standing here. A tab whose session the
 * roster does not know is a terminal row; a claimed tab is never also one.
 * @param {readonly object[]} sessions
 * @param {readonly object[]} terminals
 * @returns {Map<string, string>}
 */
export function claimedSessions(sessions, terminals) {
  const known = new Set((sessions ?? []).map((s) => s.id));
  const claimed = new Map();
  for (const t of terminals ?? []) if (t.sessionId && known.has(t.sessionId)) claimed.set(t.sessionId, t.key);
  return claimed;
}

/**
 * The kinds of session that are a checkout's own rows: a step's worker, and
 * a harness a person opened in a terminal. A conversation's turn and a
 * note's answer are reached through what they answer in, never here.
 */
export const WORK_KINDS = Object.freeze(["worker", "terminal"]);

/**
 * Whether a roster session is drawn as a row of the checkout it stands in
 * — the one rule the rail, the Workstreams panel, the footer, the pulse
 * line, the activity fold and the resource overlay read: a work session
 * only (`WORK_KINDS`), and a terminal one only when a tab claims it.
 * @param {object} session
 * @param {Map<string, string>} claimed
 */
export function isDrawn(session, claimed) {
  if (!WORK_KINDS.includes(session.kind)) return false;
  return session.kind !== "terminal" || claimed.has(session.id);
}

/**
 * The rows a workstream contains, in reading order: its unreported terminals
 * first, then each agent session immediately followed by its sub-agents. A
 * terminal row carries its liveness; an agent row carries its `SessionState`;
 * a sub-agent names its parent.
 * @param {readonly object[]} sessions the roster (`SessionRow[]`)
 * @param {readonly object[]} terminals the terminal sessions
 * @param {string} workstream
 * @returns {WorkstreamSessionRow[]}
 */
export function workstreamSessionRows(sessions, terminals, workstream) {
  const claimed = claimedSessions(sessions, terminals);
  // The tab behind each reported session, so the agent row can open it —
  // and so the row reads as the tab says (`settledByTab`): a session whose
  // tab exited has ended, its sub-agents with it, whatever the roster's last
  // frame still says.
  const tabOf = new Map();
  const shells = [];
  for (const t of terminals ?? []) {
    if (t.scope !== "workstream" || t.id !== workstream) continue;
    if (t.sessionId && claimed.has(t.sessionId)) tabOf.set(t.sessionId, t);
    else shells.push(t);
  }
  const agents = (sessions ?? [])
    .filter((s) => s.workstream === workstream && isDrawn(s, claimed))
    .map((s) => settledByTab(s, tabOf.get(s.id)));
  return [
    ...shells.map((t) => ({
      kind: "terminal",
      id: t.key,
      workstream,
      // What the tab is about — launched, or typed into the shell (`harnessOf`).
      label: harnessOf(t) ?? shellWord(),
      harness: harnessOf(t),
      liveness: t.liveness,
      // For "open 12m" while live and "ran 5m 20s" once exited.
      openedAt: t.openedAt ?? null,
      exitedAt: t.exitedAt ?? null,
      parent: null,
    })),
    // A session, then its sub-agents one level in — a harness's own spawn is
    // work a person should see, not a mystery inside the parent's row.
    ...agents.flatMap((s) => [
      {
        kind: "agent",
        id: s.id,
        workstream,
        label: s.agent ?? s.harness,
        harness: s.harness,
        // The model the session runs on, as the harness names it — the
        // launch's, then whatever the harness reported since.
        model: s.model ?? null,
        // The effort it runs at, once fitted to the model; none on a harness that takes none.
        effort: s.effort ?? null,
        agent: s.agent ?? null,
        state: s.state,
        activity: stateLabel(s.state),
        since: s.since,
        started: s.started ?? null,
        workItem: s.work_item ?? null,
        goal: s.goal ?? null,
        gateId: gateOf(s.state),
        // How many sub-agents nest under this session — the ones still shown
        // (`liveChildren`) — so a surface can print the count and offer a
        // chevron without re-deriving it.
        childCount: liveChildren(s).length,
        // The terminal tab this session lives in, when a person opened it there.
        terminalKey: tabOf.get(s.id)?.key ?? null,
        parent: null,
      },
      // A sub-agent is a row while it is live, or failed and not yet seen
      // past a turn boundary; one that finished has left.
      ...liveChildren(s).map((c) => ({
        kind: "agent",
        id: `${s.id}/${c.id}`,
        workstream,
        label: c.name,
        harness: s.harness,
        // A sub-agent runs on its parent's model, at its effort, unless the harness says otherwise, which none does.
        model: s.model ?? null,
        effort: s.effort ?? null,
        agent: null,
        state: c.state,
        activity: c.description || stateLabel(c.state),
        since: c.since,
        // The spawn instant, never reset — what its elapsed counts from.
        started: c.started,
        workItem: s.work_item ?? null,
        goal: s.goal ?? null,
        gateId: null,
        terminalKey: tabOf.get(s.id)?.key ?? null,
        parent: s.id,
      })),
    ]),
  ];
}

/**
 * The rows a surface actually paints, with folded harnesses' sub-agents hidden.
 * The builder above always returns the whole tree — the one source of truth
 * the rail and the Workstreams panel share; this drops the sub-agent rows
 * whose parent session a person has collapsed. Terminals and top-level
 * sessions always stay; only rows with a `parent` are foldable.
 * @param {readonly WorkstreamSessionRow[]} rows the full list from {@link workstreamSessionRows}
 * @param {((sessionId: string) => boolean) | { has(id: string): boolean }} isCollapsed
 *   a session id predicate, or any Set-like with `has`
 * @returns {WorkstreamSessionRow[]}
 */
export function visibleSessionRows(rows, isCollapsed) {
  const folded = typeof isCollapsed === "function" ? isCollapsed : (id) => !!isCollapsed?.has?.(id);
  return (rows ?? []).filter((r) => !(r.parent && folded(r.parent)));
}

/**
 * @typedef {object} WorkstreamSessionRow
 * @property {"terminal"|"agent"} kind
 * @property {string} id
 * @property {string} workstream
 * @property {string} label
 * @property {string | null} harness
 * @property {string | null} [model] the model the session runs on, on an agent row
 * @property {string | null} [effort] the effort it runs at, on an agent row
 * @property {string | null} [agent]
 * @property {object} [state] the live `SessionState`, on an agent row
 * @property {string} [activity] the state's sentence
 * @property {number} [since]
 * @property {number | null} [started]
 * @property {string | null} [workItem]
 * @property {string | null} [goal]
 * @property {string | null} [gateId]
 * @property {object} [liveness] the PTY liveness, on a terminal row
 * @property {number | null} [openedAt]
 * @property {number | null} [exitedAt]
 * @property {number} [childCount] how many sub-agents nest under this session (agent rows only)
 * @property {string | null} [terminalKey] the terminal tab a reported session lives in (agent rows only)
 * @property {string | null} parent the session id a sub-agent nests under; null otherwise
 */
