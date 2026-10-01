/**
 * The run read downward: every step of the frozen workflow as a
 * row, the live ones open, with what each produced and the verbs it admits.
 * With no workflow yet, a guided goal shows where the Workflow Agent stands
 * (`DesigningCard`) and a manual one the two ways out.
 */
import { placeOf } from "../../shell/viewMemoryStore";
import type { GoalView, NeedsAction } from "../../types";
import { Button, ICON } from "../../ui";
import { ProposalCard } from "../_studio/ProposalCard";
import { RunHistory } from "../_workflow/RunHistory";
import { DesigningCard } from "./DesigningCard";
import { runStanding } from "./goalPageModel.mjs";
import { finishedWords, progressRows } from "./progressModel.mjs";
import { cancelWords, runIsQueued } from "./runControl.mjs";
import { RunSteps } from "./RunSteps";
import { t } from "../../i18n/l10n.mjs";

export function ProgressTab({
  view,
  proposal,
  onEditPlan,
  onChanged,
  onOpenItem,
  onDecide,
  onRestart,
  onNewRun,
  newRunLabel = t("goal-progress-tab-new-run"),
}: {
  view: GoalView;
  /** The Workflow Agent's proposal, when one is waiting — the goal's landing. */
  proposal: NeedsAction | null;
  /** Open the canvas to edit the proposed steps before adopting. */
  onEditPlan: () => void;
  onChanged: () => void;
  onOpenItem: (item: string) => void;
  onDecide: () => void;
  /** The finished banner's *Restart*, when the goal offers one. */
  onRestart?: () => void;
  /** The finished banner's *New run…*, when the goal offers one. */
  onNewRun?: () => void;
  /** Its words, when the goal's start is another verb: *Run now…*, *Start listening…*. */
  newRunLabel?: string;
}) {
  const { goal, run, runs, strip } = view;
  const rows = progressRows(run, strip);

  // The proposed plan is the goal's landing: before any run, the
  // agent's design is the first thing a person sees — every step, and the
  // doors to adopt it, ask for changes, or edit it — not a pointer elsewhere.
  if (proposal?.proposal) {
    return (
      <div className="mx-auto w-full max-w-2xl p-6">
        <div className="rounded-card border border-warn/40 bg-warn-soft/40 p-3">
          <ProposalCard
            goal={goal.id}
            action={proposal}
            proposal={proposal.proposal}
            hero
            onEdit={onEditPlan}
            onResolved={onChanged}
          />
        </div>
      </div>
    );
  }

  // No run yet: the designing card, whatever the mode — for a manual goal it
  // says the design is the person's and opens the Workflow tab.
  if (rows.length === 0) return <DesigningCard view={view} onChanged={onChanged} />;

  const { finished } = runStanding(run);
  const queued = runIsQueued(run);
  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-2 px-4 py-3">
      {queued && (
        <p className="flex items-center gap-2 px-3 py-1 text-2xs text-text-dim">
          <ICON.queued size={12} aria-hidden />{t("goal-progress-tab-queued-starts-when-live-run-finishes")}</p>
      )}
      {/* Keyed by the run: another run's rows are its own, opened as they were left on it. */}
      <RunSteps key={run?.id ?? "plan"} run={run} strip={strip} place={run ? placeOf({ name: "run", id: run.id }) : placeOf({ name: "goal", id: goal.id })} onChanged={onChanged} onOpenItem={onOpenItem} onDecide={onDecide} />
      {finished && run && (
        <p className="flex items-center gap-2 px-3 py-2 text-2xs text-text-dim">
          {run.cancelled ? t("goal-progress-tab-run", { cancelled: cancelWords(run.cancelled) }) : finishedWords(run)}
          {onRestart && !goal.closed && (
            <Button size="sm" variant="ghost" onClick={onRestart}>
              <ICON.restart size={12} aria-hidden />{t("goal-progress-tab-restart")}</Button>
          )}
          {onNewRun && !goal.closed && (
            <Button size="sm" variant="ghost" onClick={onNewRun}>
              <ICON.run size={12} aria-hidden />{newRunLabel}</Button>
          )}
        </p>
      )}
      {/* The runs, under the run: history and the queue in one list, with
          the view into each and the withdrawal of a queued one. */}
      <RunHistory goal={goal.id} runs={runs} onChanged={onChanged} />
    </div>
  );
}
