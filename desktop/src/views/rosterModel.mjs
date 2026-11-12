/**
 * The Agents and Teams screens' own rules, out of the components so a test
 * can reach them: who an agent answers, where a session is attached, what a
 * team's roster says in one line and what of it a person put there, and the
 * whole-list body a member's removal writes back.
 *
 * The two core agents are added to every roster at read time by the node and
 * stripped from every write, so they are never stored and cannot be removed
 * (06 — Agents and teams); `CORE_AGENT_IDS` names them here as
 * `types.hand.ts` does.
 */

import { assigneeToWire } from "./_work/assigneeWire.mjs";
import { gateOf } from "../ui/sessionState.mjs";
import { t } from "../i18n/l10n.mjs";

/** The two core agents' ids, as `types.hand.ts` spells them. */
export const CORE_AGENT_IDS = Object.freeze(["general-agent", "workflow-agent"]);

/** Who an agent will act on instructions from, in words. @param {{respond?: string} | null | undefined} agent */
export function respondsTo(agent) {
  return agent?.respond === "owner_only"
    ? t("screens-roster-answers-only-messages-from-anyone-else")
    : t("screens-roster-answers-any-workspace-member");
}

/**
 * Where a session's ask is answered, the rail's rule (`gateOf`): only a
 * session waiting on a gate — a permission, a question, a gate the node
 * holds — has one, and it is answered in the Inbox, on the row the ask
 * lives under: its goal, else its conversation, else its workstream. `null`
 * when nothing waits on you; `{ item: null }` when the ask is the Inbox's
 * but no row of it is named.
 * @param {{state?: unknown, goal?: string | null, conversation?: string | null, workstream?: string | null} | null | undefined} session
 * @returns {{item: string | null} | null}
 */
export function answerOf(session) {
  if (!session || gateOf(session.state) === null) return null;
  return { item: session.goal || session.conversation || session.workstream || null };
}

/**
 * Whether a list-detail screen's detail sits **under** its roster rather
 * than beside it — the narrow layout, below the split — read from the two
 * columns' boxes: stacked when the detail starts left of the roster's right
 * edge. A pick then brings the detail into view; beside the roster it is
 * already there.
 * @param {{right: number} | null | undefined} roster @param {{left: number} | null | undefined} detail
 */
export function detailStacked(roster, detail) {
  if (!roster || !detail) return false;
  return detail.left < roster.right;
}

/** @param {object} m an assignee */
const isAgentMember = (m) => Boolean(m) && "agent" in m;

/** A member nobody put there: a core agent, added at read time. @param {object} m */
export const isImplicitMember = (m) => isAgentMember(m) && CORE_AGENT_IDS.includes(m.agent);

/** What somebody put on the team, as opposed to what belongs there. @param {{members?: readonly object[]} | null | undefined} team */
export function storedMembers(team) {
  return (team?.members ?? []).filter((m) => !isImplicitMember(m));
}

/** `3 agents · 1 person`, the one line a card has room for. @param {readonly object[] | null | undefined} members */
export function rosterLine(members) {
  const all = members ?? [];
  const agents = all.filter(isAgentMember).length;
  const humans = all.length - agents;
  return t("screens-roster-agents-and-people", { agents, people: humans });
}

/**
 * Whether a team may be addressed and assigned: one that was stood down
 * (`enabled: false`) takes no work and is addressed by nothing (06 — Agents
 * and teams), so no picker offers it. It is still *known*: an assignment
 * made before it was stood down is drawn with its name, and says so.
 * @param {{enabled?: boolean} | null | undefined} team
 */
export function teamTakesWork(team) {
  return team != null && team.enabled !== false;
}

/** The one line under a team in a picker: its roster, or that it was stood down. @param {{members?: readonly object[], enabled?: boolean}} team */
export function teamOptionLine(team) {
  return teamTakesWork(team) ? rosterLine(team.members) : t("screens-roster-stood-down-takes-no-work");
}

/**
 * The body a member's removal writes: the stored roster whole, without that
 * one member — never a half-changed list, never a core agent, since none is
 * stored to begin with.
 * @param {{members?: readonly object[]}} team @param {object} member
 * @returns {{members: object[]}}
 */
export function withoutMember(team, member) {
  const gone = assigneeToWire(member);
  return { members: storedMembers(team).filter((m) => assigneeToWire(m) !== gone) };
}

/**
 * How a member is named and pictured on a team card: an agent by its
 * definition when the roster holds one (its name, its pubkey as the avatar's
 * id, its photo), else as every picker names a member that is gone
 * ({@link goneMemberName}); a person through the workspace's
 * directory; a stray team member — a hand-edited file — by its id, so it is
 * seen and fixed rather than hidden.
 * @param {object} m @param {readonly {id: string, name: string, pubkey: string, photo?: object | null}[]} agents
 * @param {{nameOf: (pubkey: string) => string, photoOf: (pubkey: string) => object | null}} people
 * @returns {{label: string, avatarId: string, photo: object | null}}
 */
export function memberFace(m, agents, people) {
  if (isAgentMember(m)) {
    const def = (agents ?? []).find((a) => a.id === m.agent);
    return { label: def?.name ?? goneMemberName("agent", m.agent), avatarId: def?.pubkey ?? m.agent, photo: def?.photo ?? null };
  }
  if ("human" in m) return { label: people.nameOf(m.human), avatarId: m.human, photo: people.photoOf(m.human) };
  return { label: t("screens-roster-team-named", { team: m.team }), avatarId: m.team, photo: null };
}

/**
 * Whom a **roster** picker never offers, by id: the two core agents. They
 * are in every team and every room and stored in none, so a roster sent with
 * one is stripped on write — ticking it is a control that lies — and a goal
 * or a project assigned to one routes no work to it (06 — Agents and teams:
 * they take no unassigned work). A disabled agent *is* offered, marked
 * ({@link agentOptionLine}): a roster is a directory of who belongs, and an
 * agent stood down today is still on the team.
 * @param {readonly {id: string, origin?: string}[] | null | undefined} agents
 * @returns {string[]}
 */
export function rosterNotOffered(agents) {
  return (agents ?? []).filter((a) => a.origin === "core" || CORE_AGENT_IDS.includes(a.id)).map((a) => a.id);
}

/**
 * The one line under an agent in a picker: what it runs on — or, for an
 * agent that was disabled, that it is, before what it runs on. It is still
 * offered where a roster is built; nobody should put it there unknowing.
 * @param {{enabled?: boolean} | null | undefined} agent @param {string} line what it runs on
 */
export function agentOptionLine(agent, line) {
  return agent?.enabled === false ? t("screens-roster-agent-disabled-line", { line }) : line;
}

/**
 * How a member the workspace no longer has is named — on a team's card, in
 * its pane and in every picker's chip, one name: its kind and the tail of
 * its id. A raw id in one place and *Agent a1b2c3* in another read as two
 * different members.
 * @param {"agent" | "human" | "team"} kind @param {string} id
 */
export function goneMemberName(kind, id) {
  const tail = String(id ?? "").slice(-6);
  if (kind === "human") return t("screens-roster-gone-person", { tail });
  if (kind === "team") return t("screens-roster-gone-team", { tail });
  return t("screens-roster-gone-agent", { tail });
}

/**
 * The switch a record offers: what pressing it is called, what it sends, and
 * what it reads while the node answers — *Disable* / *Enable* for an agent,
 * *Stand down* / *Bring back* for a team. It sends `enabled` **alone**: the
 * node changes that one field on the record as it stands, so a switch
 * pressed on a card never writes back a name or a roster the card read a
 * while ago. A core agent has none — it cannot be disabled, and a control
 * that exists only to refuse is worse than no control.
 * @param {"agent" | "team"} kind
 * @param {{id: string, enabled?: boolean, origin?: unknown} | null | undefined} record
 * @returns {{on: boolean, label: string, doing: string, done: string, body: {enabled: boolean}} | null}
 */
export function switchOf(kind, record) {
  if (!record) return null;
  if (kind === "agent" && (record.origin === "core" || CORE_AGENT_IDS.includes(record.id))) return null;
  const on = record.enabled !== false;
  const name = record.name ?? record.id;
  const body = { enabled: !on };
  if (kind === "agent") {
    return on
      ? { on, label: t("screens-roster-disable"), doing: t("screens-roster-disabling"), done: t("screens-roster-agent-disabled-done", { name }), body }
      : { on, label: t("screens-roster-enable"), doing: t("screens-roster-enabling"), done: t("screens-roster-agent-enabled-done", { name }), body };
  }
  return on
    ? { on, label: t("screens-roster-stand-down"), doing: t("screens-roster-standing-down"), done: t("screens-roster-team-stood-down-done", { name }), body }
    : { on, label: t("screens-roster-bring-back"), doing: t("screens-roster-bringing-back"), done: t("screens-roster-team-brought-back-done", { name }), body };
}

/**
 * Whether a delete was asked of **this** record. The ask is kept as the id
 * it was made of, never as a bare *yes*: a record removed elsewhere while
 * its dialog stood open takes the ask with it, and the next record picked
 * does not open a delete nobody asked for.
 * @param {string | null | undefined} asked the id a delete was asked of
 * @param {{id: string} | null | undefined} record the record in hand
 */
export function asksDelete(asked, record) {
  return Boolean(asked) && record != null && record.id === asked;
}

/**
 * The team a pane draws: the list's copy — read again on every frame about
 * a team — unless a write of this pane was answered since, in which case
 * what the node answered stands until the list catches up. The answer
 * carries the stored roster alone, so the members nobody stored — the core
 * agents, added at read time — are kept from the list's copy, after it.
 * The next change is then made from the record the node last answered,
 * never from one the screen read before the write.
 * @param {{members?: readonly object[]}} listed @param {{members?: readonly object[]} | null | undefined} answered
 */
export function shownTeam(listed, answered) {
  if (!answered) return listed;
  return { ...answered, members: [...storedMembers(answered), ...(listed?.members ?? []).filter(isImplicitMember)] };
}

/**
 * Where a waiting session is answered, and so where its *Answer* goes: a
 * gate on its Inbox row ({@link answerOf}); anything else that waits on the
 * person — a question with no gate — **where it was asked**, the place the
 * session is at work in. A sign-in is nobody's to answer here, and a session
 * that waits on nothing has no door.
 * @param {{state?: {state?: string, on?: {on?: string}}, goal?: string | null, conversation?: string | null, workstream?: string | null} | null | undefined} session
 * @param {object | null | undefined} door the place the session's origin opens
 * @returns {{inbox: string | null} | {door: object} | null}
 */
export function answerPlace(session, door) {
  const state = session?.state;
  if (!state || state.state !== "waiting" || state.on?.on === "auth") return null;
  const ask = answerOf(session);
  if (ask) return { inbox: ask.item };
  return door ? { door } : null;
}

/**
 * The sessions an abort is still on its way for: asked, and still on the
 * roster as something that can be stopped. One that left, or reads ended,
 * is done with; the button of one still here stays latched, so a second
 * press cannot ask the node twice.
 * @param {readonly string[]} asked ids an abort was asked of
 * @param {readonly {id: string}[]} stoppable the sessions that can still be stopped
 * @returns {string[]}
 */
export function stillStopping(asked, stoppable) {
  const here = new Set((stoppable ?? []).map((s) => s.id));
  return (asked ?? []).filter((id) => here.has(id));
}

/**
 * Whether a `conversation` frame says a definition's record moved — any
 * agent, team, skill or channel written or, with `removed`, taken away. The
 * catalog reads again on it: every one of its rows says *installed* or not.
 * @param {{snapshot?: boolean} | null | undefined} frame
 */
export function definitionMoved(frame) {
  return frame?.snapshot === true;
}

/**
 * Whether a `conversation` frame says a record was **removed** — the frame
 * the node sends for an agent, a team or a skill that is gone
 * (`StoreEvent::DefinitionRemoved`). Every list that follows snapshot
 * frames reads again on it as on a write, and finds the record gone.
 * @param {{snapshot?: boolean, removed?: boolean} | null | undefined} frame
 */
export function definitionRemoved(frame) {
  return frame?.snapshot === true && frame.removed === true;
}

/**
 * Whether an engine fact moves the MCP registry as a screen holds it: a
 * server was probed, here or anywhere. A server has no record frame of its
 * own, so this — with a reconnect, and a change made in this window
 * ({@link registryChanges}) — is how an open registry follows.
 * @param {{type?: string} | null | undefined} payload
 */
export function mcpMoved(payload) {
  return payload?.type === "mcp_probed";
}

/**
 * A word passed round one window: *the MCP registry changed here*. The
 * Settings panel says it after it made, edited or removed a server; every
 * open registry — the Agents screen's, the editor's — reads again. Nothing
 * crosses the wire: another window follows on its next probe or reconnect.
 * @returns {{changed: () => void, listen: (heard: () => void) => () => void}}
 */
export function registryChanges() {
  const listeners = new Set();
  return {
    changed() {
      for (const heard of [...listeners]) heard();
    },
    listen(heard) {
      listeners.add(heard);
      return () => listeners.delete(heard);
    },
  };
}

/**
 * The catalog rows an install or an update is on its way for, with one more
 * or one fewer: a mark a row, never one slot for the whole list — a second
 * row pressed while the first is still out used to take the first row's
 * mark, and release it when it finished first.
 * @param {readonly string[]} busy @param {string} key @param {boolean} on
 * @returns {string[]}
 */
export function busyRows(busy, key, on) {
  const rest = (busy ?? []).filter((k) => k !== key);
  return on ? [...rest, key] : rest;
}

/** The kind of event a team's record is — `bisa_core::kind::KIND_TEAM`. */
export const KIND_TEAM = 33408;

/** The kind of event a skill's record is — `bisa_core::kind::KIND_SKILL`. */
export const KIND_SKILL = 33411;

/** Whether a `conversation` frame says a skill of the library moved — written, edited, removed, by whoever did it. */
export function skillMoved(frame) {
  return frame?.snapshot === true && frame.kind === KIND_SKILL;
}

/**
 * Whether a `conversation` frame says a team's record moved — made, edited,
 * stood down, deleted — by whoever did it: this window, the command line,
 * another node. The Teams screen reads its list again on it, so a roster
 * changed elsewhere is never left standing on screen.
 * @param {{kind?: number, snapshot?: boolean} | null | undefined} frame
 */
export function teamMoved(frame) {
  return frame?.snapshot === true && frame.kind === KIND_TEAM;
}

/**
 * What a roster screen says of an id its list does not hold, from the single
 * read of that record (06 — Agents and teams): a record the store cannot
 * read is **left out of the list**, and the single read of it answers the
 * store's own sentence naming the file — so the screen asks, and says what
 * the node said:
 *
 * - `reading` — the read is on its way: nothing is claimed yet;
 * - `here` — the node has it: the list was a moment behind, and is read again;
 * - `gone` — the node says there is no such thing (its 404): the screen's own
 *   note, *nothing answers to this id*;
 * - `unreadable` — anything else: the node's sentence, word for word — a
 *   record cut short is said by the file it is in, never as *not found*.
 * @param {{data?: unknown, error?: string | null, missing?: boolean, loading?: boolean}} read
 * @returns {{state: "reading"} | {state: "here"} | {state: "gone"} | {state: "unreadable", sentence: string}}
 */
export function absentRecord(read) {
  if (read.data !== null && read.data !== undefined) return { state: "here" };
  if (read.missing) return { state: "gone" };
  if (read.error) return { state: "unreadable", sentence: read.error };
  return { state: "reading" };
}

