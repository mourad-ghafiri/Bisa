/**
 * Where a post lands and who speaks it — the two fields a `notify` step and a
 * boundary's post share: a channel, a goal's or a workstream's conversation,
 * or a text input read at start (blank: the run's own conversation), and the
 * agent whose message it is. Neither commits on unmount: a Select and a
 * reference field commit at once.
 */

import type { InputDef } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { Field, Select } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { ValueRefField } from "./AgentStepForm";
import { pickedAgent, valueOf } from "./assigneeRefModel.mjs";
import { scopeListed, scopeOptions } from "./notifyStepModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

export function NotifyScopeSelect({
  value,
  inputs,
  disabled,
  hint,
  onChange,
}: {
  value: string | null | undefined;
  inputs: InputDef[];
  disabled?: boolean;
  hint: string;
  onChange: (scope: string | null) => void;
}) {
  const ws = useWorkspace();
  const textInputs = inputs.filter((i) => i.kind === "text" || i.kind === "choice");
  const scope = value ?? "";
  const options = scopeOptions({ channels: ws.channels.map((c) => c.channel), goals: ws.goals, workstreams: ws.workstreams, textInputs });
  // A scope the list does not carry — an id gone from the workspace, an input
  // the workflow no longer declares — is shown as it is rather than as the
  // first option, which would be a lie the save then makes true.
  const listed = scopeListed(
    scope,
    options.channels.map((o) => o.value),
    textInputs.map((i) => i.name),
    options.goals.map((o) => o.value),
    options.workstreams.map((o) => o.value),
  );
  return (
    <Field label={t("workflow-notify-step-form-where")} hint={hint}>
      <Select value={scope} disabled={disabled} onChange={(e) => onChange(e.target.value || null)}>
        <option value="">{options.first}</option>
        {options.channels.map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
        {options.goals.length > 0 && (
          <optgroup label={t("workflow-notify-step-form-goals")}>
            {options.goals.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </optgroup>
        )}
        {options.workstreams.length > 0 && (
          <optgroup label={t("workflow-notify-step-form-workstreams")}>
            {options.workstreams.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </optgroup>
        )}
        {options.inputs.length > 0 && (
          <optgroup label={t("workflow-notify-step-form-read-from-input-start")}>
            {options.inputs.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </optgroup>
        )}
        {listed || <option value={scope}>{scope}</option>}
      </Select>
    </Field>
  );
}

export function NotifyAuthorField({
  value,
  inputs,
  disabled,
  hint,
  onChange,
}: {
  value: Parameters<typeof valueOf>[0];
  inputs: InputDef[];
  disabled?: boolean;
  hint: string;
  onChange: (author: ReturnType<typeof pickedAgent>) => void;
}) {
  return (
    <ValueRefField label={t("workflow-notify-step-form-speaks")} hint={hint} value={valueOf(value)} inputs={inputs} kind="assignee" disabled={disabled} onChange={(v) => onChange(pickedAgent(v))}>
      {(fixed, setFixed) => (
        <AssigneePicker value={fixed ? [fixed] : []} kinds={["agent"]} max={1} disabled={disabled} onChange={(n) => setFixed(n[0] ?? "")} />
      )}
    </ValueRefField>
  );
}
