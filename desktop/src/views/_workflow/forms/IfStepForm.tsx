/**
 * An `if` step: one condition, two flows. The branches are the fixed words
 * `yes` and `no` — drawn from the step's two handles — so there is nothing to
 * name here, only the condition to state.
 */

import type { InputDef, Step } from "../../../types";
import { Labelled } from "../../../ui";
import { ConditionEditor } from "./ConditionEditor";
import { t } from "../../../i18n/l10n.mjs";

type If = Extract<Step, { kind: "if" }>;

export function IfStepForm({
  step,
  upstream,
  inputs,
  onChange,
  disabled,
}: {
  step: If;
  upstream: Step[];
  inputs: InputDef[];
  onChange: (next: Step) => void;
  disabled?: boolean;
}) {
  return (
    <Labelled label={t("workflow-if-step-form-when")} hint={t("workflow-if-step-form-holds-yes-flow-does-not-no")}>
      <ConditionEditor value={step.when} upstream={upstream} inputs={inputs} disabled={disabled} onChange={(when) => onChange({ ...step, when })} />
    </Labelled>
  );
}
