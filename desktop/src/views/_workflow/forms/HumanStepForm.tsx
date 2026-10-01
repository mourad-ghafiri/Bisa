/**
 * A human step: the prompt, the options offered (or none — free text is
 * always allowed), whether several may be picked, and who is asked.
 */

import type { AskOption, InputDef, Step } from "../../../types";
import { Button, Checkbox, Field, ICON, TextArea, TextInput } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { TEMPLATE_HINT, freshOptionId } from "../stepKinds.mjs";
import { ValueRefField } from "./AgentStepForm";
import { picked, valueOf } from "./assigneeRefModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Human = Extract<Step, { kind: "human" }>;

export function HumanStepForm({
  step,
  inputs,
  onChange,
  disabled,
}: {
  step: Human;
  inputs: InputDef[];
  onChange: (next: Step) => void;
  disabled?: boolean;
}) {
  const set = (patch: Partial<Human>) => onChange({ ...step, ...patch });
  const options = step.options ?? [];
  const patch = (i: number, o: Partial<AskOption>) => set({ options: options.map((x, j) => (j === i ? { ...x, ...o } : x)) });
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-approval-step-form-prompt")} hint={t("workflow-human-step-form-one-question-answerable-sentence", { TEMPLATE_HINT })}>
        <TextArea rows={3} value={step.prompt} disabled={disabled} onChange={(e) => set({ prompt: e.target.value })} />
      </Field>
      <Field label={t("workflow-human-step-form-options")} hint={t("workflow-human-step-form-leave-empty-free-text-question-ids")}>
        <div className="flex flex-col gap-1.5">
          {options.map((o, i) => (
            <div key={i} className="grid gap-1.5 md:grid-cols-[1fr_2fr_auto]">
              <TextInput className="font-mono" value={o.id} placeholder={t("workflow-human-step-form-id")} aria-label={t("workflow-condition-editor-option-id")} disabled={disabled} onChange={(e) => patch(i, { id: e.target.value })} />
              <TextInput value={o.label} placeholder={t("workflow-human-step-form-label")} aria-label={t("workflow-human-step-form-option-label")} disabled={disabled} onChange={(e) => patch(i, { label: e.target.value })} />
              <div className="flex items-center gap-1">
                <Button size="sm" variant={o.recommended ? "primary" : "ghost"} disabled={disabled} onClick={() => set({ options: options.map((x, j) => ({ ...x, recommended: j === i ? !x.recommended : false })) })}>
                  <ICON.recommended size={12} aria-hidden />
                </Button>
                <Button size="sm" variant="ghost" disabled={disabled} onClick={() => set({ options: options.filter((_, j) => j !== i) })}>
                  <ICON.delete size={12} aria-hidden />
                </Button>
              </div>
            </div>
          ))}
          <div>
            <Button size="sm" disabled={disabled} onClick={() => set({ options: [...options, { id: freshOptionId(options), label: "", recommended: false }] })}>
              <ICON.add size={12} aria-hidden />{t("workflow-human-step-form-add-option")}</Button>
          </div>
        </div>
      </Field>
      {options.length > 0 && (
        <Checkbox label={t("workflow-human-step-form-several-may-picked")} checked={step.multi === true} disabled={disabled} onChange={(multi) => set({ multi })} />
      )}
      <ValueRefField
        label={t("workflow-human-step-form-ask-whom")}
        hint={t("workflow-human-step-form-blank-asks-goal-s-own-people")}
        value={valueOf(step.assignee)}
        inputs={inputs}
        kind="assignee"
        disabled={disabled}
        onChange={(v) => set({ assignee: picked(v) })}
      >
        {(fixed, setFixed) => (
          <AssigneePicker value={fixed ? [fixed] : []} max={1} disabled={disabled} onChange={(n) => setFixed(n[0] ?? "")} />
        )}
      </ValueRefField>
    </div>
  );
}
