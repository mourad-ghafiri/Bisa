/**
 * A notify step: a message into a conversation, as an agent — the one it
 * names, or the Workflow Agent — mentioning whom it should wake.
 */

import type { InputDef, Step } from "../../../types";
import { Field, TextArea } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { fixedWords, inputNames, toggleInput, withFixed } from "./assigneeRefModel.mjs";
import { NotifyAuthorField, NotifyScopeSelect } from "./NotifyWhereFields";
import { t } from "../../../i18n/l10n.mjs";

type Notify = Extract<Step, { kind: "notify" }>;

export function NotifyStepForm({ step, inputs, onChange, disabled }: { step: Notify; inputs: InputDef[]; onChange: (next: Step) => void; disabled?: boolean }) {
  const set = (patch: Partial<Notify>) => onChange({ ...step, ...patch });
  const fromInputs = inputNames(step.mentions);
  const assigneeInputs = inputs.filter((i) => i.kind === "assignee");
  return (
    <div className="flex flex-col gap-3">
      <NotifyScopeSelect value={step.scope} inputs={inputs} disabled={disabled} hint={t("workflow-notify-step-form-blank-posts-into-goal-s-own")} onChange={(scope) => set({ scope })} />
      <NotifyAuthorField value={step.author} inputs={inputs} disabled={disabled} hint={t("workflow-notify-step-form-agent-whose-message-left-blank-workflow")} onChange={(author) => set({ author })} />
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
