/**
 * A notify step: a message into a conversation, as an agent — the one it
 * names, or the Workflow Agent — mentioning whom it should wake.
 */

import type { InputDef, Step } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { Field, Select, TextArea } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { ValueRefField } from "./AgentStepForm";
import { fixedWords, inputNames, pickedAgent, toggleInput, valueOf, withFixed } from "./assigneeRefModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Notify = Extract<Step, { kind: "notify" }>;

export function NotifyStepForm({ step, inputs, onChange, disabled }: { step: Notify; inputs: InputDef[]; onChange: (next: Step) => void; disabled?: boolean }) {
  const ws = useWorkspace();
  const set = (patch: Partial<Notify>) => onChange({ ...step, ...patch });
  const fromInputs = inputNames(step.mentions);
  const assigneeInputs = inputs.filter((i) => i.kind === "assignee");
  const textInputs = inputs.filter((i) => i.kind === "text" || i.kind === "choice");
  // A scope the list does not carry — a goal or workstream id, an input the
  // workflow no longer declares — is shown as it is rather than as the first
  // option, which would be a lie the save then makes true.
  const scope = step.scope ?? "";
  const scopeListed =
    scope === "" || ws.channels.some((c) => c.channel.id === scope) || textInputs.some((i) => `{inputs.${i.name}}` === scope);
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-notify-step-form-where")} hint={t("workflow-notify-step-form-blank-posts-into-goal-s-own")}>
        <Select value={scope} disabled={disabled} onChange={(e) => set({ scope: e.target.value || null })}>
          <option value="">{t("workflow-notify-step-form-goal-s-conversation")}</option>
          {ws.channels.map((c) => (
            <option key={c.channel.id} value={c.channel.id}>{t("workflow-notify-step-form-channel", { channel: c.channel.name })}</option>
          ))}
          {textInputs.length > 0 && (
            <optgroup label={t("workflow-notify-step-form-read-from-input-start")}>
              {textInputs.map((i) => (
                <option key={i.name} value={`{inputs.${i.name}}`}>{t("workflow-connector-step-form-input", { i: i.name })}</option>
              ))}
            </optgroup>
          )}
          {scopeListed || <option value={scope}>{scope}</option>}
        </Select>
      </Field>
      <ValueRefField
        label={t("workflow-notify-step-form-speaks")}
        hint={t("workflow-notify-step-form-agent-whose-message-left-blank-workflow")}
        value={valueOf(step.author)}
        inputs={inputs}
        kind="assignee"
        disabled={disabled}
        onChange={(v) => set({ author: pickedAgent(v) })}
      >
        {(fixed, setFixed) => (
          <AssigneePicker value={fixed ? [fixed] : []} kinds={["agent"]} max={1} disabled={disabled} onChange={(n) => setFixed(n[0] ?? "")} />
        )}
      </ValueRefField>
      <Field label={t("workflow-notify-step-form-message")} hint={t("workflow-notify-step-form-mentions-what-wake-agent-prose-reaches", { TEMPLATE_HINT })}>
        <TextArea rows={3} value={step.template} disabled={disabled} onChange={(e) => set({ template: e.target.value })} />
      </Field>
      <Field label={t("workflow-notify-step-form-mention")} hint={t("workflow-notify-step-form-who-woken-none-wakes-nobody-notice")}>
        <AssigneePicker value={fixedWords(step.mentions)} disabled={disabled} onChange={(next) => set({ mentions: withFixed(step.mentions, next) })} />
      </Field>
      {assigneeInputs.length > 0 && (
        <Field label={t("workflow-notify-step-form-also-mention-from-inputs")} hint={t("workflow-notify-step-form-assignee-input-run-started")}>
          <div className="flex flex-wrap gap-1.5">
            {assigneeInputs.map((i) => {
              const on = fromInputs.includes(i.name);
              return (
                <button
                  key={i.name}
                  type="button"
                  aria-pressed={on}
                  disabled={disabled}
                  onClick={() => set({ mentions: toggleInput(step.mentions, i.name) })}
                  // A chosen input is a pressed option, not a summons: the neutral selected ground.
                  className={`anim rounded-full border px-2 py-0.5 text-2xs disabled:opacity-45 ${on ? "border-text/35 bg-selected text-text" : "border-border text-text-dim hover:bg-surface-2 hover:text-text"}`}
                >
                  {i.name}
                </button>
              );
            })}
          </div>
        </Field>
      )}
    </div>
  );
}
