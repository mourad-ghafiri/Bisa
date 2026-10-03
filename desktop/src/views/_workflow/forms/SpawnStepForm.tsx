/**
 * A spawn step: a sub-goal with a workflow of its own, linked to this goal.
 *
 * The step is a start by hand of the child's workflow, so it asks what a
 * person's start asks: every input that workflow declares, each given as a
 * template of this run. What the child's run needs and the step does not
 * give is a problem the node names (`spawn_input`); the rows here are where
 * it is given (`spawnStepModel.mjs`).
 */

import { api } from "../../../api";
import type { InputDef, Step } from "../../../types";
import { Button, Checkbox, Chip, ErrorNote, Field, Labelled, TextArea, TextInput } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { useAsync } from "../../_work/useAsync";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { WorkflowPicker } from "../WorkflowPicker";
import { fixedWords, inputNames, toggleInput, withFixed } from "./assigneeRefModel.mjs";
import { give, givenRows, leftOut, notAskedFor, onWorkflow } from "./spawnStepModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Spawn = Extract<Step, { kind: "spawn" }>;

export function SpawnStepForm({ step, inputs, onChange, disabled }: { step: Spawn; inputs: InputDef[]; onChange: (next: Step) => void; disabled?: boolean }) {
  const set = (patch: Partial<Spawn>) => onChange({ ...step, ...patch });
  // The `{ input }` references ride along with every change to the fixed
  // ones (`assigneeRefModel.withFixed`).
  const fromInputs = inputNames(step.assignees);
  const assigneeInputs = inputs.filter((i) => i.kind === "assignee");
  // What the child's workflow asks of whoever starts it.
  const workflow = step.workflow ?? null;
  const child = useAsync((s) => (workflow ? api.workflow(workflow, s) : Promise.resolve(null)), [workflow]);
  const asked = child.data?.workflow.inputs ?? [];
  const rows = givenRows(step, asked);
  // Until the child is read nothing is known to be stray, and nothing to be left out.
  const stray = child.data ? notAskedFor(step, asked) : [];
  const missing = new Set(child.data ? leftOut(step, asked) : []);
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-run-workflow-dialog-statement")} hint={t("workflow-spawn-step-form-child-goal-s-own-words", { TEMPLATE_HINT })}>
        <TextArea rows={3} value={step.statement_template} disabled={disabled} onChange={(e) => set({ statement_template: e.target.value })} />
      </Field>
      <Field label={t("workflow-spawn-step-form-workflow")} hint={t("workflow-spawn-step-form-blank-hands-child-workflow-agent-parent")}>
        <WorkflowPicker
          value={workflow}
          disabled={disabled}
          allowNone
          noneLabel={t("workflow-spawn-step-form-guided-workflow-agent-proposes")}
          onChange={(id, picked) => onChange(onWorkflow(step, id, picked?.inputs ?? null))}
        />
      </Field>
      {workflow && (
        <Labelled label={t("workflow-spawn-step-form-what-child-is-given")} hint={t("workflow-spawn-step-form-what-child-is-given-hint", { TEMPLATE_HINT })}>
          <div className="flex flex-col gap-1.5">
            {child.error && <ErrorNote error={t("workflow-spawn-step-form-could-not-read-workflow", { why: child.error })} retry={child.reload} />}
            {child.data && rows.length === 0 && <p className="text-2xs text-text-dim">{t("workflow-inputs-form-workflow-takes-no-inputs")}</p>}
            {rows.map((r) => (
              <div key={r.input} className="grid grid-cols-[minmax(6rem,auto)_1fr] items-center gap-2">
                <span className="flex items-center gap-1 text-2xs">
                  <code className="font-mono">{r.input}</code>
                  {r.required && <Chip tone="warn">{t("workflow-connector-step-form-required")}</Chip>}
                </span>
                <TextInput
                  className="font-mono"
                  value={r.template ?? ""}
                  placeholder={r.required ? t("workflow-spawn-step-form-its-run-needs-this") : t("workflow-start-step-form-left-default")}
                  aria-label={t("workflow-spawn-step-form-gives", { input: r.label })}
                  aria-invalid={missing.has(r.input)}
                  disabled={disabled}
                  onChange={(e) => onChange(give(step, r.input, e.target.value))}
                />
              </div>
            ))}
            {stray.map((name) => (
              <div key={name} className="flex items-center gap-1.5 text-2xs">
                <code className="font-mono text-danger">{name}</code>
                <span className="text-text-dim">{t("workflow-spawn-step-form-child-does-not-ask")}</span>
                <Button size="sm" variant="ghost" disabled={disabled} onClick={() => onChange(give(step, name, null))}>
                  {t("workflow-connector-step-form-remove")}
                </Button>
              </div>
            ))}
          </div>
        </Labelled>
      )}
      <Labelled label={t("workflow-spawn-step-form-assignees")} hint={t("workflow-spawn-step-form-who-carries-child")}>
        <AssigneePicker value={fixedWords(step.assignees)} disabled={disabled} onChange={(next) => set({ assignees: withFixed(step.assignees, next) })} />
      </Labelled>
      {assigneeInputs.length > 0 && (
        <Labelled label={t("workflow-spawn-step-form-also-from-inputs")} hint={t("workflow-spawn-step-form-assignee-input-run-started-carries-child")}>
          <div className="flex flex-wrap gap-1.5">
            {assigneeInputs.map((i) => {
              const on = fromInputs.includes(i.name);
              return (
                <button
                  key={i.name}
                  type="button"
                  aria-pressed={on}
                  disabled={disabled}
                  // The same pressed pill as Notify's: a chosen input is selected, not a summons.
                  className={`anim rounded-full border px-2 py-0.5 text-2xs disabled:opacity-45 ${on ? "border-text/35 bg-selected text-text" : "border-border text-text-dim hover:bg-surface-2 hover:text-text"}`}
                  onClick={() => set({ assignees: toggleInput(step.assignees, i.name) })}
                >
                  {i.name}
                </button>
              );
            })}
          </div>
        </Labelled>
      )}
      <Checkbox label={t("workflow-spawn-step-form-wait-child-s-run-finish")} hint={t("workflow-spawn-step-form-off-step-done-moment-child-exists")} checked={step.wait !== false} disabled={disabled} onChange={(wait) => set({ wait })} />
    </div>
  );
}
