/**
 * The centre in Agent Mode (ide/09 §Agent Mode): the conversation this
 * checkout is on, at reading width, where the documents and terminals stand
 * in Project Mode. The rail and the right panel are what they always are;
 * only the middle changed its purpose.
 *
 * Painted from `useConversationPane`, the same hook the right panel's
 * Agent occupant paints from — one conversation, one chips tray, one draft
 * — and as every owner's surface is (`ConversationSurface`), with the room
 * the centre gives: a real header in place of the bar (the conversation's
 * name, or **Conversations** while the list or the empty state shows; the
 * root, its branch and the count or the turns' summary; its placement as
 * chips; and the one door), and under it the list of the project's
 * conversations, filling the centre, the conversation with a composer that
 * says what `@` can reach here — an agent, a team, a file — or the empty
 * state with its one button. Who an unaddressed message reaches is the
 * address tray's chip, as in the panel — nothing else names an agent up here.
 *
 * While this is the centre the Agent occupant is not offered in the right
 * panel (`availableOccupants`), and every door that asks for it lands here
 * instead — the workbench answers by putting the caret in the composer of
 * the conversation this root is on (`focusComposer`).
 */

import { Conversation, ConversationHeader } from "../_studio/Conversation";
import { Chip, DropZone, ErrorNote, ICON, LinkRoots, Tooltip } from "../../ui";
import { ConversationsDoor } from "../_studio/ConversationsBar";
import { ConversationSurface } from "../_studio/ConversationSurface";
import { countWords } from "../_studio/conversationSurfaceModel.mjs";
import { useConversationPane } from "./useConversationPane";
import { t } from "../../i18n/l10n.mjs";

export function AgentModeCenter({
  wid,
  pid,
  title,
  branch,
  activeFile,
}: {
  wid: string;
  pid: string;
  /** The root's label — the workstream's name, or the project's for the primary. */
  title: string;
  /** The checkout's branch, when it has one. */
  branch: string | null;
  activeFile: string | null;
}) {
  const c = useConversationPane(wid, pid, activeFile);

  if (c.project.error && !c.project.data) return <ErrorNote error={c.project.error} retry={c.project.reload} />;

  const s = c.surface;
  const threadShows = s.view === "thread";
  const subtitle = [title, branch, threadShows ? c.summary.words || t("workbench-agent-mode-center-no-session-running-conversation") : countWords(s.rows.length)]
    .filter(Boolean)
    .join(" · ");

  const header = (
    <ConversationHeader
      icon={<ICON.dm size={16} aria-hidden className="text-text-dim" />}
      title={threadShows && s.title ? s.title : s.door.label}
      subtitle={subtitle}
      chips={
        <span className="flex flex-wrap items-center gap-1">
          {c.frame.map((f, i) => (
            <Tooltip key={`${f.kind}:${i}`} label={t("workbench-agent-mode-center-placement-every-agent-here-told-fact")}>
              <span>
                <Chip tone="neutral" icon={f.kind === "goal" ? ICON.goal : ICON.project}>
                  {f.label}
                </Chip>
              </span>
            </Tooltip>
          ))}
        </span>
      }
      // The door switches between the thread and the list; with neither a thread nor a row it would only repeat the title.
      actions={threadShows || s.rows.length > 0 ? <ConversationsDoor {...s.door} /> : undefined}
    />
  );

  return (
    <LinkRoots roots={c.linkRoots}>
      {/* `min-w-0`: the centre is a column of the IDE row and yields its width like Project Mode's; at its content's
          minimum it held the row eight pixels past the window whenever a menu or dialog opened over it. */}
      <DropZone className="flex h-full min-h-0 min-w-0 flex-1 flex-col" label={t("workbench-agent-mode-center-attach-next-message")} accepts={c.accepts} onDrop={c.onDrop}>
        <div className="mx-auto flex h-full w-full min-h-0 max-w-4xl flex-col">
          <ConversationSurface
            surface={s}
            subject={title}
            header={header}
            thread={() =>
              c.conversationProps && (
                <Conversation
                  {...c.conversationProps}
                  placeholder={t("workbench-agent-mode-center-ask-agents-mentions-agent-team-file")}
                  emptyTitle={t("workbench-agent-mode-center-nobody-has-spoken-conversation-yet")}
                  emptyHint={t("workbench-agent-mode-center-agents-teams-answer-here-working-checkout")}
                />
              )
            }
            hint={t("workbench-agent-pane-conversation-agents-about-checkout-saved-listed")}
          />
        </div>
      </DropZone>
    </LinkRoots>
  );
}
