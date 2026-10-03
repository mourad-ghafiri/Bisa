/**
 * A `judge` step: the Decision-Making Agent reads `state` rendered against
 * the run, and picks one of `options` by what each means — `otherwise` when
 * it is not sure enough, or gives no answer that holds to the decision
 * contract. Naming the step *is* switching the Decision-Making Agent on for
 * it (workflow.judge is selected explicitly), so there is no switch here,
 * only the question.
 *
 * A branch is renamed through `relabelBranch`, which carries the option, the
 * `otherwise` and every flow along, and commits on blur — the same pattern
 * `SwitchStepForm` and `DecideStepForm` use, so a half-typed name never
 * orphans a flow on the way to its name. How sure a pick must be is
 * committed when the field is left too: a number from 0 to 1, or nothing —
 * what the node would refuse never reaches the step. The rules are
 * `judgeStepModel.mjs`'s; this draws.
 */

import { useEffect, useState } from "react";
import type { Step } from "../../../types";
import { Button, Chip, Field, ICON, TextArea, TextInput } from "../../../ui";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { BranchName } from "./BranchName";
import { addOption, confidenceText, judgeWords, removeOption, setConfidence, setMeaning } from "./judgeStepModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Judge = Extract<Step, { kind: "judge" }>;

/** How sure a pick must be, committed on blur; a refused number reverts and says why. */
function Confidence({ step, disabled, onChange }: { step: Judge; disabled?: boolean; onChange: (next: Step) => void }) {
  const stored = confidenceText(step);
  const [draft, setDraft] = useState(stored);
  const [why, setWhy] = useState<string | null>(null);
  useEffect(() => setDraft(stored), [stored]);
  const commit = () => {
    const r = setConfidence(step, draft);
    if (r.ok) {
      setWhy(null);
      if (r.step !== step) onChange(r.step);
    } else {
      setWhy(r.reason);
      setDraft(stored);
    }
  };
  return (
    <span className="flex flex-col gap-0.5">
      <TextInput
        inputMode="decimal"
        className="w-24 tnum"
        value={draft}
        placeholder="0.70"
        aria-invalid={why !== null}
        disabled={disabled}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
        }}
      />
      {why && <span className="text-2xs text-danger">{why}</span>}
    </span>
  );
}

export function JudgeStepForm({ step, onChange, disabled }: { step: Judge; onChange: (next: Step) => void; disabled?: boolean }) {
  const options = step.options ?? [];
  const set = (patch: Partial<Judge>) => onChange({ ...step, ...patch });
  const words = judgeWords(step);
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-judge-step-form-state")} hint={t("workflow-judge-step-form-rendered-text-when-step-entered-what", { TEMPLATE_HINT })}>
        <TextArea
          className="font-mono"
          rows={2}
          value={step.state}
          placeholder="{steps.triage.output.summary}" // for the machine
          disabled={disabled}
          onChange={(e) => set({ state: e.target.value })}
        />
      </Field>
      <Field label={t("workflow-agent-step-form-instructions")} hint={t("workflow-judge-step-form-question-sentence-decision-making-agent-reads-against")}>
        <TextArea
          rows={2}
          value={step.instructions}
          placeholder={t("workflow-judge-step-form-which-branch-does-call")}
          disabled={disabled}
          onChange={(e) => set({ instructions: e.target.value })}
        />
      </Field>
      <Field label={t("workflow-human-step-form-options")} hint={t("workflow-judge-step-form-what-each-branch-means-decision-making-agent")}>
        <div className="flex flex-col gap-2">
          {options.map((o, i) => (
            <div key={`${o.branch}:${i}`} className="flex flex-col gap-1.5 rounded-control bg-surface-2/50 p-2">
              <div className="flex items-center gap-2">
                <Chip tone="quiet">{t("workflow-judge-step-form-option-n", { n: i + 1 })}</Chip>
                <span className="text-2xs text-text-dim">{t("workflow-decide-step-form-branch")}</span>
                <BranchName step={step} branch={o.branch} disabled={disabled} className="w-32 font-mono" onChange={onChange} />
                <Button
                  size="sm"
                  variant="ghost"
                  className="ml-auto"
                  disabled={disabled}
                  aria-label={t("workflow-judge-step-form-remove-option")}
                  onClick={() => onChange(removeOption(step, i))}
                >
                  <ICON.delete size={12} aria-hidden />
                </Button>
              </div>
              <TextInput
                value={o.meaning}
                placeholder={t("workflow-judge-step-form-what-choosing-branch-means")}
                aria-label={t("workflow-judge-step-form-what-choosing-branch-means")}
                disabled={disabled}
                onChange={(e) => onChange(setMeaning(step, i, e.target.value))}
              />
            </div>
          ))}
          <div>
            <Button size="sm" disabled={disabled} onClick={() => onChange(addOption(step))}>
              <ICON.add size={12} aria-hidden />{t("workflow-human-step-form-add-option")}</Button>
          </div>
          {words.options && <p className="text-2xs text-warn">{words.options}</p>}
        </div>
      </Field>
      <Field label={t("workflow-decide-step-form-otherwise")} hint={t("workflow-judge-step-form-branch-when-not-sure-enough-gives")}>
        <BranchName step={step} branch={step.otherwise} disabled={disabled} className="w-40 font-mono" onChange={onChange} />
        <p className="mt-1 text-2xs text-text-dim">{words.otherwise}</p>
      </Field>
      <Field label={t("workflow-judge-step-form-minimum-confidence")} hint={t("workflow-judge-step-form-minimum-confidence-hint")}>
        <Confidence step={step} disabled={disabled} onChange={onChange} />
      </Field>
    </div>
  );
}
