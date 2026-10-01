/**
 * A `for_each` step: a template that renders to a JSON array, walked one item
 * at a time down the `each` flow — the body reads `{steps.<id>.output.item}`
 * and its last step flows back here — then `done`. A list longer than the
 * bound fails the step before the first item.
 */

import type { Step } from "../../../types";
import { Field, NumberInput, TextInput } from "../../../ui";
import { DEFAULT_MAX_ITERATIONS, TEMPLATE_HINT } from "../stepKinds.mjs";
import { t } from "../../../i18n/l10n.mjs";

type ForEach = Extract<Step, { kind: "for_each" }>;

export function ForEachStepForm({ step, onChange, disabled }: { step: ForEach; onChange: (next: Step) => void; disabled?: boolean }) {
  return (
    <div className="flex flex-col gap-3">
      <Field
        label={t("workflow-for-each-step-form-items")}
        hint={t("workflow-for-each-step-form-template-renders-json-array-upstream-output", { step: step.id, TEMPLATE_HINT })}
      >
        <TextInput className="font-mono" value={step.items} /* for the machine */ placeholder="{steps.list.output.items}" disabled={disabled} onChange={(e) => onChange({ ...step, items: e.target.value })} />
      </Field>
      <Field label={t("workflow-for-each-step-form-max-iterations")} hint={t("workflow-for-each-step-form-longer-list-fails-step-before-first", { DEFAULT_MAX_ITERATIONS })}>
        <NumberInput
          className="w-28"
          min={1}
          max={65535}
          value={step.max_iterations ?? DEFAULT_MAX_ITERATIONS}
          disabled={disabled}
          aria-label={t("workflow-for-each-step-form-max-iterations-2")}
          onCommit={(v) => onChange({ ...step, max_iterations: Math.max(1, Math.min(65535, Math.trunc(v))) })}
        />
      </Field>
    </div>
  );
}
