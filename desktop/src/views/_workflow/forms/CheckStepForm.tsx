/**
 * A check step: a command that exits 0, or a JSON schema over an upstream
 * step's output.
 */

import type { Step } from "../../../types";
import { Field, Select, TextInput } from "../../../ui";
import { CHECKS, TEMPLATE_HINT } from "../stepKinds.mjs";
import { JsonField } from "./JsonField";
import { t } from "../../../i18n/l10n.mjs";

type Check = Extract<Step, { kind: "check" }>;

export function CheckStepForm({
  step,
  upstream,
  onChange,
  disabled,
}: {
  step: Check;
  /** Steps that run before this one — the only ones a schema may read. */
  upstream: Step[];
  onChange: (next: Step) => void;
  disabled?: boolean;
}) {
  const c = step.check;
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-check-step-form-judged")}>
        <Select
          value={c.check}
          disabled={disabled}
          onChange={(e) =>
            onChange({
              ...step,
              check:
                e.target.value === "schema"
                  ? { check: "schema", schema: { type: "object" }, of: upstream[0]?.id ?? null }
                  : { check: "command", command: "" },
            })
          }
        >
          {CHECKS.map((k) => (
            <option key={k.check} value={k.check}>
              {k.label}
            </option>
          ))}
        </Select>
      </Field>
      {c.check === "command" ? (
        <Field label={t("workflow-check-step-form-command")} hint={t("workflow-check-step-form-runs-goal-s-work-folder-node", { TEMPLATE_HINT })}>
          <TextInput className="font-mono" value={c.command} disabled={disabled} onChange={(e) => onChange({ ...step, check: { check: "command", command: e.target.value } })} />
        </Field>
      ) : (
        <>
          <Field label={t("workflow-check-step-form-output")} hint={upstream.length === 0 ? t("workflow-check-step-form-nothing-runs-before-step-yet-judges") : t("workflow-check-step-form-upstream-step-whose-result-schema-judges")}>
            <Select value={c.of ?? ""} disabled={disabled} onChange={(e) => onChange({ ...step, check: { ...c, of: e.target.value || null } })}>
              <option value="">{t("workflow-check-step-form-pick-step")}</option>
              {upstream.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name || s.id}
                </option>
              ))}
            </Select>
          </Field>
          <JsonField
            label={t("workflow-check-step-form-json-schema")}
            hint={t("workflow-check-step-form-what-step-s-output-must-satisfy")}
            value={c.schema}
            disabled={disabled}
            onCommit={(schema) => onChange({ ...step, check: { ...c, schema: schema ?? {} } })}
          />
        </>
      )}
    </div>
  );
}
