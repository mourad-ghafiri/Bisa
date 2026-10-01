/**
 * What follows a merged pull request: one dialog, the steps the
 * policies chose already checked — pull the default branch, delete this
 * workstream's checkout and branch, return to the primary — each a checkbox,
 * run in that order (the pull happens on the primary *before* anything
 * leaves), each reporting what it did. A harness running on the primary keeps
 * the pull and the return off, with the reason. Under the automatic policies
 * the same list runs without asking and this dialog only shows progress —
 * and the toast says what it did.
 *
 * Deleting the checkout goes through the one close door
 * (`closeWorkstream.ts`): what still stands in it — harnesses, shells, agent
 * sessions — is terminated with it, said on the step before the person
 * confirms and in the summary after.
 *
 * Deciding lives in `afterMergeModel.mjs`; this runs the plan over the
 * existing consented routes, so every step is a person's action with its own
 * recovery ref, and a pull that stops on conflicts lands on the primary's
 * Changes view with the Resolve card and the conflict document.
 */
import { useEffect, useState } from "react";
import { ApiError, api } from "../../api";
import { navigate } from "../../router";
import type { PullMode } from "../../types";
import { Button, Checkbox, Dialog, ICON, useToast } from "../../ui";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { openPanelView } from "../_workbench/rightPanelStore";
import { afterMergePlan, stepsToRun, summaryOf, toggleStep } from "./afterMergeModel.mjs";
import type { AfterMergeStep, StepId, StepOutcome } from "./afterMergeModel.mjs";
import { closeWorkstream } from "./closeWorkstream";
import type { TerminationCounts } from "./closeWorkstreamModel.mjs";
import { afterPull } from "./syncModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function AfterMergeDialog({
  open,
  onClose,
  pid,
  wid,
  branch,
  defaultBranch,
  dirty,
  cleanup,
  afterMerge,
  pullMode,
  primaryBusy,
  terminated,
  onDone,
}: {
  open: boolean;
  onClose: () => void;
  /** The project — its primary's id is the project's. */
  pid: string;
  /** The merged workstream. */
  wid: string;
  branch: string | null;
  defaultBranch: string | null;
  dirty: boolean;
  cleanup: string;
  afterMerge: string;
  pullMode: PullMode;
  /** Sessions running on the primary right now (server count + local harness shells). */
  primaryBusy: number | null;
  /** What still stands in the merged checkout — said on the delete step. */
  terminated: TerminationCounts;
  /** The flow finished or was dismissed; the caller reloads what it shows. */
  onDone: (outcomes: Partial<Record<StepId, StepOutcome>>) => void;
}) {
  const toast = useToast();
  const plan = afterMergePlan({ cleanup, afterMerge, pullMode, primaryBusy, defaultBranch, branch, dirty, terminated });
  const [steps, setSteps] = useState<AfterMergeStep[]>(plan.steps);
  const [running, setRunning] = useState<StepId | null>(null);
  const [outcomes, setOutcomes] = useState<Partial<Record<StepId, StepOutcome>>>({});
  /** What the delete step actually ended — counted at the close, so the summary reports it, not the promise. */
  const [ended, setEnded] = useState<TerminationCounts>(terminated);
  const [conflictPaths, setConflictPaths] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [started, setStarted] = useState(false);
  const [finished, setFinished] = useState(false);
  const main = defaultBranch ?? t("work-after-merge-dialog-default-branch");

  useEffect(() => {
    if (!open) return;
    setSteps(plan.steps);
    setRunning(null);
    setOutcomes({});
    setEnded(terminated);
    setConflictPaths(null);
    setError(null);
    setStarted(false);
    setFinished(false);
    // Reset on an opening or a changed input: `plan` and `terminated` are
    // objects remade by every render of the caller.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, cleanup, afterMerge, pullMode, primaryBusy, defaultBranch, branch]);

  const goToPrimaryChanges = () => {
    openPanelView("git", "changes", rootKey("workstream", pid));
    navigate({ name: "workbench", scope: "workstream", id: pid });
  };

  const run = async () => {
    if (started) return;
    setStarted(true);
    const order = stepsToRun(steps);
    const got: Partial<Record<StepId, StepOutcome>> = {};
    let stop = false;
    for (const id of order) {
      if (stop) {
        got[id] = "skipped";
        continue;
      }
      setRunning(id);
      try {
        if (id === "pull") {
          const r = await api.gitPull(pid, pullMode);
          got.pull = "done";
          void r;
        } else if (id === "delete") {
          const closed = await closeWorkstream(wid, { tree: true });
          setEnded(closed.terminated);
          if (branch) {
            try {
              await api.gitBranchDelete(pid, branch);
            } catch (e) {
              // The checkout is gone and the branch is on the code host; a local
              // branch that would not go is said, not fatal.
              setError(t("work-after-merge-dialog-local-branch-stayed", { branch, error: e instanceof Error ? e.message : String(e) }));
            }
          }
          got.delete = "done";
        } else if (id === "return") {
          got.return = "done";
        }
      } catch (e) {
        if (id === "pull") {
          const message = e instanceof Error ? e.message : String(e);
          const banner = afterPull({ err: e instanceof ApiError ? { status: e.status, code: e.code, message, detail: e.detail } : { status: 0, message } });
          if (banner.kind === "conflict") {
            got.pull = "conflict";
            setConflictPaths(banner.paths);
          } else {
            got.pull = "failed";
            setError(banner.kind === "not_fast_forward" ? t("work-after-merge-dialog-cannot-fast-forward-has-local-commit", { main, ahead: banner.ahead }) : message);
          }
          // Nothing else runs on a primary that is mid-merge or refused.
          stop = true;
        } else {
          got[id] = "failed";
          setError(e instanceof Error ? e.message : String(e));
          stop = true;
        }
      }
      setOutcomes({ ...got });
    }
    setRunning(null);
    setFinished(true);
    onDone(got);
    // The automatic policies ran unattended: the toast is where what happened is said.
    if (plan.mode === "silent") toast.ok(summaryOf(got, defaultBranch, endedNow(got)));
    if (got.return === "done" && got.pull !== "conflict") {
      navigate({ name: "workbench", scope: "workstream", id: pid });
    }
  };

  /** The counts the summary reports: what the close ended when it ran, nothing when it did not. */
  const endedNow = (got: Partial<Record<StepId, StepOutcome>>) => (got.delete === "done" ? ended : { harnesses: 0, shells: 0, agents: 0 });

  // The automatic policies do not ask: run as soon as the dialog opens.
  useEffect(() => {
    if (open && plan.mode === "silent" && !started) void run();
    // `run` is a plain function remade every render; the silent start follows
    // the opening, the mode and whether it began.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, plan.mode, started]);

  const mark = (id: StepId) => {
    const o = outcomes[id];
    if (running === id) return <ICON.refresh size={12} aria-hidden className="animate-spin text-text-dim" />;
    if (o === "done") return <ICON.check size={12} aria-hidden className="text-ok" />;
    if (o === "conflict" || o === "failed") return <ICON.warn size={12} aria-hidden className="text-warn" />;
    return null;
  };

  return (
    <Dialog
      open={open}
      onClose={() => {
        if (running) return;
        onClose();
      }}
      title={finished ? t("work-after-merge-dialog-merged") : t("work-after-merge-dialog-merged-clean-up")}
      description={finished ? summaryOf(outcomes, defaultBranch, endedNow(outcomes)) : t("work-after-merge-dialog-pull-request-merged-code-host-clean", { main })}
      width="max-w-md"
      footer={
        finished ? (
          <>
            {outcomes.pull === "conflict" && (
              <Button variant="primary" onClick={goToPrimaryChanges}>{t("work-after-merge-dialog-resolve-conflicts", { main })}</Button>
            )}
            <Button variant={outcomes.pull === "conflict" ? "ghost" : "primary"} onClick={onClose}>{t("work-after-merge-dialog-close")}</Button>
          </>
        ) : (
          <>
            <Button variant="ghost" disabled={running !== null} onClick={onClose}>{t("work-after-merge-dialog-stay-here")}</Button>
            <Button variant="primary" disabled={running !== null || stepsToRun(steps).length === 0} onClick={() => void run()}>
              {running ? t("work-after-merge-dialog-working") : plan.mode === "silent" ? t("work-after-merge-dialog-running") : t("work-after-merge-dialog-clean-up")}
            </Button>
          </>
        )
      }
    >
      <ul className="flex flex-col gap-2 text-xs">
        {[...steps]
          .sort((a, b) => ["pull", "delete", "return"].indexOf(a.id) - ["pull", "delete", "return"].indexOf(b.id))
          .map((s) => (
            <li key={s.id} className="flex items-start gap-2">
              <span className="mt-0.5 w-3 shrink-0">{mark(s.id)}</span>
              <div className="min-w-0 flex-1">
                <Checkbox
                  label={s.label}
                  checked={s.checked && s.available}
                  disabled={!s.available || started}
                  onChange={() => setSteps((cur) => toggleStep(cur, s.id))}
                  hint={s.reason ?? s.detail}
                />
              </div>
            </li>
          ))}
      </ul>
      {conflictPaths && (
        <div className="mt-3 rounded-control border border-warn/40 bg-warn-soft px-3 py-2 text-2xs text-text">
          <p className="font-semibold">
            {t("work-after-merge-dialog-pull-stopped-on-files", { n: conflictPaths.length, main })}
          </p>
          <p className="mt-0.5 text-text-dim">
            {t("work-after-merge-dialog-nothing-else-ran-open-git-tab", { main })}
          </p>
          {conflictPaths.length > 0 && <p className="mt-1 font-mono">{conflictPaths.join("  ")}</p>}
        </div>
      )}
      {error && <p className="mt-3 text-2xs text-danger">{error}</p>}
    </Dialog>
  );
}
