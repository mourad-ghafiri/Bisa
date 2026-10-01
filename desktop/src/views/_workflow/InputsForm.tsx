/**
 * The inputs a run is started with, as fields.
 *
 * One control per declared input, chosen by kind: a text box, a number, a
 * switch, a choice, the assignee picker, a project select — every project
 * of the workspace — and, for an `account`, this machine's accounts of the
 * input's connector. What each may hold, what is sent and what the line
 * under a field says — a project given to a goal's work is attached to the
 * goal by the start (`home`) — is decided in `workflowForm.mjs`; this is
 * paint.
 */

import type { InputDef } from "../../types";
import { Checkbox, Field, Select, TextInput } from "../../ui";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { AssigneePicker } from "../_work/AssigneePicker";
import { useConnectorDetail } from "./useConnectors";
import { accountChoices, inputHint, type InputValues, type RunHome } from "./workflowForm.mjs";
import { t } from "../../i18n/l10n.mjs";

/** An `account` input: this machine's accounts of the input's connector, the default first. */
function AccountField({ connector, value, disabled, onChange }: { connector: string | null; value: string; disabled?: boolean; onChange: (id: string) => void }) {
  const detail = useConnectorDetail(connector);
  const accounts = accountChoices(detail.data?.accounts);
  return (
    <>
      <Select value={value} disabled={disabled || accounts.length === 0} onChange={(e) => onChange(e.target.value)}>
        <option value="">{t("workflow-inputs-form-pick-account")}</option>
        {accounts.map((a) => (
          <option key={a.id} value={a.id}>
            {t("workflow-connector-step-form-account-option", { label: a.label, default: a.isDefault ? "yes" : "no" })}
          </option>
        ))}
      </Select>
      {connector !== null && !detail.loading && accounts.length === 0 && <p className="mt-1 text-2xs text-text-dim">{t("workflow-inputs-form-no-account-on-machine", { connector })}</p>}
    </>
  );
}

export function InputsForm({
  inputs,
  values,
  errors,
  home,
  onChange,
  disabled,
}: {
  inputs: InputDef[];
  values: InputValues;
  errors: Record<string, string>;
  /** Whom the run is for: what giving it a project does is said under the field. */
  home: RunHome;
  onChange: (next: InputValues) => void;
  disabled?: boolean;
}) {
  const ws = useWorkspace();
  const set = (name: string, v: unknown) => onChange({ ...values, [name]: v });
  if (inputs.length === 0) {
    return <p className="text-2xs text-text-dim">{t("workflow-inputs-form-workflow-takes-no-inputs")}</p>;
  }
  return (
    <div className="flex flex-col gap-3">
      {inputs.map((i) => {
        const v = values[i.name];
        const hint = inputHint(i, errors[i.name], home);
        const label = `${i.label || i.name}`;
        switch (i.kind) {
          case "bool":
            return (
              <Checkbox
                key={i.name}
                label={label}
                hint={hint}
                checked={Boolean(v)}
                disabled={disabled}
                onChange={(c) => set(i.name, c)}
              />
            );
          case "account":
            return (
              <Field key={i.name} label={label} hint={hint}>
                <AccountField connector={i.connector ?? null} value={String(v ?? "")} disabled={disabled} onChange={(id) => set(i.name, id)} />
              </Field>
            );
          case "choice":
            return (
              <Field key={i.name} label={label} hint={hint}>
                <Select value={String(v ?? "")} disabled={disabled} onChange={(e) => set(i.name, e.target.value)}>
                  <option value="">—</option>
                  {(i.options ?? []).map((o) => (
                    <option key={o} value={o}>
                      {o}
                    </option>
                  ))}
                </Select>
              </Field>
            );
          case "assignee":
            return (
              <Field key={i.name} label={label} hint={hint}>
                <AssigneePicker
                  value={v ? [String(v)] : []}
                  max={1}
                  disabled={disabled}
                  onChange={(next) => set(i.name, next[0] ?? "")}
                />
              </Field>
            );
          case "project":
            return (
              <Field key={i.name} label={label} hint={hint}>
                <Select value={String(v ?? "")} disabled={disabled} onChange={(e) => set(i.name, e.target.value)}>
                  <option value="">{t("workflow-inputs-form-pick-project")}</option>
                  {ws.projects.map((p) => (
                    <option key={p.project.id} value={p.project.id}>
                      {p.project.name}
                    </option>
                  ))}
                </Select>
              </Field>
            );
          case "number":
            return (
              <Field key={i.name} label={label} hint={hint}>
                <TextInput
                  inputMode="decimal"
                  className="w-40"
                  value={v === undefined || v === null ? "" : String(v)}
                  disabled={disabled}
                  onChange={(e) => set(i.name, e.target.value)}
                />
              </Field>
            );
          default:
            return (
              <Field key={i.name} label={label} hint={hint}>
                <TextInput
                  value={v === undefined || v === null ? "" : String(v)}
                  disabled={disabled}
                  onChange={(e) => set(i.name, e.target.value)}
                />
              </Field>
            );
        }
      })}
    </div>
  );
}
