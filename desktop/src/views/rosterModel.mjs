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
 * Where a running session is attached — its goal, its run of the workspace,
 * its work item, its conversation — with a route to go there when there is
 * one; a kind's word when it is attached to nothing named. A goal reads by
 * its title when `titleOf` knows it, else — a goal the window has not read,
 * or one with no title — by its id's tail, as everything else does.
 * @param {{goal?: string | null, run?: string | null, work_item?: string | null, conversation?: string | null, kind?: string} | null | undefined} session
 * @param {(goal: string) => string | null | undefined} [titleOf]
 * @returns {{label: string, route: {name: string, id: string} | null}}
 */
export function attachedTo(session, titleOf) {
  const s = session ?? {};
  if (s.goal) {
    const title = titleOf?.(s.goal)?.trim();
    return { label: title || t("screens-roster-goal-tail", { tail: s.goal.slice(-6) }), route: { name: "goal", id: s.goal } };
  }
  // A run of the workspace's worker: no goal holds it, its run's page does.
  if (s.run) return { label: t("screens-roster-run-tail", { tail: s.run.slice(-6) }), route: { name: "run", id: s.run } };
  if (s.work_item) return { label: t("screens-roster-work-item", { work_item: s.work_item.slice(-6) }), route: null };
  if (s.conversation) return { label: t("screens-roster-a-conversation"), route: { name: "conversation", id: s.conversation } };
  const word = { conversation: t("screens-roster-channel-goal-s-thread"), note: t("screens-roster-a-note"), terminal: t("screens-roster-a-terminal") }[s.kind ?? ""];
  return { label: word ?? "—", route: null };
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
 * id, its photo), else by its id; a person through the workspace's
 * directory; a stray team member — a hand-edited file — by its id, so it is
 * seen and fixed rather than hidden.
 * @param {object} m @param {readonly {id: string, name: string, pubkey: string, photo?: object | null}[]} agents
 * @param {{nameOf: (pubkey: string) => string, photoOf: (pubkey: string) => object | null}} people
 * @returns {{label: string, avatarId: string, photo: object | null}}
 */
export function memberFace(m, agents, people) {
  if (isAgentMember(m)) {
    const def = (agents ?? []).find((a) => a.id === m.agent);
    return { label: def?.name ?? m.agent, avatarId: def?.pubkey ?? m.agent, photo: def?.photo ?? null };
  }
  if ("human" in m) return { label: people.nameOf(m.human), avatarId: m.human, photo: people.photoOf(m.human) };
  return { label: t("screens-roster-team-named", { team: m.team }), avatarId: m.team, photo: null };
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

