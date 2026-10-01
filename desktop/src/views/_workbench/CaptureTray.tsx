/**
 * The tray under a device's mirror (ide/19): every capture as a numbered
 * line — where on the screen, the change wanted, × to drop it — over the
 * doors every tray of chips has (`ContextTray`: Send to an agent, Attach to
 * the Agent panel, Clear), and the note box a drawn rectangle waits in. The
 * facts — the chips, the words, the target — are `captureModel.mjs`'s.
 */

import { useState } from "react";
import type { KeyboardEvent } from "react";
import { Button, ICON, TextInput, Tooltip, cn } from "../../ui";
import { captureChips, captureLabel, capturesContent, capturesTarget, markWords, removeCapture, setCaptureMessage } from "./captureModel.mjs";
import type { CaptureDraft, Mark } from "./captureModel.mjs";
import { ContextTray } from "./ContextTray";
import { t } from "../../i18n/l10n.mjs";

export function CaptureTray({
  wid,
  pid,
  device,
  label,
  draft,
  onDraft,
  onDone,
}: {
  wid: string;
  pid: string | null;
  /** The device's id, as the node lists it. */
  device: string;
  /** The device's name — what the chips and the words call it. */
  label: string;
  draft: CaptureDraft;
  onDraft: (next: CaptureDraft) => void;
  onDone: () => void;
}) {
  const count = draft.captures.length;
  const chips = captureChips(device, label, draft.captures);
  return (
    <ContextTray
      wid={wid}
      pid={pid}
      chips={chips}
      count={count}
      message={draft.message}
      onMessage={(m) => onDraft(setCaptureMessage(draft, m))}
      target={capturesTarget(count, label)}
      content={capturesContent(draft.message, count)}
      about={t("workbench-capture-tray-app", { label })}
      noun={t("workbench-capture-tray-captures")}
      rows={<CaptureRows draft={draft} onDraft={onDraft} />}
      onDone={onDone}
    />
  );
}

/** The lines of a capture tray, and the one sentence a tray with nothing in it says. */
export function CaptureRows({ draft, onDraft }: { draft: CaptureDraft; onDraft: (next: CaptureDraft) => void }) {
  if (draft.captures.length === 0) {
    return <p className="text-text-dim">{t("workbench-capture-tray-drag-rectangle-over-screen-click-whole")}</p>;
  }
  return (
    <ul className="flex flex-col gap-1" aria-label={t("workbench-capture-tray-captured-spots")}>
      {draft.captures.map((c, i) => (
        <li key={c.id} className="group flex items-start gap-2">
          <span className="mt-px inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full bg-accent px-1 font-semibold text-accent-contrast">{i + 1}</span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-text" title={captureLabel(i + 1, c)}>
              <span className="font-mono text-text-dim">{markWords(c.mark)}</span> {c.note}
            </span>
          </span>
          <Tooltip label={t("workbench-capture-tray-drop-capture")}>
            <button type="button" aria-label={t("workbench-capture-tray-drop-capture-2", { i: i + 1 })} className="anim rounded text-text-dim hover:text-danger" onClick={() => onDraft(removeCapture(draft, c.id))}>
              <ICON.close size={11} aria-hidden />
            </button>
          </Tooltip>
        </li>
      ))}
    </ul>
  );
}

/** The note a drawn rectangle waits for: Enter adds, Escape lets it go. */
export function CaptureNoteBox({ mark, label, onAdd, onClose }: { mark: Mark | null; label: string; onAdd: (note: string) => void; onClose: () => void }) {
  const [note, setNote] = useState("");
  const words = note.trim();
  const key = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && words) {
      e.preventDefault();
      onAdd(words);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };
  return (
    <div className={cn("flex shrink-0 flex-wrap items-center gap-2 border-t border-border px-3 py-2 text-2xs")} role="group" aria-label={t("workbench-capture-tray-what-should-change-here")}>
      <span className="text-text-dim">
        {label} · <span className="font-mono">{markWords(mark)}</span>
      </span>
      <TextInput autoFocus value={note} placeholder={t("workbench-capture-tray-what-should-change-here-2")} aria-label={t("workbench-capture-tray-what-should-change-here")} className="min-w-0 flex-1 text-2xs" onChange={(e) => setNote(e.target.value)} onKeyDown={key} />
      <Button size="sm" variant="primary" disabled={!words} onClick={() => onAdd(words)}>{t("workbench-capture-tray-add")}</Button>
      <Button size="sm" variant="ghost" onClick={onClose}>{t("workbench-capture-tray-cancel")}</Button>
    </div>
  );
}
