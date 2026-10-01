/**
 * What the agents changed in this conversation, above the composer (ide/20
 * §The desktop surfaces) — the shape Cursor's and Copilot's chats give the
 * same fact: one line, *3 files changed by Reviewer · +40 −12*, that opens
 * to a row per file with *Keep* and *Undo*, and a footer of real buttons —
 * **Undo all**, **Keep all**, **Review** (the first pending file in the
 * centre, lens on), *Attach* (the changes as chips on the next message). In
 * `auto` the footer says the changes are kept when the next message is
 * sent. Nothing of git's working tree is drawn here: that is the Git
 * panel's, and keeping it out is what tells the two apart.
 */

import { useState } from "react";
import type { ChangesView, ConversationMode } from "../../types";
import { Button, ICON, Tooltip, cn } from "../../ui";
import { ChangedFileRow } from "./ChangedFileRow";
import { barWords, changedFileRows, changedFilesHeaderWords } from "./changedFilesModel.mjs";
import { autoKeptHint } from "./turnChangesModel.mjs";
import { useFileSettle } from "./useFileSettle";
import { t } from "../../i18n/l10n.mjs";

export interface ChangedFilesBarProps {
  view: ChangesView;
  mode: ConversationMode;
  conversationId: string;
  /** An agent's id as the person knows it. */
  agentName: (id: string) => string;
  /** Open a file in the centre with the review lens on. */
  onOpenFile: (path: string) => void;
  onKeepAll: () => void;
  onUndoAll: () => void;
  onAttach: () => void;
}

export function ChangedFilesBar({ view, mode, conversationId, agentName, onOpenFile, onKeepAll, onUndoAll, onAttach }: ChangedFilesBarProps) {
  const rows = changedFileRows(view);
  const [open, setOpen] = useState(false);
  const settle = useFileSettle(conversationId);
  const words = barWords();
  if (rows.length === 0) return null;
  return (
    <div className="mb-1 rounded-card border border-border bg-surface-2/60 text-2xs" data-changed-files-bar>
      <button type="button" aria-expanded={open} onClick={() => setOpen((o) => !o)} className="anim flex w-full items-center gap-1.5 rounded-t-card px-2 py-1 text-left hover:bg-surface-2">
        <ICON.collapsed size={11} aria-hidden className={cn("anim shrink-0 text-text-dim", open && "rotate-90")} />
        <ICON.agent size={11} aria-hidden className="shrink-0 text-text-dim" />
        <span className="min-w-0 flex-1 truncate font-medium text-text">{changedFilesHeaderWords(view, agentName)}</span>
      </button>
      {open && (
        <div className="flex flex-col gap-1 border-t border-border px-2 py-1.5">
          {rows.map((f) => (
            <ChangedFileRow key={f.path} file={f} busy={settle.busy?.kind === "file" && settle.busy.path === f.path} onOpen={() => onOpenFile(f.path)} onKeep={() => settle.keepFile(f.path)} onUndo={() => settle.undoFile(f.path, f.overlapped)} />
          ))}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-1.5 border-t border-border px-2 py-1">
        <Button size="sm" variant="danger" onClick={onUndoAll} className="text-2xs">
          <ICON.undo size={12} aria-hidden />
          {words.undoAll}
        </Button>
        <Button size="sm" variant="primary" onClick={onKeepAll} className="text-2xs">
          <ICON.check size={12} aria-hidden />
          {words.keepAll}
        </Button>
        <Button size="sm" onClick={() => onOpenFile(rows[0].path)} className="text-2xs">
          <ICON.inspect size={12} aria-hidden />
          {words.review}
        </Button>
        <Tooltip label={t("studio-changed-files-bar-attach-changes-chips-next-message-hunks")}>
          <Button size="sm" variant="ghost" onClick={onAttach} className="text-2xs">
            {words.attach}
          </Button>
        </Tooltip>
        {mode === "auto" && <span className="ml-auto text-3xs text-text-dim/80">{autoKeptHint()}</span>}
      </div>
      {settle.dialogs}
    </div>
  );
}
