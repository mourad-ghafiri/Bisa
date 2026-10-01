/**
 * The frame every conversation wears: a header, the timeline, the composer.
 *
 * Channels, DMs, goals and workstreams differ in what their header says and
 * what sits in the right-hand pane — not in how talking works. Keeping the
 * frame in one place is what stops them drifting into four dialects again.
 *
 * The header is a {@link PageHeader} at its default `h2`. The shell's chrome
 * already emits the page's `h1` ("Channels", "Messages"), and this names the
 * *thing* being read — `#design`, a goal, the agent you are talking to —
 * which is a heading below the screen's, not a second copy of it. The `icon`
 * slot stays a `ReactNode` because a DM's leading glyph is a principal's
 * avatar rather than a symbol, and an avatar is the one identity mark that
 * cannot come from `ui/icons`.
 */

import type { ContextRef } from "../../types";
import type { FrameChip } from "../_workbench/contextChips.mjs";
import { useEffect } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode } from "react";
import { PageHeader } from "../../ui";
import type { ComposerFiles, ComposerStop, MenuItem } from "../../ui";
import { Chat } from "./Chat";
import type { ChangedFilesBarProps } from "./ChangedFilesBar";
import type { ScopeKind } from "./scope";
import { clearContext, removeContext, useAgentPane } from "../_workbench/agentPaneStore";
import { chatKey } from "./chatScopeModel.mjs";
import { publishChatTarget } from "./chatTargetStore";
import type { ChannelDef, Directory } from "../../types";

export function ConversationHeader({
  icon,
  title,
  subtitle,
  chips,
  actions,
}: {
  /** A domain glyph from `ui/icons`, or an `Avatar` when the subject is a principal. */
  icon?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  /** Chips read rather than clicked — tags, state, a working pulse. */
  chips?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="flex shrink-0 items-start gap-1 border-b border-border">
      {icon && <div className="mt-3.5 shrink-0 pl-4">{icon}</div>}
      <PageHeader
        title={title}
        subtitle={subtitle}
        meta={chips}
        actions={actions}
        className="min-w-0 flex-1"
      />
    </div>
  );
}

export function Conversation({
  scope,
  kind,
  channel,
  header,
  pinned,
  placeholder,
  emptyTitle,
  emptyHint,
  emptyAction,
  context,
  onRemoveContext,
  onClearContext,
  frame,
  files,
  attachItems,
  stop,
  activity,
  changedFiles,
  addressee,
  hosted,
  afterMessage,
  restore,
  composerLeading,
  onComposerKeyDownCapture,
}: {
  scope: string;
  kind: ScopeKind;
  channel?: ChannelDef | null;
  /** The surface's header; absent when the page above the tabs already owns one. */
  header?: ReactNode;
  pinned?: ReactNode;
  placeholder?: string;
  emptyTitle?: string;
  emptyHint?: string;
  emptyAction?: ReactNode;
  /** Context chips for the next message and the placement frame (ide/09). */
  context?: readonly ContextRef[];
  onRemoveContext?: (index: number) => void;
  onClearContext?: () => void;
  frame?: FrameChip[];
  /** The root's files under `@`, ways to attach context, and a session to stop (ide/09). */
  files?: ComposerFiles;
  attachItems?: MenuItem[];
  stop?: ComposerStop | null;
  /** What is happening here — see `Chat`; the one Stop is `stop`, the composer's slot while a turn works. */
  activity?: { words: string | null } | null;
  /** What the agents changed in this conversation — the ledger's bar above the composer (ide/20). */
  changedFiles?: ChangedFilesBarProps | null;
  /** Who an unaddressed message reaches. */
  addressee?: ReactNode;
  /** A scope on a workspace this node is a guest of: the host and its directory (14-collaboration). */
  hosted?: { host: string; directory: Directory[] } | null;
  /** A turn's changes card, drawn under the message that anchors it (ide/09) — see `Chat`. */
  afterMessage?: (id: string) => ReactNode;
  /** *Restore to before this message* (ide/09) — see `Chat`. */
  restore?: { isPrompt: (id: string) => boolean; onRestore: (id: string) => void };
  /** A conversation's mode picker, in the composer's foot (ide/09). */
  composerLeading?: ReactNode;
  /** The mode picker's `Shift+Tab` (ide/09) — see `Chat`. */
  onComposerKeyDownCapture?: (e: ReactKeyboardEvent) => void;
}) {
  // The conversation on screen, for a tray beside it — the Browser pane's
  // annotations — to attach to (ide/18); and its own chip tray, kept under
  // its kind and id, when the caller brings none (the IDE's Agent pane does).
  useEffect(() => publishChatTarget({ kind, id: scope }), [kind, scope]);
  const own = useAgentPane(chatKey(kind, scope));
  const chips = context ?? own.context;
  const removeChip = onRemoveContext ?? ((index: number) => removeContext(chatKey(kind, scope), index));
  const clearChips = onClearContext ?? (() => clearContext(chatKey(kind, scope)));
  return (
    <div className="flex h-full min-h-0 flex-col">
      {header}
      <div className="min-h-0 flex-1">
        {/* Keyed by scope: a switch of channel or of conversation starts the thread afresh — where it was being read, else at its bottom, with its own entry marker — as the Inbox's does. */}
        <Chat
          key={`${kind}:${scope}`}
          scope={scope}
          kind={kind}
          channel={channel}
          pinned={pinned}
          placeholder={placeholder}
          emptyTitle={emptyTitle}
          emptyHint={emptyHint}
          emptyAction={emptyAction}
          context={chips}
          onRemoveContext={removeChip}
          onClearContext={clearChips}
          frame={frame}
          files={files}
          attachItems={attachItems}
          stop={stop}
          activity={activity}
          changedFiles={changedFiles}
          addressee={addressee}
          hosted={hosted}
          afterMessage={afterMessage}
          restore={restore}
          composerLeading={composerLeading}
          onComposerKeyDownCapture={onComposerKeyDownCapture}
        />
      </div>
    </div>
  );
}
