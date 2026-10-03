/**
 * What the agents changed in this conversation, above the composer (ide/20
 * §The desktop surfaces) — the shape Cursor's and Copilot's chats give the
 * same fact: one line, *3 files changed by Reviewer · +40 −12*, that opens
 * to a row per file with *Keep* and *Undo*, and a footer of real buttons —
 * **Undo all** (asked first, naming the count: it takes work back),
 * **Review** (the first pending file in the centre, lens on), *Attach* (the
 * changes as chips on the next message), and **Keep all** trailing as the
 * bar's one primary. While a bulk word is out both bulk verbs are held, and
 * a toast says how many files it reached. In `auto` the footer says the
 * changes are kept when the next message is sent. Nothing of git's working
 * tree is drawn here: that is the Git panel's, and keeping it out is what
 * tells the two apart.
 */

import { useState } from "react";
import type { ChangesView, ConversationMode } from "../../types";
import { Button, ConfirmDialog, ICON, Tooltip, cn } from "../../ui";
import { ChangedFileRow } from "./ChangedFileRow";
import { barWords, changedFileRows, changedFilesHeaderWords, undoAllConfirmWords } from "./changedFilesModel.mjs";
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
  onAttach: () => void;
}

export function ChangedFilesBar({ view, mode, conversationId, agentName, onOpenFile, onAttach }: ChangedFilesBarProps) {
  const rows = changedFileRows(view);
  const [open, setOpen] = useState(false);
  const [confirmUndo, setConfirmUndo] = useState(false);
  /** The bulk word in flight, if one is: both bulk verbs and every row wait for it. */
  const [settling, setSettling] = useState<"keep" | "undo" | null>(null);
  const settle = useFileSettle(conversationId);
  const words = barWords();
  if (rows.length === 0) return null;
  const settleAll = async (verdict: "keep" | "undo") => {
    setSettling(verdict);
    try {
      await settle.settleAll(verdict);
    } finally {
      setSettling(null);
    }
  };
  const confirm = undoAllConfirmWords(view);
  const glyph = (verdict: "keep" | "undo") => {
    if (settling === verdict) return <ICON.working size={12} aria-hidden className="motion-safe:animate-spin" />;
    return verdict === "keep" ? <ICON.check size={12} aria-hidden /> : <ICON.undo size={12} aria-hidden />;
  };
  return (
    <div className="mb-1 rounded-card border border-border bg-surface-2/60 text-2xs" data-changed-files-bar>
      <button type="button" aria-expanded={open} onClick={() => setOpen((o) => !o)} className="anim flex w-full items-center gap-1.5 rounded-t-card px-2 py-1 text-left hover:bg-surface-2">
        <ICON.collapsed size={11} aria-hidden className={cn("anim shrink-0 text-text-dim", open && "rotate-90")} />
        <ICON.agent size={11} aria-hidden className="shrink-0 text-text-dim" />
        <span className="min-w-0 flex-1 truncate font-medium text-text">{changedFilesHeaderWords(view, agentName)}</span>
      </button>
      {open && (
        <div className="flex flex-col gap-1 border-t border-hairline px-2 py-1.5">
          {rows.map((f) => (
            <ChangedFileRow key={f.path} file={f} busy={settling !== null || (settle.busy?.kind === "file" && settle.busy.path === f.path)} onOpen={() => onOpenFile(f.path)} onKeep={() => settle.keepFile(f.path)} onUndo={() => settle.undoFile(f.path, f.overlapped)} />
          ))}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-1.5 border-t border-hairline px-2 py-1" aria-busy={settling !== null}>
        <Button size="sm" variant="danger" disabled={settling !== null} onClick={() => setConfirmUndo(true)} className="text-2xs">
          {glyph("undo")}
          {words.undoAll}
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
        {/* Keep all trails the row: the bar's one primary, where the eye ends. */}
        <div className="ml-auto flex items-center gap-1.5">
          {mode === "auto" && <span className="text-2xs text-text-dim">{autoKeptHint()}</span>}
          <Button size="sm" variant="primary" disabled={settling !== null} onClick={() => void settleAll("keep")} className="text-2xs">
            {glyph("keep")}
            {words.keepAll}
          </Button>
        </div>
      </div>
      <ConfirmDialog
        open={confirmUndo}
        onClose={() => setConfirmUndo(false)}
        onConfirm={() => {
          setConfirmUndo(false);
          void settleAll("undo");
        }}
        title={confirm.title}
        body={confirm.body}
        confirmLabel={confirm.confirm}
        danger
      />
      {settle.dialogs}
    </div>
  );
}
