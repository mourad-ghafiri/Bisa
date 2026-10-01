/**
 * One file an agent changed, as the bar above the composer and the turn card
 * under a reply both draw it (ide/20): the kind's glyph, the path with its
 * folders dimmed, the lines added and removed, the state's chip, and the
 * verbs the state offers as real buttons — *Keep* and *Undo* (the bar), plus
 * *Undo with a note* where the caller hands one (the card). The path opens
 * the file in the centre with the review lens on.
 */

import type { FileChangeView } from "../../types";
import { Button, Chip, ICON, Tooltip, cn } from "../../ui";
import { changeKindMark, changeKindTone, fileChipTone, fileChipWords, fileVerbs, splitPathForRow } from "./changedFilesModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function ChangedFileRow({
  file,
  busy = false,
  onOpen,
  onKeep,
  onUndo,
  onUndoWithNote,
}: {
  file: FileChangeView;
  busy?: boolean;
  onOpen: () => void;
  onKeep: () => void;
  onUndo: () => void;
  /** Offered only where a note has somewhere to go — the turn card. */
  onUndoWithNote?: () => void;
}) {
  const verbs = fileVerbs(file);
  const { dir, base } = splitPathForRow(file.path);
  const tone = changeKindTone(file.kind);
  return (
    <div className="flex items-center gap-2">
      <span className={cn("w-3 shrink-0 text-center font-mono text-2xs", tone === "ok" ? "text-ok" : "text-warn")} aria-hidden>
        {changeKindMark(file.kind)}
      </span>
      <button type="button" onClick={onOpen} className="anim min-w-0 flex-1 truncate rounded-control px-1 py-0.5 text-left font-mono text-2xs text-text hover:bg-surface" title={t("studio-changed-file-row-open-review", { file: file.path })}>
        {dir && <span className="text-text-dim">{dir}</span>}
        {base}
      </button>
      {!file.opaque && (
        <span className="tnum shrink-0 text-2xs">
          <span className="text-ok">+{file.added}</span> <span className="text-danger">−{file.removed}</span>
        </span>
      )}
      <Chip tone={fileChipTone(file)}>{fileChipWords(file)}</Chip>
      {verbs.includes("keep") && (
        <Button size="sm" variant="ghost" disabled={busy} onClick={onKeep} className="text-2xs">
          <ICON.check size={12} aria-hidden />{t("studio-changed-file-row-keep")}</Button>
      )}
      {verbs.includes("undo") && (
        <Button size="sm" variant="ghost" disabled={busy} onClick={onUndo} className="text-2xs text-danger hover:text-danger">
          <ICON.undo size={12} aria-hidden />{t("studio-changed-file-row-undo")}</Button>
      )}
      {onUndoWithNote && verbs.includes("undo_with_note") && (
        <Tooltip label={t("studio-changed-file-row-undo-then-post-note-message")}>
          <Button size="sm" variant="ghost" disabled={busy} onClick={onUndoWithNote} className="text-2xs">{t("studio-changed-file-row-undo-note")}</Button>
        </Tooltip>
      )}
    </div>
  );
}
