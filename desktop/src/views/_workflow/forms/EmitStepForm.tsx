/**
 * An emit step: a named signal the run raises — heard by a start that
 * begins on it, a wait that holds for it, a boundary event that hears it —
 * with a payload of templates rendered against the run. Its output is
 * `{ signal }`. Raised once however many times a restart runs it again; it
 * never fails because nobody listens.
 */

import type { Step } from "../../../types";
import { Field, Labelled, TextInput } from "../../../ui";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { ExactFieldsEditor } from "./ExactFieldsEditor";
import { validSignalName } from "./startForm.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Emit = Extract<Step, { kind: "emit" }>;

export function EmitStepForm({ step, onChange, disabled }: { step: Emit; onChange: (next: Step) => void; disabled?: boolean }) {
  const set = (patch: Partial<Emit>) => onChange({ ...step, ...patch });
  // A name may be a template; only a literal one is held to the words here — the validator judges the rest.
  const literal = !step.signal.includes("{");
  const bad = literal && step.signal !== "" && !validSignalName(step.signal);
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-signal-filter-fields-name")} hint={bad ? t("workflow-signal-filter-fields-name-not-dotted-words") : t("workflow-emit-step-form-signal-hint", { TEMPLATE_HINT })}>
        <TextInput className="font-mono" value={step.signal} /* for the machine */ placeholder="report.ready" disabled={disabled} onChange={(e) => set({ signal: e.target.value })} />
      </Field>
      <Labelled label={t("workflow-emit-step-form-payload")} hint={t("workflow-emit-step-form-payload-hint", { TEMPLATE_HINT })}>
        <ExactFieldsEditor
          value={step.payload}
          disabled={disabled}
          onChange={(payload) => {
            // An empty payload is none, as the core writes it.
            if (Object.keys(payload).length > 0) set({ payload });
            else {
              const { payload: _payload, ...rest } = step;
              onChange(rest as Emit);
            }
          }}
        />
      </Labelled>
      <p className="text-2xs text-text-dim">{t("workflow-emit-step-form-heard-by", { step: step.id })}</p>
    </div>
  );
}
