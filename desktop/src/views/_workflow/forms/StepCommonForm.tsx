/**
 * What every step has: its id, its name, how flows join, what a failure
 * does, retries and the visit bound — and, for a step whose work can be
 * stopped while it is live, its boundary events (`BoundaryEventsEditor`).
 * The kind's own fields sit below in the kind's form.
 *
 * Renaming rewrites every reference (`workflowGraph.renameStep`), so the id
 * field commits on blur rather than per keystroke — a half-typed id would
 * otherwise rename the step three times on the way to its name.
 */

import { useEffect, useState } from "react";
import type { InputDef, Step } from "../../../types";
import { Field, NumberInput, Select, TextInput } from "../../../ui";
import { DEFAULT_MAX_VISITS, JOINS, ON_FAILS } from "../stepKinds.mjs";
import { failChoice, failTargets } from "../workflowGraph.mjs";
import { BoundaryEventsEditor } from "./BoundaryEventsEditor";
import { t } from "../../../i18n/l10n.mjs";

const ID_RE = /^[a-z][a-z0-9_-]{0,31}$/;

export function StepCommonForm({
  step,
  steps,
  inputs = [],
  onChange,
  onRename,
  disabled,
}: {
  step: Step;
  /** Every step, for the `on_fail: then` target. */
  steps: Step[];
  /** The workflow's inputs, for a boundary event's clock read from one. */
  inputs?: InputDef[];
  onChange: (next: Step) => void;
  onRename: (to: string) => void;
  disabled?: boolean;
}) {
  const [id, setId] = useState(step.id);
  useEffect(() => setId(step.id), [step.id]);
  const idOk = ID_RE.test(id);
  const taken = id !== step.id && steps.some((s) => s.id === id);
  const set = (patch: Partial<Step>) => onChange({ ...step, ...patch } as Step);
  const onFail = step.on_fail?.on_fail ?? "fail";
  // Where a failure may be routed: never the step itself, never a start.
  const targets = failTargets(steps, step.id);

  // The route a failure takes, when the step names one: read once, so the
  // options below read the same value the field shows.
  const routed = step.on_fail?.on_fail === "then" ? step.on_fail : null;
  return (
    <div className="flex flex-col gap-3">
      <div className="grid gap-3 md:grid-cols-2">
        <Field
          label={t("workflow-step-common-form-id")}
          hint={
            !idOk
              ? t("workflow-step-common-form-z-0-9-starts-letter-32")
              : taken
                ? t("workflow-step-common-form-another-step-has-id")
                : t("workflow-step-common-form-named-flows-templates-steps-id-output")
          }
        >
          <TextInput
            className="font-mono"
            value={id}
            disabled={disabled}
            onChange={(e) => setId(e.target.value)}
            onBlur={() => {
              if (idOk && !taken && id !== step.id) onRename(id);
              else setId(step.id);
            }}
          />
        </Field>
        <Field label={t("workflow-inspector-name")} hint={t("workflow-step-common-form-what-canvas-shows")}>
          <TextInput value={step.name} disabled={disabled} onChange={(e) => set({ name: e.target.value })} />
        </Field>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
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
      <div className="grid gap-3 md:grid-cols-2">
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
