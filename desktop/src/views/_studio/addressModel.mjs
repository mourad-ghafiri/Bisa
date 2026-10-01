/**
 * Who a conversation offers to address, and what it remembers you chose.
 *
 * Three separate lies used to live in these few rules, and every one of them
 * was silent:
 *
 * 1. **The tray offered a disabled agent and the `@`-picker did not.** One
 *    list filtered on `enabled`, the other did not filter at all, so the same
 *    agent was pickable in one control and invisible in the other a centimetre
 *    away. Addressing it posts a mention nothing will ever answer — which is
 *    exactly the reason `docs/architecture/05-channels.md` gives for dropping disabled agents
 *    from a channel-handle expansion.
 * 2. **"Your choice is the truth" was false for the choice "nobody".** An
 *    empty stored list was read as "never chosen" and re-seeded, so removing
 *    the last chip, navigating away and coming back silently re-addressed the
 *    agent you had just removed. *Never chosen* and *chose nobody* are
 *    different answers and are now different values.
 * 3. **An addressee could be invisible.** Chips were drawn by filtering the
 *    candidate list, so a pubkey persisted while one channel was open and read
 *    back where that agent is not a candidate was still merged into the
 *    outgoing mentions while drawing no chip at all. Every addressed pubkey
 *    now draws a chip, resolvable or not — the same rule `participants.ts`
 *    already holds for an unresolvable roster id.
 *
 * Plain `.mjs` with a `.d.mts` beside it: there is no jsdom here, so the way
 * a rule gets a test is by not living inside a component.
 */

import { t as tr } from "../../i18n/l10n.mjs";

/**
 * The platform's own agent, by the one field that carries it.
 *
 * Mirrors `isCoreAgent` in `types.hand.ts`, structurally rather than by
 * import, because a `.ts` file cannot be imported by `node --test`.
 */
export function isCore(a) {
  return a?.origin === "core";
}

/** A disabled agent is stored and visible, and it will never take a turn. */
export function canAnswer(a) {
  return a?.enabled !== false;
}

/**
 * The Workflow Agent's id, as the core spells it (`AgentId::WORKFLOW`;
 * `WORKFLOW_AGENT_ID` in `types.hand.ts`, mirrored here for the same reason
 * `isCore` mirrors `isCoreAgent`).
 */
export const WORKFLOW_AGENT_ID = "workflow-agent";

/** The General Agent's id, as the core spells it (`AgentId::GENERAL`). */
export const GENERAL_AGENT_ID = "general-agent";

/** The platform's workflow designer — a goal's agent, never a workstream's. */
export function isWorkflow(a) {
  return a?.id === WORKFLOW_AGENT_ID;
}

/**
 * Whether an agent can be reached at all from a conversation of this kind.
 *
 * The Workflow Agent designs workflows and shapes goals; a conversation about a checkout —
 * the IDE's conversation on a checkout — has neither for it to act on, so
 * there it is offered nowhere, resolves to nothing and is never the triage
 * target. The engine holds the same rule (`conversation.rs::dispatch`), so this
 * is the surface agreeing with the system, not a UI-only opinion. Goals and
 * channels keep it: `@Workflow Agent` there is how a workflow gets asked for.
 * @param {{id?: string} | null | undefined} a
 * @param {string | null | undefined} kind `"workstream"` · `"goal"` · `"channel"` · `"dm"`
 */
export function reachableIn(a, kind) {
  return !(NO_WORKFLOW_AGENT_KINDS.includes(kind) && isWorkflow(a));
}

/**
 * The kinds whose conversations run in a checkout: a workstream's, a
 * project's (13 — Conversations).
 */
export const CHECKOUT_KINDS = Object.freeze(["workstream", "project"]);

/**
 * Where the Workflow Agent is never reachable: a conversation about a
 * checkout, one about a drawing (19) and one about a note. It designs
 * workflows and shapes goals; a checkout has neither, a picture has nothing
 * to shape and a scratchpad is somebody's own — the engine holds the same
 * rule (`ConversationOrigin::reaches_workflow_agent`).
 * A channel, a direct channel, a goal's thread and a conversation about a
 * goal, a workflow, the workspace or the node keep it.
 */
const NO_WORKFLOW_AGENT_KINDS = Object.freeze([...CHECKOUT_KINDS, "drawing", "note"]);

/** {@link addressable}, narrowed to what a conversation of this kind can reach. */
export function addressableIn(agents, kind) {
  return addressable(agents).filter((a) => reachableIn(a, kind));
}

/**
 * The teams a conversation offers under `@`: every enabled one, its id as
 * the token — the store expands a team handle to its enabled agents' pubkeys
 * at post time (`resolve_mentions`), in every scope. A disabled team is out
 * for the reason a disabled agent is: addressing it is refused, and a
 * picker must not offer what the node will not take.
 * @param {readonly {id: string, name: string, purpose?: string | null, enabled?: boolean, members?: readonly unknown[]}[] | null | undefined} teams
 */
export function teamMentionables(teams) {
  return (teams ?? [])
    .filter((t) => t.enabled !== false)
    .map((t) => {
      const n = t.members?.length ?? 0;
      return {
        id: t.id,
        name: t.name,
        kind: "team",
        description: t.purpose || tr("studio-address-team-member-members-one-handle-every", { n }),
        suggest: true,
      };
    });
}

/**
 * The agents an addressing surface offers — the `@`-picker and the tray.
 *
 * The core agent is out because it is an implicit member of every room and
 * answers anything addressed to nobody, so a row for it suggests a choice
 * there is never a reason to make. That is only safe because triage exists:
 * since M13 an unaddressed message in a standing channel reaches it anyway.
 * A disabled agent is out because addressing it does nothing.
 */
export function addressable(agents) {
  return (agents ?? []).filter((a) => !isCore(a) && canAnswer(a));
}

/**
 * The agents a **roster** picker offers, which is a different question.
 *
 * The core agent is excluded because the node stores it zero times: a roster
 * submitted containing it is stripped on write, so offering it is a control
 * that lies — you tick it, you save, and it is not there when you come back.
 * A disabled agent *is* offered: a roster is a directory of who belongs in a
 * room, not an act of addressing, and an agent disabled today is still on the
 * team. It is marked instead.
 */
export function rosterable(agents) {
  return (agents ?? []).filter((a) => !isCore(a));
}

/**
 * The platform's own agents, in the order the core spells them — the
 * General Agent, then the Workflow Agent — for the Agents page's pinned
 * block: lifted out of the roster before any filter sees it, which is the
 * only way to guarantee no filter can hide them. The rest of the roster is
 * `rosterable`, the same split by the same field.
 */
export function coreAgents(agents) {
  const order = [GENERAL_AGENT_ID, WORKFLOW_AGENT_ID];
  return (agents ?? [])
    .filter(isCore)
    .sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
}

/**
 * Rostered agents first, in the order the roster names them, then the rest.
 *
 * That ordering is most of what a roster buys: in `#engineering` the
 * engineers should be the first six the picker offers, not whoever the
 * workspace happened to list first. A roster id naming an agent this node
 * does not have simply contributes no row here — `resolveRoster` is where an
 * unresolved id is shown and marked.
 */
export function orderByRoster(agents, rosterIds) {
  const all = agents ?? [];
  const rostered = (rosterIds ?? []).flatMap((id) => {
    const found = all.find((a) => a.id === id);
    return found ? [found] : [];
  });
  const seen = new Set(rostered.map((a) => a.id));
  return [...rostered, ...all.filter((a) => !seen.has(a.id))];
}

/**
 * What localStorage holds for a scope: a chosen list, or nothing chosen yet.
 *
 * `null` is *never chosen*; `[]` is *chose nobody*. Anything unparseable is
 * treated as never chosen — a corrupted preference should fall back to the
 * seed, not to silence.
 */
/** Where a reader's choice of addressees is kept, per conversation scope. @param {string} scope */
export function storedAddressKey(scope) {
  return `bisa:address:${scope}`;
}

export function parseStored(raw) {
  if (raw === null || raw === undefined) return null;
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((p) => typeof p === "string") : null;
  } catch {
    return null;
  }
}

/**
 * The tray's opening state for a scope.
 *
 * `seed` is the agents already in the conversation — a DM's counterpart —
 * because a DM with an agent that did not reach the agent is just a diary.
 * It applies exactly once, when nothing has ever been chosen here.
 */
export function initialAddressed(stored, seed) {
  return stored === null || stored === undefined ? [...(seed ?? [])] : [...stored];
}

/**
 * One chip per addressed pubkey, resolved against this scope's candidates.
 *
 * Never a filter over the candidates: a pubkey the send will carry and the
 * tray will not draw is an addressee you cannot see and cannot remove. An
 * entry with no `agent` is shown and marked, exactly as an unresolvable
 * roster id is.
 */
export function addressChips(pubkeys, candidates) {
  const byKey = new Map((candidates ?? []).map((a) => [a.pubkey, a]));
  return (pubkeys ?? []).map((pubkey) => ({ pubkey, agent: byKey.get(pubkey) ?? null }));
}
