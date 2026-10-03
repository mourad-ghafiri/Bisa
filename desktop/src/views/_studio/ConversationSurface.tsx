/**
 * The one conversation surface (13 — Conversations): what every owner paints
 * from `useConversationSurface` — the goal page's Conversation tab, the
 * Workflow Designer's Agent pane, the drawers beside a note or a drawing
 * (`ConversationDrawer`), the IDE's Agent pane and its Agent mode. One bar —
 * the owner's glyph, the thing's title (the picked conversation's while a
 * thread shows) and the **Conversations** door with its count — over one of
 * the views the model names (`surfaceView`): the list, filling the surface;
 * the picked thread, whole; the owner's own surface while nothing is picked,
 * where it has one; a quiet line while a record is read; the error, with a
 * retry; or the empty state with its one button, ***New conversation*** —
 * the same words as the list's foot, one click and no name.
 *
 * What a host brings is its own: its glyph and subject, what else its bar
 * says (the IDE's turn chip and frame), a whole header in place of the bar
 * (Agent mode's), how its thread is drawn (the IDE's `Conversation` with its
 * chips, mode and ledger; `ConversationThread` otherwise), and the surface
 * that stands while nothing is picked (the goal's thread). What no host may
 * change is the act.
 */

import type { ReactNode } from "react";
import type { ConversationView } from "../../types";
import { ErrorNote, Tooltip } from "../../ui";
import { ConversationList, ConversationsDoor } from "./ConversationsBar";
import { ConversationThread } from "./ConversationThread";
import { NoConversation } from "./NoConversation";
import { ownerKey } from "./conversationSurfaceModel.mjs";
import type { ConversationSurfaceState } from "./useConversationSurface";

export function ConversationSurface({
  surface: c,
  icon,
  subject,
  barExtras,
  header,
  thread,
  fallback,
  hint,
}: {
  /** The owner's conversations, read once by the host. */
  surface: ConversationSurfaceState;
  /** The owner's glyph, at the head of the bar and on its row in the list. */
  icon?: ReactNode;
  /** The thing's title, shown while no thread is. */
  subject: string;
  /** What else the bar says, between the title and the door. */
  barExtras?: ReactNode;
  /** A whole header in place of the bar — Agent mode's `ConversationHeader`, which carries the door itself. */
  header?: ReactNode;
  /** How the picked thread is drawn; `ConversationThread` when absent. */
  thread?: (row: ConversationView) => ReactNode;
  /** The owner's own surface while nothing is picked — the goal's thread — and its row's words in the list. */
  fallback?: { node: ReactNode; label: string };
  /** What a conversation here is about, for the empty state. */
  hint?: string;
}) {
  const listId = `conversations-${ownerKey(c.source.owner)}`;
  const title = c.view === "thread" && c.title ? c.title : subject;
  const bar = header ?? (
    // One bar: the owner, which conversation (or the thing while none is picked), what else the host says, and the one door.
    <div className="flex h-row shrink-0 items-center gap-2 border-b border-border px-2 text-2xs">
      {icon && <span className="shrink-0 text-text-dim">{icon}</span>}
      <Tooltip label={title}>
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-text">{title}</span>
      </Tooltip>
      {barExtras}
      <ConversationsDoor {...c.door} controls={listId} />
    </div>
  );
  let body: ReactNode;
  switch (c.view) {
    case "list":
      body = (
        <ConversationList
          id={listId}
          rows={c.rows}
          selected={c.pickId}
          onSelect={(row) => c.pick(row.id)}
          onChanged={c.reload}
          onNew={c.start}
          empty={c.loading ? c.words.reading : c.words.empty}
          q={c.q}
          onQuery={c.setQ}
          archived={c.archived}
          onArchived={c.setArchived}
          home={fallback ? { label: fallback.label, icon, current: c.pickId === null, onPick: c.back } : null}
        />
      );
      break;
    case "thread":
      body = c.selected && (thread ? thread(c.selected) : (
        <ConversationThread
          row={c.selected}
          onChanged={c.reload}
          onGone={() => {
            c.back();
            c.reload();
          }}
        />
      ));
      break;
    case "fallback":
      body = fallback?.node ?? null;
      break;
    case "reading":
      body = <p className="px-2 py-1 text-2xs text-text-dim">{c.words.reading}</p>;
      break;
    case "error":
      body = c.error !== null && <ErrorNote error={c.error} retry={c.reload} />;
      break;
    case "empty":
      body = <NoConversation title={c.words.empty} hint={hint} onNew={c.start} />;
      break;
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col" data-conversation-surface>
      {bar}
      <div className="min-h-0 flex-1">{body}</div>
    </div>
  );
}
