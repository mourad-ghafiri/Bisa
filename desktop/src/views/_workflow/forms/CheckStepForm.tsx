/**
 * A check step: a command that exits 0, or a JSON schema over an upstream
 * step's output.
 */

import type { InputDef, Step } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { Field, Select, TextInput } from "../../../ui";
import { CHECKS, TEMPLATE_HINT } from "../stepKinds.mjs";
import { ValueRefField } from "./AgentStepForm";
import type { ProjectRef } from "./eventFilters";
import { JsonField } from "./JsonField";
import { t } from "../../../i18n/l10n.mjs";

type Check = Extract<Step, { kind: "check" }>;
/** A command check as the wire carries it: `project` names where it runs — an id, or an input of kind project — and is typed by hand until `just gen-types`. */
type Command = Extract<Check["check"], { check: "command" }> & { project?: ProjectRef | null };

export function CheckStepForm({
  step,
  inputs,
  upstream,
  onChange,
  disabled,
}: {
  step: Check;
  /** The workflow's inputs — one of kind project may name where a command runs. */
  inputs: InputDef[];
  /** Steps that run before this one — the only ones a schema may read. */
  upstream: Step[];
  onChange: (next: Step) => void;
  disabled?: boolean;
}) {
  const ws = useWorkspace();
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
        <>
          <Field label={t("workflow-check-step-form-command")} hint={t("workflow-check-step-form-runs-goal-s-work-folder-node", { TEMPLATE_HINT })}>
            <TextInput className="font-mono" value={c.command} disabled={disabled} onChange={(e) => onChange({ ...step, check: { ...c, command: e.target.value } })} />
          </Field>
          <ValueRefField
            label={t("workflow-check-step-form-project")}
            hint={t("workflow-check-step-form-where-command-runs")}
            value={(c as Command).project ?? null}
            inputs={inputs}
            kind="project"
            disabled={disabled}
            onChange={(v) => onChange({ ...step, check: { ...c, project: v } as Command })}
          >
            {(fixed, setFixed) => (
              <Select value={fixed} disabled={disabled} onChange={(e) => setFixed(e.target.value)}>
                <option value="">{t("workflow-check-step-form-where-work-landed")}</option>
                {ws.projects.map((p) => (
                  <option key={p.project.id} value={p.project.id}>
                    {p.project.name}
                  </option>
                ))}
              </Select>
            )}
          </ValueRefField>
        </>
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
