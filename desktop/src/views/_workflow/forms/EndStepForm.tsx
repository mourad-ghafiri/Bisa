/**
 * An end step: what it ends. *End this path* — the default, exactly a step
 * with nothing after it: the run is done once every path has drained;
 * *Finish the run* — done now, whatever is still live cancelled; *Fail the
 * run* — this step fails, and the run with it.
 */

import type { Finish, Step } from "../../../types";
import { Field, Select } from "../../../ui";
import { END_FINISHES } from "../stepKinds.mjs";
import { t } from "../../../i18n/l10n.mjs";

type End = Extract<Step, { kind: "end" }>;

/** What each finish does, in a sentence under the choice. */
const FINISH_HINT: Record<Finish, () => string> = {
  path: () => t("workflow-end-step-form-path-hint"),
  done: () => t("workflow-end-step-form-done-hint"),
  failed: () => t("workflow-end-step-form-failed-hint"),
};

export function EndStepForm({ step, onChange, disabled }: { step: End; onChange: (next: Step) => void; disabled?: boolean }) {
  const finish: Finish = step.finish ?? "path";
  return (
    <Field label={t("workflow-end-step-form-ends")} hint={FINISH_HINT[finish]()}>
      <Select
        value={finish}
        disabled={disabled}
        onChange={(e) => {
          const next = e.target.value as Finish;
          // The path is the default: unwritten, as the core writes it.
          const { finish: _finish, ...rest } = step;
          onChange(next === "path" ? (rest as End) : { ...rest, finish: next });
        }}
      >
        {END_FINISHES.map((f) => (
          <option key={f.finish} value={f.finish}>
            {f.label}
          </option>
        ))}
      </Select>
    </Field>
  );
}
