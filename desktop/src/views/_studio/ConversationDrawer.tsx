/**
 * A conversation drawer beside a document (13 — Conversations): what the
 * Notes and Draw editors mount at their right edge — the conversations about
 * *this* note or drawing, the one surface every owner has
 * (`ConversationSurface`): the same bar, list, thread and one-click *New
 * conversation*, in a column the person **resizes** by its left edge and the
 * app remembers (`useStoredSize`, the Workbench right panel's recipe). Its
 * pick is **kept** per note or drawing (`conversationSelection` "kept"): a
 * drawer reopened lands on the conversation the person left there, and
 * nothing is written into the address of the screen underneath, which owns
 * `?conversation=`.
 *
 * The handle is the drawer's flex sibling, as `ResizeHandle` is built to be
 * used; the parent row is `flex min-h-0`.
 */

import type { ReactNode } from "react";
import type { ConversationOrigin } from "../../types";
import { ResizeHandle, useStoredSize } from "../../ui";
import { ConversationSurface } from "./ConversationSurface";
import { useConversationSurface } from "./useConversationSurface";

const DRAWER_DEFAULT_WIDTH = 352;
const DRAWER_MIN_WIDTH = 280;
const DRAWER_MAX_WIDTH = 640;

export function ConversationDrawer({
  origin,
  icon,
  subject,
  widthKey,
  label,
  hint,
}: {
  /** What the conversations are about — a note, a drawing. */
  origin: ConversationOrigin;
  /** The owner's glyph, at the head of the drawer's bar. */
  icon: ReactNode;
  /** The document's title, shown while no conversation is picked. */
  subject: string;
  /** The `localStorage` key the width is remembered under — one per owner. */
  widthKey: string;
  /** The handle's accessible name. */
  label: string;
  /** What a conversation here is about, for the empty state. */
  hint?: string;
}) {
  const c = useConversationSurface({ owner: origin }, "kept");
  const [width, setWidth] = useStoredSize(widthKey, DRAWER_DEFAULT_WIDTH, { min: DRAWER_MIN_WIDTH, max: DRAWER_MAX_WIDTH });
  return (
    <>
      <ResizeHandle side="left" size={width} min={DRAWER_MIN_WIDTH} max={DRAWER_MAX_WIDTH} defaultSize={DRAWER_DEFAULT_WIDTH} onSize={setWidth} label={label} />
      <aside data-conversation-drawer aria-label={label} style={{ width }} className="flex min-h-0 shrink-0 flex-col border-l border-border bg-surface">
        <ConversationSurface surface={c} icon={icon} subject={subject} hint={hint} />
      </aside>
    </>
  );
}
