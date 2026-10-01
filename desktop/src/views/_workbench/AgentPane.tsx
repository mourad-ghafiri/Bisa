/**
 * The agent pane (ide/09, 13 — Conversations): the conversation this
 * checkout is on, or the list of the checkout's and its project's
 * conversations to open one or start another — the right panel's Agent
 * occupant, painted as every owner's surface is (`ConversationSurface`).
 *
 * It is not a second chat — it is `Conversation` on a conversation's own
 * scope — with what a channel does not have:
 *
 * - **one bar**, the surface's: the root's glyph, the conversation's name
 *   (or the root's while the list or the empty state shows), what its turns
 *   add up to, the frame the agent is told (the project and the goals it is
 *   attached to) in a tooltip rather than in chips that look removable and
 *   are not, and the one door — **Conversations**, to the list and back;
 * - **one view at a time** (`surfaceView`): the list — a search the node
 *   answers, the *Archived* switch, one line per conversation with the verbs
 *   a record takes on hover (name, archive, delete), *New conversation* — or
 *   the conversation, whole, or the empty state whose one button says the
 *   same words. A row opens its conversation and the composer continues it;
 *   the fold is remembered per root (`agentPaneViewStore.ts`);
 * - **context chips**: what the person attached to the next message, kept per
 *   checkout (`agentPaneStore.ts`). Nothing is injected that is not a chip;
 *   a chip can be removed before sending;
 * - **an addressee**: who an unaddressed message reaches (`agents.default`),
 *   as the chip at the head of the address tray — nowhere else;
 * - **a status line** under the timeline, from the roster alone — who is
 *   working and on what, with *Stop* — and, once agents have written to disk,
 *   what changed and the way to Git › Changes;
 * - **the reply as it is written**: the timeline streams the agent's words
 *   and its thinking (`Chat`'s live turn), the thinking folded above the
 *   words and shown or hidden by the footer's toggle.
 *
 * Every one of those is wired in `useConversationPane`; this file paints
 * the compact form. The centre in Agent Mode (`AgentModeCenter`) paints
 * the full one from the same hook. The rail's rows are the checkout's work
 * sessions and terminals; a conversation's turn is never one of them — the
 * bar's chip, the composer's *Stop* and the pet say what the turns do.
 */

import { Conversation } from "../_studio/Conversation";
import { ConversationSurface } from "../_studio/ConversationSurface";
import { Chip, DropZone, ErrorNote, ICON, LinkRoots, Tooltip } from "../../ui";
import { useConversationPane } from "./useConversationPane";
import { t } from "../../i18n/l10n.mjs";

export function AgentPane({
  wid,
  pid,
  title,
  activeFile,
}: {
  /** The workstream whose conversations these are. */
  wid: string;
  /** Its project — settings, goals and the frame come from here. */
  pid: string;
  /** The root's label — the workstream's name, or the project's for the primary. */
  title: string;
  activeFile: string | null;
}) {
  const c = useConversationPane(wid, pid, activeFile);

  if (c.project.error && !c.project.data) return <ErrorNote error={c.project.error} retry={c.project.reload} />;

  return (
    <DropZone className="flex min-h-0 flex-1 flex-col" label={t("workbench-agent-mode-center-attach-next-message")} accepts={c.accepts} onDrop={c.onDrop}>
      <LinkRoots roots={c.linkRoots}>
        <ConversationSurface
          surface={c.surface}
          icon={wid === pid ? <ICON.project size={14} aria-hidden /> : <ICON.workstream size={14} aria-hidden />}
          subject={title}
          barExtras={
            <>
              {c.summary.words && (
                <Chip tone={c.summary.waiting > 0 ? "accent" : c.summary.working > 0 ? "ok" : "quiet"} title={t("workbench-agent-pane-what-conversation-s-turns-add-up")}>
                  {c.summary.words}
                </Chip>
              )}
              {c.frameWords && (
                <Tooltip label={t("workbench-agent-pane-every-agent-here-told-placement-not", { frameWords: c.frameWords })}>
                  <span className="anim shrink-0 cursor-default truncate text-text-dim" style={{ maxWidth: "10rem" }}>
                    {c.frameWords}
                  </span>
                </Tooltip>
              )}
            </>
          }
          thread={() =>
            c.conversationProps && (
              <Conversation
                {...c.conversationProps}
                placeholder={t("workbench-agent-pane-ask-hand-over-adds-file")}
                emptyTitle={t("workbench-agent-pane-nobody-has-spoken-here-yet")}
                emptyHint={t("workbench-agent-pane-agents-answer-conversation-working-checkout-what")}
              />
            )
          }
          hint={t("workbench-agent-pane-conversation-agents-about-checkout-saved-listed")}
        />
      </LinkRoots>
    </DropZone>
  );
}
