/**
 * What every step has, in two parts the inspector draws apart. `identity` —
 * its id and its name — sits at the top, above the kind's own form; `flow` —
 * how flows join, what a failure does, retries and the visit bound and, for
 * a step whose work can be stopped while it is live, its boundary events
 * (`BoundaryEventsEditor`) — sits under it, folded into *Flow and failure*:
 * the plumbing is read less often than what the step does.
 *
 * Renaming rewrites every reference (`workflowGraph.renameStep`), so the id
 * field commits on blur rather than per keystroke — a half-typed id would
 * otherwise rename the step three times on the way to its name. An id that
 * cannot be taken says why under the field, and a blur that puts the old id
 * back says it kept it (`workflowForm.idProblem`).
 */

import { useEffect, useState } from "react";
import type { InputDef, Step } from "../../../types";
import { Field, NumberInput, Select, TextInput } from "../../../ui";
import { DEFAULT_MAX_VISITS, JOINS, ON_FAILS } from "../stepKinds.mjs";
import { idProblem, idProblemWords } from "../workflowForm.mjs";
import { failChoice, failTargets } from "../workflowGraph.mjs";
import { BoundaryEventsEditor } from "./BoundaryEventsEditor";
import { t } from "../../../i18n/l10n.mjs";

export function StepCommonForm({
  step,
  steps,
  inputs = [],
  onChange,
  onRename,
  disabled,
  part,
}: {
  step: Step;
  /** Every step, for the `on_fail: then` target. */
  steps: Step[];
  /** The workflow's inputs, for a boundary event's clock read from one. */
  inputs?: InputDef[];
  onChange: (next: Step) => void;
  onRename: (to: string) => void;
  disabled?: boolean;
  /** Which half: the step's id and name, or how its flows join, fail and repeat. */
  part: "identity" | "flow";
}) {
  if (part === "identity") return <IdentityPart step={step} steps={steps} disabled={disabled} onChange={onChange} onRename={onRename} />;
  return <FlowPart step={step} steps={steps} inputs={inputs} disabled={disabled} onChange={onChange} />;
}

function IdentityPart({ step, steps, onChange, onRename, disabled }: { step: Step; steps: Step[]; onChange: (next: Step) => void; onRename: (to: string) => void; disabled?: boolean }) {
  const [id, setId] = useState(step.id);
  // Why the last blur put the id back, until the next keystroke.
  const [kept, setKept] = useState<string | null>(null);
  useEffect(() => setId(step.id), [step.id]);
  const problem = idProblem(id, step.id, (x) => steps.some((s) => s.id === x));
  const error = kept ?? idProblemWords(problem, "step");
  return (
    <div className="grid gap-3 @sm:grid-cols-2">
      <Field label={t("workflow-step-common-form-id")} hint={t("workflow-step-common-form-named-flows-templates-steps-id-output")} error={error}>
        <TextInput
          className="font-mono"
          value={id}
          disabled={disabled}
          onChange={(e) => {
            setId(e.target.value);
            setKept(null);
          }}
          onBlur={() => {
            if (problem === null) {
              if (id !== step.id) onRename(id);
              return;
            }
            setKept(idProblemWords(problem, "step", step.id));
            setId(step.id);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              e.currentTarget.blur();
            }
          }}
        />
      </Field>
      <Field label={t("workflow-inspector-name")} hint={t("workflow-step-common-form-what-canvas-shows")}>
        <TextInput value={step.name} disabled={disabled} onChange={(e) => onChange({ ...step, name: e.target.value })} />
      </Field>
    </div>
  );
}

function FlowPart({ step, steps, inputs, onChange, disabled }: { step: Step; steps: Step[]; inputs: InputDef[]; onChange: (next: Step) => void; disabled?: boolean }) {
  const set = (patch: Partial<Step>) => onChange({ ...step, ...patch } as Step);
  const onFail = step.on_fail?.on_fail ?? "fail";
  // Where a failure may be routed: never the step itself, never a start.
  const targets = failTargets(steps, step.id);

  // The route a failure takes, when the step names one: read once, so the
  // options below read the same value the field shows.
  const routed = step.on_fail?.on_fail === "then" ? step.on_fail : null;
  return (
    <div className="flex flex-col gap-3">
      <div className="grid gap-3 @sm:grid-cols-2">
        <Field label={t("workflow-step-common-form-join")} hint={t("workflow-step-common-form-how-several-incoming-flows-meet-irrelevant")}>
          <Select value={step.join ?? "all"} disabled={disabled} onChange={(e) => set({ join: e.target.value as Step["join"] })}>
            {JOINS.map((j) => (
              <option key={j.join} value={j.join}>
                {j.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("workflow-step-common-form-failure")} hint={t("workflow-step-common-form-once-retries-spent-route-another-step")}>
          <Select
            value={onFail}
            disabled={disabled}
            onChange={(e) => {
              // A route with nowhere to go is refused: the step keeps what it has.
              const on_fail = failChoice(steps, step, e.target.value);
              if (on_fail) set({ on_fail });
            }}
          >
            {ON_FAILS.map((o) => (
              <option key={o.on_fail} value={o.on_fail} disabled={o.on_fail === "then" && targets.length === 0}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {routed && (
        <Field label={t("workflow-step-common-form-route-failures")} hint={t("workflow-step-common-form-remediation-step-only-edge-taken-failure")}>
          <Select
            value={routed.step}
            disabled={disabled}
            onChange={(e) => set({ on_fail: { on_fail: "then", step: e.target.value } })}
          >
            {/* A route the validator refuses — into a start, a step that is gone — is shown as it is, to be moved off. */}
            {targets.some((s) => s.id === routed.step) || <option value={routed.step}>{routed.step}</option>}
            {targets.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name || s.id}
              </option>
            ))}
          </Select>
        </Field>
      )}
      <div className="grid gap-3 @sm:grid-cols-2">
        <Field label={t("workflow-step-common-form-retries")} hint={t("workflow-step-common-form-failed-attempts-re-run-before-failure")}>
          <NumberInput className="w-24" value={step.retries ?? 0} min={0} max={255} disabled={disabled} onCommit={(retries) => set({ retries })} aria-label={t("workflow-step-common-form-retries")} />
        </Field>
        <Field label={t("workflow-step-common-form-max-visits")} hint={t("workflow-step-common-form-how-many-times-loop-may-enter", { DEFAULT_MAX_VISITS })}>
          <NumberInput className="w-24" value={step.max_visits ?? DEFAULT_MAX_VISITS} min={1} max={255} disabled={disabled} onCommit={(max_visits) => set({ max_visits })} aria-label={t("workflow-step-common-form-max-visits-2")} />
        </Field>
      </div>
      <BoundaryEventsEditor step={step} inputs={inputs} disabled={disabled} onChange={onChange} />
    </div>
  );
}
