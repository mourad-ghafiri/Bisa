/**
 * One conversation read whole, under a header that names it, says what it
 * is about with a door to that thing, and the verbs its record takes (13 —
 * Conversations). What an owner draws under its list once a conversation is
 * picked, and what the door route draws for a conversation nothing owns —
 * the node's, the workspace's. Its memory is its messages: the harness
 * keeps its own context, so nothing here compacts.
 *
 * The composer has what the IDE's has: the **addressee** — who an
 * unaddressed message reaches — as the pill at the head of the address tray,
 * and **Stop** beside *Send* while a turn runs (the roster's rows for this
 * conversation, `conversationPaneModel.turnSummary`). The pill is read-only
 * here: an owned conversation's default agent is the workspace layer's, and a
 * thread is no place to rewrite a workspace setting — `@` names another
 * agent for one message.
 */

import { useMemo } from "react";
import { openDrawing } from "../../draw/drawStore";
import { openNote } from "../../notes/notesStore";
import { href } from "../../router";
import { stopSession, useSessions } from "../../shell/sessionsStore";
import { stopWords } from "../../shell/sessionRosterModel.mjs";
import { useWorkspace } from "../../shell/useWorkspaceData";
import type { ConversationOrigin, ConversationView } from "../../types";
import { Button, Chip, ICON, Menu, failureText, useToast } from "../../ui";
import { addressee } from "../_workbench/agentRailModel.mjs";
import { sessionsOf, turnSummary } from "../_workbench/conversationPaneModel.mjs";
import { GENERAL_AGENT } from "../_workbench/editorAgentModel.mjs";
import { Conversation, ConversationHeader } from "./Conversation";
import { useConversationVerbs, useOriginNames } from "./ConversationRows";
import { originIcon, originWords, routeOf, titleOf } from "./conversationsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The origin as a chip, a door to the thing when the thing has a screen —
 * or, for a drawing and a note, the overlay that shows it.
 */
function OriginDoor({ origin }: { origin: ConversationOrigin }) {
  const names = useOriginNames();
  const words = originWords(origin, names);
  const open = origin.kind === "drawing" ? () => openDrawing(origin.id) : origin.kind === "note" ? () => openNote(origin.id) : null;
  const route =
    origin.kind === "goal"
      ? href({ name: "goal", id: origin.id })
      : origin.kind === "workflow"
        ? href({ name: "workflow", id: origin.id })
        : origin.kind === "project"
          ? href({ name: "workbench", scope: "workstream", id: origin.id })
          : origin.kind === "workstream"
            ? href({ name: "workbench", scope: "workstream", id: origin.id })
            : null;
  const Icon = ICON[originIcon(origin.kind) as keyof typeof ICON] as typeof ICON.dm;
  const chip = (
    <Chip tone="quiet" icon={Icon}>
      {words}
    </Chip>
  );
  if (open) {
    return (
      <button type="button" onClick={open} className="anim inline-flex hover:opacity-80" title={t("studio-conversation-thread-open", { words })}>
        {chip}
      </button>
    );
  }
  return route ? (
    <a href={route} className="anim inline-flex hover:opacity-80" title={t("studio-conversation-thread-open", { words })}>
      {chip}
    </a>
  ) : (
    chip
  );
}

export function ConversationThread({
  row,
  onChanged,
  onGone,
}: {
  row: ConversationView;
  /** The record moved — renamed, archived; the owner re-reads its list. */
  onChanged: () => void;
  /** The record was deleted; the owner clears its selection. */
  onGone: () => void;
}) {
  const ws = useWorkspace();
  const toast = useToast();
  const names = useOriginNames();
  const verbs = useConversationVerbs((what) => {
    if (what === "deleted") onGone();
    else onChanged();
  });
  const working = ws.working[row.id] ?? [];
  const reaches = addressee(ws.agents, GENERAL_AGENT, GENERAL_AGENT, row.origin.kind);
  const home = routeOf(row);
  // The conversation's turns, from the roster: what they add up to, and the
  // one *Stop* names — none while every turn is idle.
  const sessionRows = useSessions();
  const summary = useMemo(() => turnSummary(sessionsOf(sessionRows, row.id)), [sessionRows, row.id]);
  const stoppable = summary.stoppable;
  const stop = stoppable
    ? {
        onStop: () =>
          void stopSession(stoppable.id).then(
            ({ how, ended }) => toast.ok(stopWords(how, t("studio-conversation-thread-turn-stopped"), ended)),
            (e: unknown) => toast.error(failureText("studio", "conversation-thread-failed", e)),
          ),
      }
    : null;
  const addresseeChip = reaches && (
    <span
      className="inline-flex h-6 shrink-0 items-center gap-1 rounded-full border border-border px-2 text-2xs text-text-dim"
      title={t("studio-conversation-thread-unaddressed-message-reaches", { reaches: reaches.name })}
    >
      <ICON.agent size={11} aria-hidden />
      <span className="max-w-32 truncate">{reaches.name}</span>
    </span>
  );
  return (
    <>
      <Conversation
        scope={row.id}
        kind="conversation"
        addressee={addresseeChip}
        stop={stop}
        activity={{ words: summary.activity }}
        placeholder={row.archived ? t("studio-conversation-thread-conversation-archived-take-back-out-continue") : reaches ? t("studio-conversation-thread-ask-another-agent", { reaches: reaches.name }) : t("studio-conversation-thread-write-message")}
        emptyTitle={t("studio-conversation-thread-nobody-has-spoken-here-yet")}
        emptyHint={t("studio-conversation-thread-conversation-about-agent-answers-here-remembers", { names: originWords(row.origin, names) })}
        header={
          <ConversationHeader
            icon={<ICON.dm size={18} aria-hidden className="text-text-dim" />}
            title={titleOf(row)}
            subtitle={[`${row.message_count} ${row.message_count === 1 ? "message" : "messages"}`, row.archived ? "archived" : null].filter(Boolean).join(" · ")}
            chips={
              <>
                <OriginDoor origin={row.origin} />
                {working.length > 0 && <Chip tone="ok">{t("studio-conversation-thread-agent-writing")}</Chip>}
                {row.agents.map((a) => (
                  <Chip key={a} tone="neutral" icon={ICON.agent}>
                    {ws.agents.find((x) => x.id === a)?.name ?? a}
                  </Chip>
                ))}
              </>
            }
            actions={
              <>
                {home.route.name === "workbench" && (
                  <Button asChild size="sm">
                    <a href={href(home.route as Parameters<typeof href>[0], home.search ?? undefined)}>
                      <ICON.open size={13} aria-hidden />{t("studio-conversation-thread-open-ide")}</a>
                  </Button>
                )}
                <Menu
                  label={t("studio-conversation-rows-conversation-actions")}
                  items={verbs.items(row)}
                  trigger={
                    <span className="anim inline-flex h-7 w-7 items-center justify-center rounded-control border border-border text-text-dim hover:bg-surface-2 hover:text-text">
                      <ICON.more size={14} aria-hidden />
                    </span>
                  }
                />
              </>
            }
          />
        }
      />
      {verbs.dialog}
    </>
  );
}
