/**
 * The dialogs behind a goal's three run verbs — one component the goal page
 * and the Goals list's card both mount, so a stop reads the same wherever it
 * is pressed.
 *
 * *Start* is `StartRunDialog` with the workflow's inputs (queued behind a
 * live run when one is; the adopt form when a proposal waits). A design
 * that begins on events starts by listening — *Start listening…* asks only
 * what its events do not supply — and a listening goal's run by hand is
 * *Run now…* at its start by hand. *Stop*
 * confirms with what it cancels and withdraws. *Restart* confirms only when
 * it stops something; on a finished goal it goes straight to the node.
 * `pending` names the verb in flight; the words are `runControl.mjs`'s.
 */
import { useEffect, useRef } from "react";
import { api } from "../../api";
import type { GoalView, Workflow } from "../../types";
import { ConfirmDialog, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { StartRunDialog } from "../_workflow/StartRunDialog";
import { startInputs } from "../_workflow/forms/startForm.mjs";
import { pageFacts } from "./goalPageModel.mjs";
import { queuedRuns, restartWords, runVerbs, stopWords } from "./runControl.mjs";
import { t } from "../../i18n/l10n.mjs";

export type RunVerb = "start" | "stop" | "restart";

export function RunVerbDialogs({
  view,
  pending,
  startable: design = null,
  onClose,
  onDone,
}: {
  view: GoalView;
  pending: RunVerb | null;
  /** The workflow a start would run, when the host has it: the goal's stored one before a run. Else the run's frozen copy. */
  startable?: Workflow | null;
  onClose: () => void;
  /** After the node answered: the caller refetches. */
  onDone: () => void;
}) {
  const toast = useToast();
  const { goal, run, runs, guidance, pending_gates, strip } = view;
  const startable = design ?? run?.workflow ?? null;
  // Whether a design waits to be adopted, whether it begins on events, and where a run by hand begins — the page's own reading (`goalPageModel`).
  const { adoptGate, proposed, listens, manualEntry } = pageFacts({ goal, run, pendingGates: pending_gates, startable });
  const verbs = runVerbs({ goal, run, runs, guidance, proposed, startable: !!startable || !!goal.workflow, listens, manualEntry });
  // Starting by listening asks only what its events do not supply.
  const listen = !!verbs.start?.listen || (proposed && listens);
  const asked = startInputs(startable, listen);
  const live = !!verbs.stop?.live;
  const queued = queuedRuns(runs).length;
  const restartConfirm = restartWords({ live, queued });

  const stop = () => {
    void attempt(() => api.stopGoal(goal.id, {}), toast.error, (s) => {
      const n = s.withdrawn.length;
      toast.ok(n > 0 ? t("goal-run-verb-dialogs-stopped-queued-run-runs-withdrawn", { n }) : s.stopped ? t("goal-run-verb-dialogs-stopped") : t("goal-run-verb-dialogs-nothing-running"));
      onDone();
    });
    onClose();
  };
  const restart = () => {
    void attempt(() => api.restartGoal(goal.id), toast.error, () => {
      toast.ok(t("goal-run-verb-dialogs-restarted"));
      onDone();
    });
    onClose();
  };

  // A restart that stops nothing owes no confirm: it goes as soon as it is
  // asked — once per ask, whatever re-renders in between.
  const fired = useRef<RunVerb | null>(null);
  useEffect(() => {
    if (pending === "restart" && restartConfirm === null && fired.current !== "restart") {
      fired.current = "restart";
      restart();
    }
    if (pending === null) fired.current = null;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fire on the ask, not on every render
  }, [pending, restartConfirm]);

  return (
    <>
      <StartRunDialog
        open={pending === "start"}
        goal={goal.id}
        name={startable?.name ?? strip.workflow_name ?? t("goal-run-verb-dialogs-workflow")}
        inputs={asked}
        adopt={proposed ? { gate: adoptGate?.id ?? null } : null}
        queues={!!verbs.start?.queues}
        listen={listen}
        at={verbs.start?.at ?? null}
        onClose={onClose}
        onDone={onDone}
      />
      <ConfirmDialog
        open={pending === "stop"}
        onClose={onClose}
        title={t("goal-run-verb-dialogs-stop-goal")}
        confirmLabel={t("goal-run-verb-dialogs-stop")}
        danger
        body={stopWords({ live, queued, liveSteps: live ? strip.current.length : 0 })}
        onConfirm={stop}
      />
      <ConfirmDialog
        open={pending === "restart" && restartConfirm !== null}
        onClose={onClose}
        title={t("goal-run-verb-dialogs-restart-goal")}
        confirmLabel={t("goal-progress-tab-restart")}
        body={restartConfirm ?? ""}
        onConfirm={restart}
      />
    </>
  );
}
