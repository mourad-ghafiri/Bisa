/**
 * The interactive rebase editor (ide/04 §The Branches view): the target at
 * the top, the current branch's commits since it as rows — oldest first,
 * the order git replays them — each with its action (pick · reword · squash
 * · fixup · drop), ↑/↓ to reorder, a message opening for a reword or a
 * squash, the summary line, and the consenting **Rebase**, off with the
 * first problem the plan has. The rules are `rebaseEditorModel.mjs`'s; the
 * plan runs with no terminal anywhere.
 */
import { useEffect, useState } from "react";
import { api } from "../../api";
import type { GitRebaseAction } from "../../types";
import { Button, Dialog, EmptyState, ErrorNote, Field, ICON, Select, SkeletonRows, TextArea, Tooltip } from "../../ui";
import { REBASE_ACTIONS, REBASE_ACTION_WORDS, editorRows, moveStep, planConsent, planIsIdentity, planOf, planProblem, planSummary, setAction, setMessage } from "./rebaseEditorModel.mjs";
import type { EditorRow, RebasePlanBody } from "./rebaseEditorModel.mjs";
import { SafetyNote } from "./SafetyNote";
import { useAsync } from "./useAsync";
import { t as tr } from "../../i18n/l10n.mjs";

export function RebaseEditorDialog({
  open,
  onClose,
  wid,
  current,
  targets,
  defaultTarget,
  busy = false,
  onConfirm,
}: {
  open: boolean;
  onClose: () => void;
  wid: string;
  current: string;
  /** Every branch the target may be — local and remote. */
  targets: string[];
  defaultTarget: string | null;
  busy?: boolean;
  onConfirm: (plan: RebasePlanBody, summary: string) => void;
}) {
  const [target, setTarget] = useState(defaultTarget ?? targets[0] ?? "");
  const commits = useAsync((s) => (open && target ? api.gitBranchCommits(wid, current, target, s) : Promise.resolve(null)), [wid, current, target, open]);
  const [rows, setRows] = useState<EditorRow[]>([]);
  useEffect(() => {
    if (open) setTarget(defaultTarget ?? targets[0] ?? "");
  }, [open, defaultTarget, targets]);
  const listed = commits.data?.commits ?? null;
  useEffect(() => {
    setRows(listed ? editorRows(listed) : []);
  }, [listed]);
  const problem = planProblem(rows);
  const summary = planSummary(rows);
  const unchanged = listed ? planIsIdentity(rows, listed) : true;
  const copy = planConsent(current, target, summary);
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={`${copy.title.replace("?", "")}`}
      description={tr("work-rebase-editor-dialog-each-commit-s-action-order-they")}
      width="max-w-2xl"
      footer={
        <>
          <span className="min-w-0 flex-1 truncate text-2xs text-text-dim">{problem ?? summary}</span>
          <Button variant="ghost" onClick={onClose}>{tr("work-agent-editor-cancel")}</Button>
          <Tooltip label={problem ?? (unchanged ? tr("work-rebase-editor-dialog-nothing-would-change") : copy.body)}>
            <span className="inline-flex">
              <Button variant="primary" disabled={busy || problem !== null || unchanged} onClick={() => onConfirm(planOf(rows, target), summary)}>
                {busy ? tr("work-rebase-dialog-rebasing") : copy.confirm}
              </Button>
            </span>
          </Tooltip>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={tr("work-rebase-editor-dialog-onto")} hint={tr("work-rebase-editor-dialog-commits-after-point-ones-edited")}>
          <Select value={target} onChange={(e) => setTarget(e.target.value)} className="font-mono">
            {targets.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </Select>
        </Field>
        {commits.loading && !commits.data ? (
          <SkeletonRows rows={3} />
        ) : commits.error ? (
          <ErrorNote error={commits.error} retry={commits.reload} />
        ) : rows.length === 0 ? (
          <EmptyState title={tr("work-rebase-editor-dialog-nothing-rebase")} hint={tr("work-rebase-editor-dialog-has-no-commits-since", { current, target: target || tr("work-rebase-editor-dialog-target") })} className="py-3" action={null} />
        ) : (
          // One list on hairlines, not a box per commit: the plan reads as one thing in order.
          <ol className="flex max-h-[50vh] flex-col divide-y divide-hairline overflow-y-auto rounded-control border border-border" aria-label={tr("work-rebase-editor-dialog-plan-oldest-first")}>
            {rows.map((r, i) => (
              <li key={r.id} className="flex flex-col gap-1 px-2 py-1.5 text-2xs">
                <div className="flex items-center gap-2">
                  <span className="tnum w-5 shrink-0 text-text-dim/70">{i + 1}</span>
                  <Select value={r.action} onChange={(e) => setRows(setAction(rows, i, e.target.value as GitRebaseAction))} className="h-6 w-24 text-2xs" aria-label={tr("work-rebase-editor-dialog-action", { short: r.short })}>
                    {REBASE_ACTIONS.map((a) => (
                      <option key={a} value={a} title={REBASE_ACTION_WORDS[a].meaning}>
                        {REBASE_ACTION_WORDS[a].label}
                      </option>
                    ))}
                  </Select>
                  <span className="font-mono text-text-dim">{r.short}</span>
                  <span className={`min-w-0 flex-1 truncate ${r.action === "drop" ? "text-text-dim line-through" : "text-text"}`} title={r.subject}>
                    {r.subject}
                  </span>
                  <span className="shrink-0 text-text-dim">{r.author}</span>
                  <span className="inline-flex shrink-0 items-center">
                    <button type="button" aria-label={tr("work-rebase-editor-dialog-move-up", { short: r.short })} disabled={i === 0} onClick={() => setRows(moveStep(rows, i, "up"))} className="anim rounded px-1 text-text-dim hover:text-text disabled:opacity-30">
                      <ICON.up size={12} aria-hidden />
                    </button>
                    <button type="button" aria-label={tr("work-rebase-editor-dialog-move-down", { short: r.short })} disabled={i === rows.length - 1} onClick={() => setRows(moveStep(rows, i, "down"))} className="anim rounded px-1 text-text-dim hover:text-text disabled:opacity-30">
                      <ICON.down size={12} aria-hidden />
                    </button>
                  </span>
                </div>
                {(r.action === "reword" || r.action === "squash") && (
                  <TextArea value={r.message} rows={2} aria-label={tr("work-rebase-editor-dialog-message", { short: r.short })} placeholder={r.action === "reword" ? tr("work-rebase-editor-dialog-new-message") : tr("work-rebase-editor-dialog-message-combined-commit")} className="text-2xs" onChange={(e) => setRows(setMessage(rows, i, e.target.value))} />
                )}
              </li>
            ))}
          </ol>
        )}
        <SafetyNote kind="tree" />
      </div>
    </Dialog>
  );
}
