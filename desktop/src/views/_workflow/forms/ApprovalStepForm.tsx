/** An approval step: the one sentence a person signs yes or no to. */

import type { Step } from "../../../types";
import { Field, TextArea } from "../../../ui";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Approval = Extract<Step, { kind: "approval" }>;

export function ApprovalStepForm({ step, onChange, disabled }: { step: Approval; onChange: (next: Step) => void; disabled?: boolean }) {
  return (
    <Field label={t("workflow-approval-step-form-prompt")} hint={t("workflow-approval-step-form-what-being-approved-declined-failure-step", { TEMPLATE_HINT })}>
      <TextArea rows={3} value={step.prompt} disabled={disabled} onChange={(e) => onChange({ ...step, prompt: e.target.value })} />
    </Field>
  );
}
