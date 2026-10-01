/**
 * Who is in a conversation, and who can be addressed in it.
 *
 * Agents are principals like anyone else, so one directory serves the mention
 * picker, the address tray, the DM picker and every author label. It reads
 * from the shell's workspace store rather than fetching — the store already
 * has members and agents loaded, and a conversation that refetched the
 * directory on every open is how the old screens lost their scroll position.
 *
 * A channel's roster shows up here twice, and neither time as a subscription:
 * it orders the `@`-picker, and it puts the channel's own handle in it. Nothing
 * here makes a rostered agent answer — that still takes addressing it.
 *
 * The rules about *who* — core agent out of every picker and in the resolution
 * set, disabled agents out of every addressing surface — live in
 * `addressModel.mjs`, and what a row *says* — a roster id whose agent is gone,
 * a harness this node lacks, the role line — in `participantsModel.mjs`, where
 * `node --test` can reach them. They used to live inline and disagree with each
 * other: the tray offered a disabled agent that the `@`-picker twelve lines
 * below had already filtered out.
 */

import { useMemo } from "react";
import type { AgentDef, ChannelDef } from "../../types";
import { audiencePrincipals, rosterAgents } from "../../types";
import type { AgentCandidate, Mentionable } from "../../ui";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { useInstalledHarnesses } from "../../shell/useHarnesses";
import { addressable, canAnswer, orderByRoster, reachableIn, rosterable, teamMentionables } from "./addressModel.mjs";
import { candidateOf } from "./participantsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

interface Participants {
  /**
   * Everyone addressable with `@` — the channel handle, members and agents.
   *
   * Addressable, which is not the same as offered: the core agent is in here
   * with `suggest: false`. This list is also what turns a typed name into an
   * outgoing mention, so leaving it out to stop suggesting it would make
   * `@General Agent` resolve to nothing at all.
   */
  mentionables: Mentionable[];
  /**
   * Agents this conversation *offers* to address; for a DM, the ones in the
   * audience. Neither the core agent nor a disabled one is among them — see
   * `addressModel.mjs`.
   */
  agents: AgentDef[];
  /** The same agents as picker rows — what the address tray offers. */
  addressCandidates: AgentCandidate[];
  /** Agents actually in this conversation's audience (a DM's counterpart). */
  inAudience: AgentDef[];
  /** Human pubkeys in the audience, minus you. */
  humans: string[];
}

/**
 * Every agent this workspace can put on a **roster**, as picker rows.
 *
 * A roster is a directory, so a disabled agent is offered and marked rather
 * than dropped. The core agent is not offered at all: the node stores it zero
 * times, so a tick for it is a control that lies.
 */
export function useRosterCandidates(): AgentCandidate[] {
  const ws = useWorkspace();
  const installed = useInstalledHarnesses();
  return useMemo(
    () => rosterable(ws.agents).map((a) => ({ ...candidateOf(a, a.id, installed), group: t("studio-participants-agents") })),
    [ws.agents, installed],
  );
}

/**
 * The people a channel's roster may list (14-collaboration): the hosted
 * members, keyed by pubkey. A guest reaches a channel only when listed;
 * an admin or a member reaches every standing channel, listed or not, so
 * the picker says which is which.
 */
export function usePeopleRosterCandidates(): AgentCandidate[] {
  const ws = useWorkspace();
  return useMemo(
    () =>
      ws.members
        .filter((m) => m.pubkey !== ws.me && m.role !== "owner")
        .map((m) => ({
          id: m.pubkey,
          name: m.label ?? `${m.pubkey.slice(0, 8)}…`,
          kind: "human" as const,
          description: m.role === "guest" ? t("studio-participants-guest-reaches-channel-only-when-listed") : t("studio-participants-reaches-every-standing-channel", { role: m.role }),
          group: t("studio-inbox-people"),
        })),
    [ws.members, ws.me],
  );
}

/**
 * Everyone a new conversation can be opened with, keyed by pubkey.
 *
 * Grouped, agents first, because an agent is the thing people come here to
 * talk to; the workspace's own members are the smaller and better-known list.
 */
export function useDirectMessageCandidates(): AgentCandidate[] {
  const ws = useWorkspace();
  const installed = useInstalledHarnesses();
  return useMemo(() => {
    const agents = addressable(ws.agents).map((a) => ({
      ...candidateOf(a, a.pubkey, installed),
      group: t("studio-participants-agents"),
    }));
    const people: AgentCandidate[] = ws.members
      .filter((m) => m.pubkey !== ws.me)
      .map((m) => ({
        id: m.pubkey,
        name: m.label ?? `${m.pubkey.slice(0, 8)}…`,
        kind: "human" as const,
        description: m.role,
        group: t("studio-inbox-people"),
      }));
    return [...agents, ...people];
  }, [ws.agents, ws.members, ws.me, installed]);
}

/**
 * A channel with an empty audience is the whole workspace, so *any* agent can
 * be pulled into it by addressing them. A DM has an explicit audience, and
 * the agents in it are the ones already listening.
 */
export function useParticipants(channel?: ChannelDef | null, kind?: string | null): Participants {
  const ws = useWorkspace();
  const installed = useInstalledHarnesses();

  return useMemo(() => {
    const audience = audiencePrincipals(channel);
    const inAudience = audience.length
      ? ws.agents.filter((a) => audience.includes(a.pubkey))
      : [];
    const agentPubkeys = new Set(ws.agents.map((a) => a.pubkey));
    const humans = audience.filter((p) => !agentPubkeys.has(p) && p !== ws.me);

    // Rostered agents lead, in the order the roster names them. That ordering
    // is most of what a roster buys: in `#engineering` the engineers should be
    // the first six the picker offers, not whoever the workspace listed first.
    const roster = rosterAgents(channel);
    const ordered = orderByRoster(ws.agents, roster);

    // What gets *offered*: the core agent is dropped because it is in every
    // room and answers anything addressed to nobody, and a disabled one
    // because addressing it posts a mention nothing will ever answer. Both
    // lists below are built from this one, so the tray and the `@`-picker can
    // no longer disagree about who is pickable.
    const agents = addressable(ordered);

    const mentionables: Mentionable[] = [
      // The channel handle only exists inside a standing channel: a DM has no
      // roster to expand, and the id would address nothing.
      ...(channel && channel.kind === "standing" && roster.length > 0
        ? [
            {
              id: channel.id,
              name: channel.name,
              kind: "channel" as const,
              description: t("studio-participants-everyone-channel-s-roster-agent-agents", { roster: roster.length }),
            },
          ]
        : []),
      ...ws.members
        .filter((m) => m.pubkey !== ws.me)
        .map((m) => ({
          id: m.pubkey,
          name: m.label ?? `${m.pubkey.slice(0, 8)}…`,
          kind: "human" as const,
          description: m.role,
        })),
      // A disabled agent is out of the *directory*, not merely out of the
      // dropdown: resolving its name would put a `p` tag on an agent that
      // will never take a turn, and — because the message then addressed
      // somebody — triage would not answer either. Nothing at all would.
      //
      // The core agent is the opposite case and stays, hidden. It is what
      // turns `@General Agent` into a token at all: an entry absent from
      // this list produces no mention, no error, and a message you believe
      // you addressed.
      //
      // Except the Workflow Agent on a workstream (`reachableIn`): a goal's
      // agent, which the engine would not wake there anyway — so on a
      // checkout `@Workflow Agent` is no token, and the message you typed
      // reaches whoever triage names instead of nobody.
      ...ordered
        .filter(canAnswer)
        .filter((a) => reachableIn(a, kind))
        .map((a) => ({
          ...candidateOf(a, a.pubkey, installed),
          suggest: agents.includes(a),
        })),
      // Teams, in every kind: one handle the store expands to its enabled
      // members' pubkeys at post time — the docs' promise that team handles
      // work here as everywhere, kept by the picker too.
      ...teamMentionables(ws.teams),
    ];

    const addressCandidates = agents.map((a) => candidateOf(a, a.pubkey, installed));

    return { mentionables, agents, addressCandidates, inAudience, humans };
  }, [channel, kind, installed, ws.agents, ws.me, ws.members, ws.teams]);
}
