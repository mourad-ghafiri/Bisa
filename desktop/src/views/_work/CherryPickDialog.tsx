/**
 * The one dialog a cherry-pick from a branch is asked for in (ide/04 §The
 * Branches view): the commits the branch has and the current one lacks,
 * newest first with a tick each, *Record where each came from* (`-x`) and
 * *Stop before committing*, and the consenting **Cherry-pick** naming the
 * count. The commits are sent oldest first, as git applies them.
 */
import { useEffect, useState } from "react";
import { api } from "../../api";
import { Button, Checkbox, Dialog, EmptyState, ErrorNote, RelativeTime, SkeletonRows } from "../../ui";
import { pickConsent, pickOrder } from "./mergeModel.mjs";
import { SafetyNote } from "./SafetyNote";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

export function CherryPickDialog({
  open,
  onClose,
  wid,
  source,
  current,
  busy = false,
  onConfirm,
}: {
  open: boolean;
  onClose: () => void;
  wid: string;
  /** The branch picked from — local, or `origin/x`. */
  source: string | null;
  current: string;
  busy?: boolean;
  onConfirm: (commits: string[], opts: { recordOrigin: boolean; noCommit: boolean }) => void;
}) {
  const commits = useAsync((s) => (open && source ? api.gitBranchCommits(wid, source, current, s) : Promise.resolve(null)), [wid, source, current, open]);
  const [picked, setPicked] = useState<Set<string>>(() => new Set());
  const [recordOrigin, setRecordOrigin] = useState(true);
  const [noCommit, setNoCommit] = useState(false);
  useEffect(() => {
    if (open) {
      setPicked(new Set());
      setRecordOrigin(true);
      setNoCommit(false);
    }
  }, [open, source]);
  const listed = commits.data?.commits ?? [];
  const ordered = pickOrder([...picked], listed);
  const copy = pickConsent(Math.max(ordered.length, 1), { recordOrigin, noCommit });
  const toggle = (id: string) =>
    setPicked((p) => {
      const next = new Set(p);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("work-cherry-pick-dialog-cherry-pick-from-onto", { source: source ?? "", current })}
      description={ordered.length > 0 ? copy.body : t("work-cherry-pick-dialog-commits-has-lacks-tick-ones-pick", { source: source ?? t("work-cherry-pick-dialog-branch"), current })}
      width="max-w-lg"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={busy || ordered.length === 0} onClick={() => onConfirm(ordered, { recordOrigin, noCommit })}>
            {busy ? t("work-cherry-pick-dialog-picking") : ordered.length > 1 ? t("work-cherry-pick-dialog-cherry-pick-commits", { ordered: ordered.length }) : copy.confirm}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        {commits.loading && !commits.data ? (
          <SkeletonRows rows={3} />
        ) : commits.error ? (
          <ErrorNote error={commits.error} retry={commits.reload} />
        ) : listed.length === 0 ? (
          <EmptyState title={t("work-cherry-pick-dialog-nothing-pick")} hint={t("work-cherry-pick-dialog-already-has-every-commit-has", { current, source: source ?? t("work-cherry-pick-dialog-branch") })} className="py-3" action={null} />
        ) : (
          <ul className="flex max-h-72 flex-col overflow-y-auto rounded-control border border-border" role="listbox" aria-label={t("work-cherry-pick-dialog-commits-pick")} aria-multiselectable>
            {listed.map((c) => (
              <li key={c.id} role="option" aria-selected={picked.has(c.id)}>
                <label className={`flex cursor-pointer items-center gap-2 px-2 py-1 text-2xs hover:bg-surface-2 ${picked.has(c.id) ? "bg-selected" : ""}`}>
                  <input type="checkbox" checked={picked.has(c.id)} onChange={() => toggle(c.id)} />
                  <span className="font-mono text-text-dim">{c.short}</span>
                  <span className="min-w-0 flex-1 truncate text-text">{c.subject}</span>
                  <span className="shrink-0 text-text-dim">{c.author}</span>
                  <RelativeTime at={c.timestamp} className="shrink-0 text-text-dim" />
                </label>
              </li>
            ))}
          </ul>
        )}
        <div className="flex flex-wrap gap-4">
          <Checkbox label={t("work-cherry-pick-dialog-record-where-each-came-from")} hint={t("work-cherry-pick-dialog-cherry-picked-from-line-message")} checked={recordOrigin} onChange={setRecordOrigin} />
          <Checkbox label={t("work-cherry-pick-dialog-stop-before-committing")} hint={t("work-cherry-pick-dialog-change-staged-one-commit")} checked={noCommit} onChange={setNoCommit} />
        </div>
        <SafetyNote kind="tree" />
      </div>
    </Dialog>
  );
}
