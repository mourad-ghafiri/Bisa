/**
 * A `switch` step: a subject rendered as text against its cases — the first
 * whose value is the text names the branch — and `otherwise` when none
 * matches. A branch is renamed through `relabelBranch`, which carries the
 * case, the `otherwise` and every flow along, and commits on blur so a
 * half-typed name never orphans a flow on the way to its name.
 */

import type { Step } from "../../../types";
import { Button, Chip, Field, ICON, TextInput } from "../../../ui";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { freshBranch } from "../workflowGraph.mjs";
import { BranchName } from "./BranchName";
import { t } from "../../../i18n/l10n.mjs";

type Switch = Extract<Step, { kind: "switch" }>;

export function SwitchStepForm({ step, onChange, disabled }: { step: Switch; onChange: (next: Step) => void; disabled?: boolean }) {
  const cases = step.cases ?? [];
  const set = (patch: Partial<Switch>) => onChange({ ...step, ...patch });
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-switch-step-form-words")} hint={t("workflow-switch-step-form-rendered-text-when-step-entered-compared", { TEMPLATE_HINT })}>
        <TextInput className="font-mono" value={step.on} /* for the machine */ placeholder="{steps.triage.output.severity}" disabled={disabled} onChange={(e) => set({ on: e.target.value })} />
      </Field>
      <Field label={t("workflow-switch-step-form-cases")} hint={t("workflow-switch-step-form-first-case-whose-value-text-names")}>
        <div className="flex flex-col gap-2">
          {cases.map((c, i) => (
            <div key={`${c.branch}:${i}`} className="flex flex-wrap items-center gap-2 rounded-control bg-surface-2/50 p-2">
              <Chip tone="quiet">{t("workflow-switch-step-form-case-n", { n: i + 1 })}</Chip>
              <TextInput
                className="w-36 font-mono"
                value={c.value}
                placeholder={t("workflow-switch-step-form-case-value")}
                aria-label={t("workflow-switch-step-form-case-value")}
                disabled={disabled}
                onChange={(e) => set({ cases: cases.map((x, j) => (j === i ? { ...x, value: e.target.value } : x)) })}
              />
              <span className="text-2xs text-text-dim">{t("workflow-decide-step-form-branch")}</span>
              <BranchName step={step} branch={c.branch} disabled={disabled} className="w-32 font-mono" onChange={onChange} />
              <Button
                size="sm"
                variant="ghost"
                className="ml-auto"
                disabled={disabled}
                aria-label={t("workflow-switch-step-form-remove-case")}
                onClick={() =>
                  // The case goes; a flow still labelled with its branch is
                  // the validator's to report, never silently rewritten.
                  set({ cases: cases.filter((_, j) => j !== i) })
                }
              >
                <ICON.delete size={12} aria-hidden />
              </Button>
            </div>
          ))}
          <div>
            <Button size="sm" disabled={disabled} onClick={() => set({ cases: [...cases, { value: "", branch: freshBranch(step) }] })}>
              <ICON.add size={12} aria-hidden />{t("workflow-switch-step-form-add-case")}</Button>
          </div>
        </div>
      </Field>
      <Field label={t("workflow-decide-step-form-otherwise")} hint={t("workflow-switch-step-form-branch-when-no-case-matches")}>
        <BranchName step={step} branch={step.otherwise} disabled={disabled} className="w-40 font-mono" onChange={onChange} />
      </Field>
    </div>
  );
}
