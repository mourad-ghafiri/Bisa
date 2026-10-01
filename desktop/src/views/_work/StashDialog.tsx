/**
 * *Stash changes…* (ide/04 §Stash), the Stashes view's dialog: a message,
 * whether untracked files ride along, whether the index stays — for the
 * whole tree, or for *Only these files*, the tracked changes ticked from the
 * checkout's own listing. The verb is the consented `POST …/git/stash`; the
 * tree is captured first, and the answer carries the entry. The words are
 * `stashModel.mjs`'s, and so is the one reason the button is off.
 */

import { useEffect, useState } from "react";
import type { GitStashPush, GitStatusInfo } from "../../types";
import { Button, Checkbox, Dialog, Field, ICON, TextInput, cn } from "../../ui";
import { ReasonLine } from "./ReasonLine";
import { canStash, stashPushCopy } from "./stashModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function StashDialog({
  open,
  files,
  status,
  onClose,
  submit,
}: {
  open: boolean;
  /** The paths a stash may be scoped to (`stashablePaths`); null while they load. */
  files: readonly string[] | null;
  status: GitStatusInfo;
  onClose: () => void;
  /** Run the push (the store's operation); answers whether an entry was made. */
  submit: (body: GitStashPush) => Promise<boolean>;
}) {
  const [message, setMessage] = useState("");
  const [untracked, setUntracked] = useState(false);
  const [keepIndex, setKeepIndex] = useState(false);
  const [scoped, setScoped] = useState(false);
  const [selected, setSelected] = useState<readonly string[]>([]);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setMessage("");
      setUntracked(false);
      setKeepIndex(false);
      setScoped(false);
      setSelected([]);
    }
  }, [open]);
  const paths = scoped ? selected : [];
  const copy = stashPushCopy(paths);
  // A path-scoped stash is judged by git on those paths; the whole-tree one
  // is judged here first, so the button says why before the click. A scope
  // with nothing ticked is not a stash of nothing — it waits for a tick.
  const can = scoped ? (selected.length > 0 ? { ok: true, reason: null } : { ok: false, reason: t("work-stash-dialog-tick-least-one-file-stash-whole") }) : canStash(status, untracked);

  const toggle = (path: string) => setSelected((s) => (s.includes(path) ? s.filter((p) => p !== path) : [...s, path]));

  const run = async () => {
    if (!can.ok || busy) return;
    setBusy(true);
    const made = await submit({
      message: message.trim() || null,
      include_untracked: !scoped && untracked,
      keep_index: keepIndex,
      paths: paths.length > 0 ? [...paths] : null,
    });
    setBusy(false);
    if (made) onClose();
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={copy.title}
      description={copy.description}
      footer={
        <>
          <Button size="sm" variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button size="sm" variant="primary" disabled={!can.ok || busy} onClick={() => void run()}>
            {busy ? t("work-stash-dialog-stashing") : copy.confirm}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          void run();
        }}
      >
        <Field label={t("work-merge-dialog-message")} hint={copy.messageHint}>
          <TextInput value={message} placeholder={t("work-stash-dialog-half-change")} autoFocus onChange={(e) => setMessage(e.target.value)} />
        </Field>
        {!scoped && <Checkbox label={copy.untracked.label} hint={copy.untracked.hint} checked={untracked} onChange={setUntracked} />}
        <Checkbox label={copy.keepIndex.label} hint={copy.keepIndex.hint} checked={keepIndex} onChange={setKeepIndex} />
        {/* The path-scoped push: the tracked changes, each a tick. */}
        <div className="flex flex-col gap-1">
          <Checkbox
            label={t("work-stash-dialog-only-these-files")}
            hint={t("work-stash-dialog-stash-ticked-paths-alone-rest-tree")}
            checked={scoped}
            disabled={files !== null && files.length === 0}
            onChange={setScoped}
          />
          {scoped && (
            <ul className={cn("max-h-40 overflow-y-auto rounded-control border border-border bg-surface-2/60 p-1", files === null && "opacity-60")} aria-label={t("work-stash-dialog-files-stash")}>
              {files === null && <li className="px-2 py-1 text-2xs text-text-dim">{t("work-stash-dialog-reading-changes")}</li>}
              {files?.map((path) => (
                <li key={path}>
                  <label className="flex cursor-pointer items-center gap-2 rounded-control px-2 py-1 font-mono text-2xs hover:bg-surface-2">
                    <input type="checkbox" checked={selected.includes(path)} onChange={() => toggle(path)} />
                    <ICON.file size={11} aria-hidden className="shrink-0 text-text-dim" />
                    <span className="min-w-0 truncate" title={path}>
                      {path}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
        </div>
        {!can.ok && can.reason && <ReasonLine tone="warn">{can.reason}</ReasonLine>}
      </form>
    </Dialog>
  );
}
