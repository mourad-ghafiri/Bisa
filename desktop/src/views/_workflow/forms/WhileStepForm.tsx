/**
 * A `while` step: a condition tested on every entry, the first included, and
 * a bound on how many times the loop flow may be taken. The body's last step
 * flows back into this one; `done` continues when the condition stops
 * holding. The bound is the step's own — `max_visits` counts how often the
 * whole loop starts, not its turns.
 */

import type { InputDef, Step } from "../../../types";
import { Field, NumberInput } from "../../../ui";
import { DEFAULT_MAX_ITERATIONS } from "../stepKinds.mjs";
import { ConditionEditor } from "./ConditionEditor";
import { t } from "../../../i18n/l10n.mjs";

type While = Extract<Step, { kind: "while" }>;

export function WhileStepForm({
  step,
  upstream,
  inputs,
  onChange,
  disabled,
}: {
  step: While;
  upstream: Step[];
  inputs: InputDef[];
  onChange: (next: Step) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-step-kinds-while")} hint={t("workflow-while-step-form-tested-every-entry-holds-loop-flow")}>
        <ConditionEditor value={step.when} upstream={upstream} inputs={inputs} disabled={disabled} onChange={(when) => onChange({ ...step, when })} />
      </Field>
      <Field label={t("workflow-for-each-step-form-max-iterations")} hint={t("workflow-while-step-form-reaching-fails-step-unless-say-otherwise", { DEFAULT_MAX_ITERATIONS })}>
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
