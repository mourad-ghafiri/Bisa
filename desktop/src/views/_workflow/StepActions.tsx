/**
 * What a person can do to the selected step of a run — a goal's or a run of
 * the workspace: answer a human step, mark it done by hand, release a wait,
 * decide an approval (through its gate, pinned above the conversation), or
 * open the agent's work item.
 *
 * The verbs come from `runView.mjs`'s `stepActions`; this is paint over
 * `api.answerStep`, `api.markStepDone` and `api.releaseStep`, each addressed
 * to the run by its id.
 */

import { useState } from "react";
import { api } from "../../api";
import type { Step, StepRecord } from "../../types";
import { Button, Checkbox, ICON, TextArea, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { stepActions } from "./runView.mjs";
import { t } from "../../i18n/l10n.mjs";

export function StepActions({
  run: runId,
  step,
  record,
  onChanged,
  onOpenItem,
  onDecide,
}: {
  /** The run the step belongs to. */
  run: string;
  step: Step;
  record: StepRecord | null | undefined;
  onChanged: () => void;
  onOpenItem?: (item: string) => void;
  /** Scroll to the pinned gate — the decision is signed there. */
  onDecide?: () => void;
}) {
  const toast = useToast();
  const actions = stepActions(step, record);
  const [answer, setAnswer] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  if (actions.length === 0) return null;
  const run = (label: string, fn: () => Promise<unknown>) => async () => {
    setBusy(true);
    await attempt(fn, toast.error, () => {
      toast.ok(label);
      onChanged();
    });
    setBusy(false);
  };
  const options = step.kind === "human" ? (step.options ?? []) : [];
  const multi = step.kind === "human" && step.multi === true;
  return (
    <div className="flex flex-col gap-2 rounded-card border border-warn/40 bg-warn-soft/40 p-3">
      <p className="text-2xs font-semibold tracking-wide text-warn uppercase">{t("workflow-step-actions-step-yours")}</p>
      {actions.includes("answer") && (
        <>
          {options.length > 0 && !multi && (
            <div className="flex flex-wrap gap-1.5">
              {options.map((o) => (
                <Button key={o.id} size="sm" disabled={busy} onClick={run(t("workflow-step-actions-answered", { o: o.label }), () => api.answerStep(runId, step.id, { selected: [o.id], text: answer.trim() || null, unsure: false }))}>
                  {o.label}
                </Button>
              ))}
            </div>
          )}
          {options.length > 0 && multi && (
            // The question allows several: each option is a box, and one
            // button sends what is ticked.
            <div className="flex flex-col gap-1">
              <div className="flex flex-wrap gap-2">
                {options.map((o) => (
                  <Checkbox
                    key={o.id}
                    label={o.label}
                    checked={picked.includes(o.id)}
                    disabled={busy}
                    onChange={(on) => setPicked(on ? [...picked, o.id] : picked.filter((id) => id !== o.id))}
                  />
                ))}
              </div>
              <div>
                <Button
                  size="sm"
                  disabled={busy || picked.length === 0}
                  onClick={run(t("workflow-step-actions-answered-one-option-options", { picked: picked.length }), () =>
                    api.answerStep(runId, step.id, { selected: picked, text: answer.trim() || null, unsure: false }),
                  )}
                >
                  {t("workflow-step-actions-answer-with", { picked: picked.length })}
                </Button>
              </div>
            </div>
          )}
          <TextArea rows={2} value={answer} placeholder={options.length > 0 ? t("workflow-step-actions-anything-options-miss") : t("workflow-step-actions-answer")} disabled={busy} onChange={(e) => setAnswer(e.target.value)} />
          <div className="flex flex-wrap gap-2">
            <Button variant="primary" size="sm" disabled={busy || !answer.trim()} onClick={run(t("workflow-step-actions-answered-2"), () => api.answerStep(runId, step.id, { selected: [], text: answer.trim(), unsure: false }))}>{t("workflow-step-actions-send-answer")}</Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={run(t("workflow-step-actions-said-not-sure"), () => api.answerStep(runId, step.id, { selected: [], text: null, unsure: true }))}>{t("workflow-step-actions-i-m-not-sure")}</Button>
            {actions.includes("done") && (
              <Button size="sm" className="ml-auto" disabled={busy} onClick={run(t("workflow-step-actions-marked-done"), () => api.markStepDone(runId, step.id))}>
                <ICON.ok size={12} aria-hidden />{t("workflow-step-actions-done-hand")}</Button>
            )}
          </div>
        </>
      )}
      {actions.includes("decide") && (
        <div className="flex items-center gap-2">
          <p className="min-w-0 flex-1 text-2xs text-text-dim">{t("workflow-step-actions-approval-signed-decision-decide-gate-pinned")}</p>
          {onDecide && (
            <Button size="sm" variant="primary" onClick={onDecide}>{t("workflow-step-actions-go-gate")}</Button>
          )}
        </div>
      )}
      {actions.includes("release") && (
        <div className="flex items-center gap-2">
          <p className="min-w-0 flex-1 text-2xs text-text-dim">{t("workflow-step-actions-run-holds-here-until-release")}</p>
          <Button size="sm" variant="primary" disabled={busy} onClick={run(t("workflow-step-actions-released"), () => api.releaseStep(runId, step.id))}>{t("workflow-step-actions-release")}</Button>
        </div>
      )}
      {actions.includes("open") && record?.work_item && onOpenItem && (
        <div className="flex items-center gap-2">
          <p className="min-w-0 flex-1 text-2xs text-text-dim">{t("workflow-step-actions-agent-step")}</p>
          <Button size="sm" onClick={() => onOpenItem(record.work_item!)}>{t("workflow-step-actions-open-work-item")}</Button>
        </div>
      )}
    </div>
  );
}
