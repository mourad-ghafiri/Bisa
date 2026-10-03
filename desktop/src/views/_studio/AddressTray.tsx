/**
 * The address tray: which agents this message will reach.
 *
 * Addressing an agent is how you get an answer out of it, and that was
 * completely invisible — you had to know to type `@`. Here the agents you are
 * talking to sit above the composer as chips, persist per conversation, and
 * are attached to every message you send until you remove them.
 *
 * The tray is also the only honest place to say what a roster does not do. A
 * channel's roster is a directory; **this** list is the subscription, and it
 * lasts exactly one message unless you leave it set. So the tray never
 * pre-fills itself from a roster — only from an audience, where the agent is
 * already the other end of the conversation.
 *
 * Which is exactly why the core agent is not among the `candidates` it is
 * handed: subscribing to the one agent that answers everything addressed to
 * nobody is a subscription with nothing to buy. `participants.ts` filters it
 * out, and keeps it addressable by name.
 *
 * Two things it now promises that it previously only appeared to:
 *
 * - **Every addressee draws a chip.** Chips used to be a filter over the
 *   candidate list, so a pubkey remembered in one scope and read back where
 *   that agent is not a candidate was merged into the outgoing mentions while
 *   drawing nothing — an addressee you could not see and could not remove.
 * - **"Nobody" is a choice.** See {@link useAddressed}.
 */

import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import type { AgentDef } from "../../types";
import {
  AgentChip,
  AgentPicker,
  ICON,
  Popover,
  Tooltip,
  WorkingDot,
  type AgentCandidate,
} from "../../ui";
import { addressChips, initialAddressed, parseStored, storedAddressKey as key } from "./addressModel.mjs";
import { warningFor } from "./participantsModel.mjs";
import { readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** `null` is *never chosen here*; `[]` is *chose nobody*. See below. */
function load(scope: string): string[] | null {
  return readPref(webStorage(), key(scope), (raw) => parseStored(raw), null);
}

interface Addressed {
  /** Agent pubkeys this message is addressed to. */
  pubkeys: string[];
  set: (next: string[]) => void;
  toggle: (pubkey: string) => void;
}

/**
 * `seed` is the agents already in the conversation (a DM's counterpart):
 * they are addressed by default, because a DM with an agent that did not
 * reach the agent is just a diary.
 *
 * The seed applies **once per scope, and only when nothing has ever been
 * chosen here**. It used to re-apply whenever the stored list was empty,
 * which made the choice "nobody" unrepresentable: removing the last chip,
 * navigating away and coming back silently re-addressed the agent you had
 * just removed, and the message went out to it. Absent and empty are
 * different answers, so they are now different values.
 */
export function useAddressed(scope: string, seed: AgentDef[], unreachable: readonly string[] = []): Addressed {
  // Keyed on the pubkeys themselves, not on the array: callers rebuild `seed`
  // on every render, and a memo on its identity re-ran the effect below each
  // time — a state change per render, which is the update-depth loop.
  const seedKey = seed.map((a) => a.pubkey).join(",");
  const seedKeys = useMemo(() => (seedKey ? seedKey.split(",") : []), [seedKey]);
  // The same reasoning for `unreachable`: pubkeys this scope's kind can never
  // reach (the Workflow Agent on a workstream) — dropped on read and on write,
  // so a chip remembered elsewhere never rides into a send here. This is the
  // one exception to "every addressed pubkey draws a chip": an addressee the
  // engine would refuse is not an addressee, and drawing it would promise a
  // reply that cannot come.
  const unreachableKey = unreachable.join(",");
  const barred = useMemo(() => new Set(unreachableKey ? unreachableKey.split(",") : []), [unreachableKey]);
  const reachable = useCallback((list: string[]) => list.filter((p) => !barred.has(p)), [barred]);
  const [pubkeys, setPubkeys] = useState<string[]>([]);

  useEffect(() => {
    setPubkeys(reachable(initialAddressed(load(scope), seedKeys)));
  }, [scope, seedKeys, reachable]);

  const set = useCallback(
    (next: string[]) => {
      const kept = reachable(next);
      setPubkeys(kept);
      // Written even when empty: the empty array is what records that this
      // reader chose nobody, and it is the only thing standing between that
      // choice and the seed above. A preference, not state: a full quota
      // never breaks sending (`writePref` never throws).
      writePref(webStorage(), key(scope), kept);
    },
    [scope, reachable],
  );

  const toggle = useCallback(
    (pubkey: string) =>
      setPubkeys((prev) => {
        if (barred.has(pubkey)) return prev;
        const next = prev.includes(pubkey)
          ? prev.filter((p) => p !== pubkey)
          : [...prev, pubkey];
        writePref(webStorage(), key(scope), next);
        return next;
      }),
    [scope, barred],
  );

  return { pubkeys, set, toggle };
}

export function AddressTray({
  addressed,
  candidates,
  known,
  working,
  lead,
}: {
  addressed: Addressed;
  /** Agents this conversation offers — core agent and disabled ones already out. */
  candidates: AgentCandidate[];
  /**
   * Every agent the workspace has, for drawing chips. Wider than `candidates`
   * on purpose: an addressee that is no longer offerable here — disabled
   * since you picked it, or remembered from another scope — still has to be
   * visible and removable.
   */
  known: AgentDef[];
  /** Agent ids mid-turn here, so a chip can show it is thinking. */
  working: string[];
  /**
   * A chip drawn before the addressed ones and owned by the caller — the IDE's
   * *who an unaddressed message reaches*. It is not an addressee: it never
   * joins `addressed`, so it never reaches the wire as a mention and never
   * persists. The tray draws it and stays out of it.
   */
  lead?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const chips = addressChips(addressed.pubkeys, known);
  const byPubkey = useMemo(
    () => new Map(candidates.map((c) => [c.id, c])),
    [candidates],
  );

  if (candidates.length === 0 && chips.length === 0 && !lead) return null;

  return (
    <div className="mb-1 flex flex-wrap items-center gap-1">
      {lead}
      {chips.map(({ pubkey, agent }) => (
        <span key={pubkey} className="inline-flex items-center gap-1">
          <AgentChip
            candidate={
              byPubkey.get(pubkey) ??
              (agent
                ? {
                    id: pubkey,
                    name: agent.name,
                    kind: "agent" as const,
                    avatar: agent.pubkey,
                    photo: agent.photo,
                    // Offered nowhere any more, and still addressed: the chip
                    // is the only place that can say why nothing came back.
                    warning: warningFor(agent, null),
                  }
                : null)
            }
            name={t("studio-address-tray-not-in-conversation", { key: pubkey.slice(0, 8) })}
            onRemove={() => addressed.toggle(pubkey)}
          />
          {agent && working.includes(agent.id) && (
            <WorkingDot title={t("studio-address-tray-writing", { agent: agent.name })} />
          )}
        </span>
      ))}

      {candidates.length > 0 && (
        <Popover
          open={open}
          onOpenChange={setOpen}
          label={t("studio-address-tray-address-agent")}
          className="w-80"
          side="top"
          trigger={
            // Dashed because it is an "add here" door; its hover stays in the neutral ink.
            <span className="anim inline-flex h-6 items-center gap-1 rounded-full border border-dashed border-border px-2 text-2xs text-text-dim hover:border-text-dim/60 hover:bg-surface-2 hover:text-text">
              <ICON.add size={11} aria-hidden />
              {chips.length ? t("studio-address-tray-agent") : t("studio-address-tray-address-agent-2")}
            </span>
          }
        >
          <AgentPicker
            candidates={candidates}
            value={addressed.pubkeys}
            onChange={addressed.set}
            autoFocus
            chips={false}
            label={t("studio-address-tray-address-agent")}
            placeholder={t("studio-address-tray-search-name-role-tag")}
            listClassName="max-h-64"
            emptyTitle={t("studio-address-tray-no-agent-address-here")}
            emptyHint={t("studio-address-tray-send-anyway-unaddressed-message-reaches-default")}
            note={t("studio-address-tray-addressed-agents-see-every-message-send")}
          />
        </Popover>
      )}

      {chips.length > 0 && (
        <Tooltip label={t("studio-address-tray-everyone-here-mentioned-every-message-send")}>
          <span className="ml-1 cursor-default text-2xs text-text-dim">
            {chips.length === 1 && chips[0]!.agent
              ? t("studio-address-tray-will-see", { agent: chips[0]!.agent.name })
              : t("studio-address-tray-they-will-see")}
          </span>
        </Tooltip>
      )}
    </div>
  );
}
