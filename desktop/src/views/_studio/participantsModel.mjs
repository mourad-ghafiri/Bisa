/**
 * Who a picker offers, as facts `node --test` can reach: a channel's roster
 * resolved against the workspace's agents, why an agent may not answer, and
 * one agent as a picker row. The hooks in `participants.ts` read the stores
 * and memoise; every rule about the rows is here, so the tray, the `@`-picker
 * and the roster editor cannot disagree about what a row says.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * Resolve a channel's roster against the workspace's agents.
 *
 * A roster is a list of agent definition ids, and an id can outlive the agent
 * it named — the definition was deleted, or this node has not synced it yet.
 * That entry is kept and marked rather than dropped: silently shortening the
 * roster would make the channel look correctly configured when it is not.
 * @template {{ id: string; name: string }} T
 * @param {readonly string[]} ids
 * @param {readonly T[]} agents
 * @returns {{ id: string; name: string; agent: T | null }[]}
 */
export function resolveRoster(ids, agents) {
  return ids.map((id) => {
    const agent = agents.find((a) => a.id === id) ?? null;
    return { id, name: agent?.name ?? id, agent };
  });
}

/**
 * Why this agent may not answer, or null.
 *
 * Two different facts: a disabled agent, and an agent whose harness is not
 * installed on *this* node — which looked identical to a working one in every
 * picker in the app. It is marked rather than hidden, because the harness is a
 * fact about this machine — a collaborator's node may well have it, and the
 * agent is still the right one to put on a roster. `installed` as `null` is
 * "not known yet" and marks nothing: flashing "no harness" on every agent for
 * the eighty milliseconds before `/harnesses` lands would invent a problem the
 * workspace does not have.
 * @param {{ enabled?: boolean; harness: string }} agent
 * @param {ReadonlySet<string> | null} installed
 * @returns {string | null}
 */
export function warningFor(agent, installed) {
  if (agent.enabled === false) return t("studio-participants-disabled");
  if (installed && !installed.has(agent.harness)) return t("studio-participants-not-installed", { harness: agent.harness });
  return null;
}

/**
 * What a row says an agent is for: its description, else the first line of
 * its system prompt — what the agent was written to be. An agent with neither
 * says nothing (`null`), never an empty line.
 * @param {{ description?: string | null; system_prompt: string }} agent
 * @returns {string | null}
 */
export function roleLineOf(agent) {
  if (agent.description) return agent.description;
  const first = agent.system_prompt.split("\n")[0] ?? "";
  return first.length > 0 ? first : null;
}

/**
 * One agent as a picker row. `id` is the caller's choice of identifier — a
 * definition id for a roster, a pubkey for an address.
 * @param {{ id: string; name: string; description?: string | null; system_prompt: string; harness: string; tags?: string[]; pubkey: string; photo?: unknown; enabled?: boolean }} agent
 * @param {string} id
 * @param {ReadonlySet<string> | null} installed
 */
export function candidateOf(agent, id, installed) {
  return {
    id,
    name: agent.name,
    kind: /** @type {const} */ ("agent"),
    description: roleLineOf(agent),
    harness: agent.harness,
    tags: agent.tags,
    avatar: agent.pubkey,
    photo: agent.photo,
    warning: warningFor(agent, installed),
  };
}
