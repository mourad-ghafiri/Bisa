/**
 * The Agent panel's addressing (ide/09), as facts.
 *
 * A conversation's turns are `conversationPaneModel.sessionsOf`; the rows of
 * a checkout are the rail's (`workstreamSessionsModel`). What is left here is
 * what the panel's composer decides on its own: who an unaddressed message
 * reaches.
 */

import { defaultAddresseeIn, reachableIn } from "../_studio/addressModel.mjs";

/**
 * The agent an unaddressed message reaches, as the composer's chip draws it:
 * the conversation's own agent when it has one (`defaultAddresseeIn` — the
 * Workflow Agent in a conversation about a workflow), else the default the
 * setting names (`agents.default`), else the general agent — the engine's
 * own order (`default_agent`), so the chip never promises the wrong agent.
 * An agent that is disabled, unknown, or the Workflow Agent where the
 * conversation is about a checkout (`reachableIn`) falls through exactly as
 * the engine's does.
 * @param {readonly {id: string, name?: string, enabled?: boolean}[]} agents
 * @param {string | null | undefined} defaultId
 * @param {string} generalId
 * @param {string} [originKind] what the conversation is about; a checkout's by default
 */
export function addressee(agents, defaultId, generalId, originKind = "workstream") {
  const usable = (id) => (agents ?? []).find((a) => a.id === id && a.enabled !== false && reachableIn(a, originKind)) ?? null;
  return usable(defaultAddresseeIn(originKind)) ?? usable(defaultId) ?? usable(generalId) ?? null;
}
