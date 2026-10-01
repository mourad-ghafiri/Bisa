/**
 * A run filter's fields — a run of which workflow, ending how — shared by a
 * start that begins when a run finishes and a wait that holds for one. Any
 * workflow and any end when left so.
 */

import type { RunEnd } from "../../../types";
import { Field, Select } from "../../../ui";
import { RUN_ENDS } from "../stepKinds.mjs";
import { WorkflowPicker } from "../WorkflowPicker";
import { t } from "../../../i18n/l10n.mjs";

export interface RunFilter {
  workflow?: string | null;
  outcome?: RunEnd | null;
}

export function RunFilterFields<F extends RunFilter>({ value, onChange, disabled }: { value: F; onChange: (next: F) => void; disabled?: boolean }) {
  const set = (patch: Partial<RunFilter>) => onChange({ ...value, ...patch });
  return (
    <div className="grid gap-3 md:grid-cols-2">
      <Field label={t("workflow-spawn-step-form-workflow")} hint={t("workflow-run-filter-fields-workflow-hint")}>
        <WorkflowPicker value={value.workflow ?? null} disabled={disabled} allowNone noneLabel={t("workflow-run-filter-fields-any-workflow")} onChange={(id) => set({ workflow: id })} />
      </Field>
      <Field label={t("workflow-run-filter-fields-ending")}>
        <Select value={value.outcome ?? ""} disabled={disabled} onChange={(e) => set({ outcome: (e.target.value || null) as RunEnd | null })}>
          <option value="">{t("workflow-run-filter-fields-any-end")}</option>
          {RUN_ENDS.map((r) => (
            <option key={r.outcome} value={r.outcome}>
              {r.label}
            </option>
          ))}
        </Select>
      </Field>
    </div>
  );
}
