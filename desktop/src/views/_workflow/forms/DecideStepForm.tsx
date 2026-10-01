/**
 * A decide step: rules in order, each choosing a branch, and the branch when
 * none holds — and which rules choose: *the first rule that holds* (one
 * branch), or *every rule that holds* (as many branches as hold, their
 * flows all taken at once; `otherwise` only when none does). The branches
 * are what the step's outgoing flows carry — so a branch is renamed through
 * `relabelBranch`, which carries the rule, the `otherwise` and every flow
 * along, and commits on blur rather than per keystroke: a half-typed name
 * would otherwise orphan the flow three times on the way to its name.
 */

import type { InputDef, Rule, Step } from "../../../types";
import { Button, Chip, Field, ICON, Select } from "../../../ui";
import { DECIDE_PICKS } from "../stepKinds.mjs";
import { freshBranch } from "../workflowGraph.mjs";
import { BranchName } from "./BranchName";
import { ConditionEditor } from "./ConditionEditor";
import { freshCondition } from "./conditionModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Decide = Extract<Step, { kind: "decide" }>;

export function DecideStepForm({
  step,
  upstream,
  inputs,
  onChange,
  disabled,
}: {
  step: Decide;
  upstream: Step[];
  inputs: InputDef[];
  onChange: (next: Step) => void;
  disabled?: boolean;
}) {
  const rules = step.rules ?? [];
  const set = (patch: Partial<Decide>) => onChange({ ...step, ...patch });
  const patch = (i: number, r: Partial<Rule>) => set({ rules: rules.map((x, j) => (j === i ? { ...x, ...r } : x)) });
  const every = step.pick === "every";
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-decide-step-form-choose")}>
        <Select
          value={every ? "every" : "first"}
          disabled={disabled}
          onChange={(e) => {
            // The first rule is the default: unwritten, as the core writes it.
            const { pick: _pick, ...rest } = step;
            onChange(e.target.value === "every" ? { ...rest, pick: "every" } : (rest as Decide));
          }}
        >
          {DECIDE_PICKS.map((p) => (
            <option key={p.pick} value={p.pick}>
              {p.label}
            </option>
          ))}
        </Select>
      </Field>
      <Field label={t("workflow-decide-step-form-rules")} hint={every ? t("workflow-decide-step-form-every-holds-takes-branch") : t("workflow-decide-step-form-first-holds-names-branch-draw-flow")}>
        <div className="flex flex-col gap-2">
          {rules.map((r, i) => (
            <div key={`${r.branch}:${i}`} className="rounded-control border border-border p-2">
              <div className="mb-1.5 flex items-center gap-2">
                <Chip tone="quiet">{t("workflow-decide-step-form-rule-n", { n: i + 1 })}</Chip>
                <span className="text-2xs text-text-dim">{t("workflow-decide-step-form-branch")}</span>
                <BranchName step={step} branch={r.branch} disabled={disabled} className="w-32 font-mono" onChange={onChange} />
                <Button
                  size="sm"
                  variant="ghost"
                  className="ml-auto"
                  disabled={disabled}
                  aria-label={t("workflow-decide-step-form-remove-rule")}
                  onClick={() =>
                    // The rule goes; a flow still labelled with its branch is
                    // the validator's to report, never silently rewritten.
                    set({ rules: rules.filter((_, j) => j !== i) })
                  }
                >
                  <ICON.delete size={12} aria-hidden />
                </Button>
              </div>
              <ConditionEditor value={r.when} upstream={upstream} inputs={inputs} disabled={disabled} onChange={(when) => patch(i, { when })} />
            </div>
          ))}
          <div>
            <Button
              size="sm"
              disabled={disabled}
              onClick={() =>
                set({
                  rules: [
                    ...rules,
                    { when: freshCondition(upstream, inputs), branch: freshBranch(step) },
                  ],
                })
              }
            >
              <ICON.add size={12} aria-hidden />{t("workflow-decide-step-form-add-rule")}</Button>
          </div>
        </div>
      </Field>
      <Field label={t("workflow-decide-step-form-otherwise")} hint={t("workflow-decide-step-form-branch-when-no-rule-holds")}>
        <BranchName step={step} branch={step.otherwise} disabled={disabled} className="w-40 font-mono" onChange={onChange} />
      </Field>
    </div>
  );
}
