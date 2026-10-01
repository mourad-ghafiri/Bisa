/**
 * The door to an owner's conversations and the list it opens (13 —
 * Conversations): what every surface's bar wears — the goal's tab, the
 * designer's Agent pane, the drawers beside a note or a drawing, the IDE's
 * Agent pane and Agent mode — so the five read alike. The list fills the
 * surface: a search over the words said in them (the node's), an *Archived*
 * switch, the owner's own surface as a first row where it has one (the goal's
 * thread), the rows, and *New conversation* at the foot — the same words as
 * the empty state's one button.
 */

import type { ReactNode } from "react";
import type { ConversationView } from "../../types";
import { Button, ICON, Switch, TextInput, Tooltip, cn } from "../../ui";
import { ConversationRows } from "./ConversationRows";
import { t } from "../../i18n/l10n.mjs";

/** The bar's door to the conversations list — and back: the word, the count, pressed while the list shows; held when the list is the only view. */
export function ConversationsDoor({
  label,
  count,
  hint,
  open,
  disabled = false,
  controls,
  onToggle,
}: {
  label: string;
  count: number;
  hint: string;
  open: boolean;
  disabled?: boolean;
  /** The list's element id, for `aria-controls`. */
  controls?: string;
  onToggle: () => void;
}) {
  return (
    <Tooltip label={hint}>
      <span className="inline-flex">
        <Button size="sm" variant={open ? "default" : "ghost"} aria-expanded={open} aria-controls={controls} disabled={disabled} onClick={onToggle}>
          <ICON.dm size={12} aria-hidden />
          {label}
          {count > 0 && <span className="tnum text-text-dim">{count}</span>}
        </Button>
      </span>
    </Tooltip>
  );
}

/** The owner's own surface as a row of the list: the goal's thread, current while nothing is picked. */
export interface ListHome {
  label: string;
  icon?: ReactNode;
  current: boolean;
  onPick: () => void;
}

/** The list the door opens: the search and the switch, the owner's own surface as a row where it has one, the rows, and *New conversation*. */
export function ConversationList({
  id,
  rows,
  selected,
  onSelect,
  onChanged,
  onNew,
  empty,
  q,
  onQuery,
  archived,
  onArchived,
  home = null,
}: {
  /** The element id the door's `aria-controls` names. */
  id?: string;
  rows: readonly ConversationView[];
  selected: string | null;
  onSelect: (row: { id: string }) => void;
  onChanged: () => void;
  onNew: () => void;
  /** The owner's sentence when the rows are none — *No conversation about this note yet.* — or that they are being read. */
  empty: ReactNode;
  /** The words searched for — the node's `?q=`, what was said included. */
  q: string;
  onQuery: (q: string) => void;
  /** Whether the archived ones are listed instead of the live. */
  archived: boolean;
  onArchived: (archived: boolean) => void;
  home?: ListHome | null;
}) {
  return (
    <div id={id} className="h-full min-h-0 overflow-y-auto px-1 py-1">
      <div className="mb-1 flex items-center gap-2 px-1">
        <TextInput value={q} placeholder={t("studio-conversations-bar-search-messages-names")} aria-label={t("studio-conversations-bar-search-conversations")} className="h-6 min-w-0 flex-1 text-2xs" onChange={(e) => onQuery(e.currentTarget.value)} />
        <Switch checked={archived} onChange={onArchived} label={t("studio-conversations-bar-archived")} />
      </div>
      {home && (
        <button
          type="button"
          aria-current={home.current ? "true" : undefined}
          onClick={home.onPick}
          className={cn("anim mb-1 flex w-full min-w-0 items-center gap-1.5 rounded-control px-2 py-1 text-left text-xs hover:bg-surface-2", home.current ? "bg-surface-2 font-semibold text-text" : "text-text")}
        >
          {home.icon && <span className="shrink-0 text-text-dim">{home.icon}</span>}
          <span className="min-w-0 truncate">{home.label}</span>
        </button>
      )}
      <ConversationRows rows={rows} selected={selected} onSelect={onSelect} onChanged={onChanged} dense empty={<p className="px-2 py-1 text-2xs text-text-dim">{empty}</p>} />
      <Button size="sm" variant="ghost" className="mt-1 w-full justify-start" onClick={onNew}>
        <ICON.add size={12} aria-hidden />{t("studio-conversations-bar-new-conversation")}</Button>
    </div>
  );
}
