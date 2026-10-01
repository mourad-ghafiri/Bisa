/**
 * A turn's changes, drawn under its reply (ide/09 §Modes and the change
 * ledger, ide/20): the files it touched, each a `ChangedFileRow` with the
 * verbs its state offers, and the turn's own bulk verbs when more than one
 * file is still pending. Settling — the busy word, the *also edited by
 * someone else* confirm, *Undo with a note* — is `useFileSettle`'s, shared
 * with the bar above the composer.
 */

import type { TurnCard } from "./turnChangesModel.mjs";
import { turnWords } from "./turnChangesModel.mjs";
import { Button, ICON } from "../../ui";
import { ChangedFileRow } from "./ChangedFileRow";
import { useFileSettle } from "./useFileSettle";
import { t } from "../../i18n/l10n.mjs";

export function TurnChangesCard({
  conversationId,
  card,
  onOpenFile,
  onPostNote,
}: {
  conversationId: string;
  card: TurnCard;
  /** Open a file in the centre with the review lens on (ide/03). */
  onOpenFile: (path: string) => void;
  /** Undo with a note: the note posts as a plain message, after the undo lands. */
  onPostNote: (text: string) => Promise<void>;
}) {
  const settle = useFileSettle(conversationId, onPostNote);
  const turnBusy = settle.busy?.kind === "turn";
  return (
    <div className="mt-1.5 flex flex-col gap-1.5 rounded-card border border-border bg-surface-2/60 p-2 text-2xs">
      <div className="flex items-center gap-2 text-text-dim">
        <ICON.file size={12} aria-hidden className="shrink-0" />
        <span className="min-w-0 flex-1 truncate font-medium text-text">{turnWords(card.files)}</span>
        {card.verbs.length > 0 && (
          <div className="flex shrink-0 items-center gap-1">
            <Button size="sm" variant="ghost" disabled={turnBusy} onClick={() => settle.keepTurn(card.turn)} className="text-2xs">
              <ICON.check size={12} aria-hidden />{t("studio-turn-changes-card-keep-all")}</Button>
            <Button size="sm" variant="ghost" disabled={turnBusy} onClick={() => settle.undoTurn(card.turn)} className="text-2xs text-danger hover:text-danger">
              <ICON.undo size={12} aria-hidden />{t("studio-turn-changes-card-undo-all")}</Button>
            <Button size="sm" variant="ghost" disabled={turnBusy} onClick={() => settle.askNote({ turn: card.turn })} className="text-2xs">{t("studio-turn-changes-card-undo-all-note")}</Button>
          </div>
        )}
      </div>
      <div className="flex flex-col gap-1">
        {card.files.map((file) => (
          <ChangedFileRow
            key={file.path}
            file={file}
            busy={settle.busy?.kind === "file" && settle.busy.path === file.path}
            onOpen={() => onOpenFile(file.path)}
            onKeep={() => settle.keepFile(file.path)}
            onUndo={() => settle.undoFile(file.path, file.overlapped)}
            onUndoWithNote={() => settle.askNote({ file: file.path })}
          />
        ))}
      </div>
      {settle.dialogs}
    </div>
  );
}
