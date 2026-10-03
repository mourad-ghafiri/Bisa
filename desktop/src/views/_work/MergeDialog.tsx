/**
 * The one dialog a merge is asked for in (ide/04 §The Branches view): the
 * source into the current branch, the mode as four choices with a sentence
 * each — the project's `git.merge_strategy` picks the first — the message a
 * merge commit carries, and the consenting **Merge** with the `SafetyNote`.
 * The words are `mergeModel.mjs`'s.
 */
import { useEffect, useState } from "react";
import { api } from "../../api";
import type { GitMergeMode } from "../../types";
import { Button, Dialog, Field, TextInput } from "../../ui";
import { MERGE_MODES, MERGE_MODE_WORDS, mergeConsent, mergeMessagePlaceholder, mergeTakesMessage, previewWords } from "./mergeModel.mjs";
import { ReasonLine } from "./ReasonLine";
import { SafetyNote } from "./SafetyNote";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

/** What the node foresees for merging `source` (or rebasing onto it), before anything moves. */
export function LookAhead({ wid, source, open, rebase = false }: { wid: string; source: string; open: boolean; rebase?: boolean }) {
  const preview = useAsync((s) => (open && source ? api.gitMergePreview(wid, source, s) : Promise.resolve(null)), [wid, source, open]);
  if (!open || !source) return null;
  const words = previewWords(preview.error ? { supported: false, clean: false, paths: [] } : preview.data, { rebase });
  return <ReasonLine tone={words.tone === "warn" ? "warn" : "dim"}>{words.text}</ReasonLine>;
}

export function MergeDialog({
  open,
  onClose,
  wid,
  source,
  target,
  defaultMode,
  busy = false,
  onConfirm,
}: {
  open: boolean;
  onClose: () => void;
  wid: string;
  /** The branch merged in — local, or `origin/x`. */
  source: string;
  /** The current branch. */
  target: string;
  defaultMode: GitMergeMode;
  busy?: boolean;
  onConfirm: (mode: GitMergeMode, message: string | null) => void;
}) {
  const [mode, setMode] = useState<GitMergeMode>(defaultMode);
  const [message, setMessage] = useState("");
  useEffect(() => {
    if (open) {
      setMode(defaultMode);
      setMessage("");
    }
  }, [open, defaultMode, source]);
  const copy = mergeConsent(source, target, mode);
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={copy.title}
      description={copy.body}
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={busy} onClick={() => onConfirm(mode, mergeTakesMessage(mode) && message.trim() ? message.trim() : null)}>
            {busy ? t("work-merge-control-merging") : copy.confirm}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <fieldset className="flex flex-col gap-1.5" aria-label={t("work-merge-dialog-how-merge-lands")}>
          {MERGE_MODES.map((m) => (
            <label key={m} className={`flex cursor-pointer items-start gap-2 rounded-control border px-2 py-1.5 ${mode === m ? "border-text/35 bg-selected text-text" : "border-border hover:bg-surface-2"}`}>
              <input type="radio" name="merge-mode" value={m} checked={mode === m} onChange={() => setMode(m)} className="mt-0.5" />
              <span className="flex flex-col">
                <span className="text-xs text-text">{MERGE_MODE_WORDS[m].label}</span>
                <span className="text-2xs text-text-dim">{MERGE_MODE_WORDS[m].meaning}</span>
              </span>
            </label>
          ))}
        </fieldset>
        <LookAhead wid={wid} source={source} open={open} />
        {mergeTakesMessage(mode) && (
          <Field label={t("work-merge-dialog-message")} hint={t("work-merge-dialog-merge-commit-when-one-made-git")}>
            <TextInput value={message} placeholder={mergeMessagePlaceholder(source, target)} onChange={(e) => setMessage(e.target.value)} />
          </Field>
        )}
        <SafetyNote kind="tree" />
      </div>
    </Dialog>
  );
}
