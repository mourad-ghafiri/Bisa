/**
 * An agent step: instructions a fresh session can act on, who runs it, where,
 * on which harness and model and at what effort, with what result schema and
 * tool tier.
 *
 * `assignee` and `project` are value references: fixed, or an input of the
 * matching kind — a template cannot know a workspace's people or a project's
 * ULID, so a shape that varies per goal names an input instead.
 *
 * `model` and `effort` are pins. The effort's picker offers what the step's
 * harnesses take for its model (`agentStepModel.stepHarnesses`), and every
 * level when nobody can say which harness will run it; a level a model
 * cannot take is fitted at launch, never refused.
 *
 * The harness field is a draft while it is typed and read on blur: a field
 * re-written from the parsed list on every keystroke cannot be typed a comma
 * into, and so could never hold a second harness (`JsonField` says the same
 * of its braces).
 */

import { useEffect, useState } from "react";
import { api } from "../../../api";
import { errorFields, log } from "../../../log";
import type { InputDef, Step } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { Field, Labelled, Select, TextArea, TextInput } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { EffortPicker } from "../../_work/EffortPicker";
import { effortOptions, withEffort } from "../../_work/effortModel.mjs";
import { useAsync } from "../../_work/useAsync";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { answersFor, harnessText, harnessesFrom, stepEffortHint, stepEfforts, stepHarnesses } from "./agentStepModel.mjs";
import { JsonField } from "./JsonField";
import { picked, valueOf } from "./assigneeRefModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Agent = Extract<Step, { kind: "agent" }>;

/** A value reference as the form edits it: `input:<name>` or the fixed value. */
export function ValueRefField({
  label,
  hint,
  value,
  inputs,
  kind,
  onChange,
  disabled,
  children,
}: {
  label: string;
  hint?: string;
  value: { input: string } | string | null | undefined;
  inputs: InputDef[];
  kind: InputDef["kind"];
  onChange: (next: { input: string } | string | null) => void;
  disabled?: boolean;
  /** The fixed-value control, rendered when no input is chosen. */
  children: (fixed: string, setFixed: (v: string) => void) => React.ReactNode;
}) {
  const candidates = inputs.filter((i) => i.kind === kind);
  const asInput = value && typeof value === "object" ? value.input : null;
  // A select and a picker are two controls: one caption over them, not one `<label>` round both.
  return (
    <Labelled label={label} hint={hint}>
      <div className="flex flex-col gap-1.5">
        {candidates.length > 0 && (
          <Select
            value={asInput ?? ""}
            aria-label={label}
            disabled={disabled}
            onChange={(e) => onChange(e.target.value ? { input: e.target.value } : null)}
          >
            <option value="">{t("workflow-agent-step-form-fixed-value")}</option>
            {candidates.map((i) => (
              <option key={i.name} value={i.name}>{t("workflow-agent-step-form-from-input", { i: i.name })}</option>
            ))}
          </Select>
        )}
        {!asInput && children(typeof value === "string" ? value : "", (v) => onChange(v || null))}
      </div>
    </Labelled>
  );
}

export function AgentStepForm({
  step,
  inputs,
  onChange,
  disabled,
}: {
  step: Agent;
  inputs: InputDef[];
  onChange: (next: Step) => void;
  disabled?: boolean;
}) {
  const ws = useWorkspace();
  const set = (patch: Partial<Agent>) => onChange({ ...step, ...patch });

  // What each harness the step may run on takes. One that does not answer is
  // unknown, and so is a step nobody can name a harness for.
  const harnesses = stepHarnesses(step, ws.agents);
  const asked = harnesses.join(",");
  const read = useAsync(
    async (s) => ({
      asked,
      answers: await Promise.all(
        harnesses.map((h) =>
          api.models(h, s).catch((e: unknown) => {
            // Unknown, not none: the picker offers every level, and the log says which harness did not answer.
            log.debug("workflow", "a harness did not say what it takes", { harness: h, ...errorFields(e) });
            return null;
          }),
        ),
      ),
    }),
    [asked],
  );
  const { available: taken, known } = stepEfforts(answersFor(asked, read.data), step.model);

  // The harness field as typed, read into the step when the field is left.
  const stored = harnessText(step);
  const [harnessDraft, setHarnessDraft] = useState(stored);
  useEffect(() => setHarnessDraft(stored), [stored]);
  const commitHarness = () => {
    const next = harnessesFrom(harnessDraft);
    setHarnessDraft(next.join(", "));
    if (next.join(",") !== (step.harness ?? []).join(",")) set({ harness: next });
  };

  return (
    <div className="flex flex-col gap-3">
      <Field
        label={t("workflow-agent-step-form-instructions")}
        hint={t("workflow-agent-step-form-actionable-fresh-session-no-memory-conversation", { TEMPLATE_HINT })}
      >
        <TextArea rows={5} value={step.instructions} disabled={disabled} onChange={(e) => set({ instructions: e.target.value })} />
      </Field>
      <ValueRefField
        label={t("workflow-agent-step-form-assignee")}
        hint={t("workflow-agent-step-form-who-runs-blank-falls-back-goal")}
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
      <ValueRefField
        label={t("workflow-agent-step-form-project")}
        hint={t("workflow-agent-step-form-where-work-happens-must-attached-goal")}
        value={step.project ?? null}
        inputs={inputs}
        kind="project"
        disabled={disabled}
        onChange={(v) => set({ project: v })}
      >
        {(fixed, setFixed) => (
          <Select value={fixed} disabled={disabled} onChange={(e) => setFixed(e.target.value)}>
            <option value="">{t("workflow-agent-step-form-goal-s-own-folder")}</option>
            {ws.projects.map((p) => (
              <option key={p.project.id} value={p.project.id}>
                {p.project.name}
              </option>
            ))}
          </Select>
        )}
      </ValueRefField>
      <div className="grid gap-3 @xs:grid-cols-2">
        <Field label={t("workflow-agent-step-form-harness")} hint={t("workflow-agent-step-form-comma-separated-fallback-order-blank-means")}>
          <TextInput
            className="font-mono"
            value={harnessDraft}
            disabled={disabled}
            onChange={(e) => setHarnessDraft(e.target.value)}
            onBlur={commitHarness}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commitHarness();
              }
            }}
          />
        </Field>
        <Field label={t("workflow-agent-step-form-model")} hint={t("workflow-agent-step-form-pin-never-substituted-unavailable-pin-fails")}>
          <TextInput
            className="font-mono"
            value={step.model ?? ""}
            disabled={disabled}
            onChange={(e) => set({ model: e.target.value.trim() || null })}
          />
        </Field>
        {/* The second pin, in the cell after the model's. */}
        <Field label={t("workflow-agent-step-form-effort")} hint={stepEffortHint(step, { available: taken, known })}>
          <EffortPicker
            value={step.effort}
            options={effortOptions(taken, { inherit: true, auto: true, known, current: step.effort })}
            disabled={disabled}
            onChange={(effort) => onChange(withEffort(step, effort))}
          />
        </Field>
        <Field label={t("workflow-agent-step-form-tool-tier")} hint={t("workflow-agent-step-form-most-session-may-do")}>
          <Select value={step.tier_ceiling ?? "write"} disabled={disabled} onChange={(e) => set({ tier_ceiling: e.target.value as Agent["tier_ceiling"] })}>
            <option value="read">{t("workflow-agent-step-form-read")}</option>
            <option value="write">{t("workflow-agent-step-form-write")}</option>
            <option value="exec">{t("workflow-agent-step-form-exec")}</option>
          </Select>
        </Field>
        <JsonField
          label={t("workflow-agent-step-form-output-schema")}
          hint={t("workflow-agent-step-form-json-schema-result-must-satisfy-shown")}
          value={step.output_schema}
          rows={4}
          disabled={disabled}
          onCommit={(output_schema) => set({ output_schema })}
        />
      </div>
    </div>
  );
}
