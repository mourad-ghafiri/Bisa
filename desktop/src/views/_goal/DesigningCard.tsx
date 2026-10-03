/**
 * What a goal shows before it has a run. On an auto or guided goal: where
 * the Workflow Agent stands, for how long, what it is doing right now, and
 * the moves a person has — retry the design, pick a workflow, ask the agent,
 * review the proposal. On a manual goal: that the design is theirs, with the
 * door to the designer. The facts are `designStatus.mjs`'s; this paints them
 * and listens to the bus so the card moves without a refetch.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { ModelLine } from "../../activityModel.mjs";
import { navigate } from "../../router";
import type { DesignStatus, GoalView } from "../../types";
import { Button, Chip, ICON, WorkingDot, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { WorkflowPicker } from "../_workflow/WorkflowPicker";
import { activityFrom, applyGuidedFrame, designView, kindWord, ticks } from "./designStatus.mjs";
import { useVisible } from "../../shell/visibility";
import { t as tr } from "../../i18n/l10n.mjs";

export function DesigningCard({ view, onChanged }: { view: GoalView; onChanged: () => void }) {
  const toast = useToast();
  const { goal, guidance } = view;
  const [design, setDesign] = useState<DesignStatus | null>(guidance.design ?? null);
  const [activity, setActivity] = useState<ModelLine | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const [busy, setBusy] = useState(false);

  // The node's word wins whenever the page refetches; the bus fills in between.
  // Keyed on what the word *says*, not on the object a refetch rebuilds, so an
  // unchanged status does not overwrite the frame the bus already applied.
  const designStatus = guidance.design?.status ?? null;
  const designSince = guidance.design?.since ?? null;
  // Keyed on the two facts above, never the `guidance.design` object a refetch rebuilds.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => setDesign(guidance.design ?? null), [designStatus, designSince]);
  useEngineEvents((e) => {
    const p = e.payload;
    if (p.type === "guided") setDesign((d) => applyGuidedFrame(d, p, Math.floor(Date.now() / 1000)));
    else if (p.type === "session") setActivity((prev) => activityFrom(p, prev));
  }, goal.id);

  const v = designView({ ...guidance, design }, goal, { now, activity });
  const live = ticks(v.kind);
  // A hidden window needs no ticking clock; it resumes on wake.
  const visible = useVisible();
  useEffect(() => {
    if (!live || !visible) return;
    setNow(Math.floor(Date.now() / 1000));
    const t = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(t);
  }, [live, visible]);

  const retry = async () => {
    setBusy(true);
    await attempt(
      () => api.designGoal(goal.id),
      toast.error,
      () => {
        toast.ok(tr("goal-designing-card-asked-workflow-agent-design-again"));
        onChanged();
      },
    );
    setBusy(false);
  };

  return (
    <div className="mx-auto w-full max-w-2xl p-6">
      <div className="flex flex-col gap-3 rounded-card border border-border bg-surface p-4">
        <div className="flex items-center gap-2">
          {v.kind === "working" || v.kind === "scheduled" ? (
            <WorkingDot title={tr("goal-designing-card-workflow-agent-designing")} />
          ) : (
            <ICON.coreAgent size={14} aria-hidden className="text-text-dim" />
          )}
          <h3 className="min-w-0 flex-1 text-sm font-semibold">{v.headline}</h3>
          <Chip tone={v.tone}>{kindWord(v.kind)}</Chip>
          {v.elapsed && <span className="tnum text-2xs text-text-dim">{v.elapsed}</span>}
        </div>
        {v.hint && <p className="max-w-measure text-xs leading-relaxed text-text-dim">{v.hint}</p>}
        {v.activity && (
          <p className="flex items-center gap-1.5 text-2xs text-text-dim">
            <ICON.working size={12} aria-hidden />
            <span className="truncate">{v.activity.text}</span>
          </p>
        )}
        <div className="flex flex-wrap items-center gap-2">
          {v.offerRetry && (
            <Button size="sm" variant={v.primary === "retry" ? "primary" : "default"} disabled={busy} onClick={() => void retry()}>
              <ICON.redo size={12} aria-hidden />{tr("goal-designing-card-retry-design")}</Button>
          )}
          {v.primary === "review" && (
            <Button size="sm" variant="primary" onClick={() => navigate({ name: "goal", id: goal.id }, { tab: "workflow" })}>
              <ICON.workflow size={12} aria-hidden />{tr("goal-designing-card-review-proposal")}</Button>
          )}
          {v.primary === "design" && (
            <Button size="sm" variant="primary" onClick={() => navigate({ name: "goal", id: goal.id }, { tab: "workflow", edit: "1" })}>
              <ICON.edit size={12} aria-hidden />{tr("goal-designing-card-design-workflow")}</Button>
          )}
          {v.offerAsk && (
            <Button size="sm" onClick={() => navigate({ name: "goal", id: goal.id }, { tab: "conversation" })}>
              <ICON.coreAgent size={12} aria-hidden />{tr("goal-designing-card-talk-workflow-agent")}</Button>
          )}
        </div>
        {v.offerPick && (
          <div className="flex flex-col gap-1 border-t border-hairline pt-3">
            <span className="text-2xs text-text-dim">
              {v.primary === "pick" ? tr("goal-designing-card-pick-workflow") : tr("goal-designing-card-pick-workflow-yourself-instead")}
            </span>
            <div className="w-72">
              <WorkflowPicker
                goal={goal.id}
                value={null}
                onChange={(id) => id && void attempt(() => api.setGoalWorkflow(goal.id, { workflow: id }), toast.error, onChanged)}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
