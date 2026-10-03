/**
 * A wait step: what the outside world has to do before the run moves — a
 * catch event. A delay, a moment or a schedule the clock drives; a named
 * signal, a message, a project's change, a run's end or a platform event
 * the world raises — the filters the start events use, their fields
 * rendered against the run; or a person releasing it. A delay's seconds
 * and a schedule's cron are value references: fixed, or read from an input
 * of the right kind so one template serves many holds. What was heard is
 * the step's output.
 */

import type { InputDef, Step } from "../../../types";
import { Field, NumberInput, Select, TextInput } from "../../../ui";
import { TEMPLATE_HINT, WAITS, blankWait } from "../stepKinds.mjs";
import { MessageFilterFields } from "./MessageFilterFields";
import { ProjectFilterFields } from "./ProjectFilterFields";
import { RefSource, isInputRef } from "./RefSource";
import { RunFilterFields } from "./RunFilterFields";
import { PlatformFilterFields, SignalFilterFields } from "./SignalFilterFields";
import { t } from "../../../i18n/l10n.mjs";

type Wait = Extract<Step, { kind: "wait" }>;

export function WaitStepForm({
  step,
  inputs,
  onChange,
  disabled,
}: {
  step: Wait;
  inputs: InputDef[];
  onChange: (next: Step) => void;
  disabled?: boolean;
}) {
  const u = step.until;
  const set = (until: Wait["until"]) => onChange({ ...step, until });
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-wait-step-form-wait")}>
        <Select value={u.until} disabled={disabled} onChange={(e) => set(blankWait(e.target.value as Wait["until"]["until"]))}>
          {WAITS.map((w) => (
            <option key={w.until} value={w.until}>
              {w.label}
            </option>
          ))}
        </Select>
      </Field>
      {u.until === "delay" && (
        <Field label={t("workflow-wait-step-form-seconds")} hint={t("workflow-wait-step-form-counted-engine-s-own-clock-survives")}>
          <div className="flex flex-col gap-1.5">
            <RefSource value={u.secs} inputs={inputs} kind="number" disabled={disabled} onChange={(r) => set({ ...u, secs: r ?? 3600 })} />
            {!isInputRef(u.secs) && (
              <NumberInput className="w-32" value={typeof u.secs === "number" ? u.secs : 3600} min={1} disabled={disabled} onCommit={(secs) => set({ ...u, secs })} aria-label={t("workflow-wait-step-form-seconds")} />
            )}
          </div>
        </Field>
      )}
      {u.until === "time" && (
        <Field label={t("workflow-wait-step-form-until")} hint={t("workflow-wait-step-form-moment-hint", { TEMPLATE_HINT })}>
          <TextInput className="font-mono" value={u.at} disabled={disabled} /* for the machine */ placeholder="{inputs.deadline}" onChange={(e) => set({ ...u, at: e.target.value })} />
        </Field>
      )}
      {u.until === "schedule" && (
        <div className="grid gap-3 @xs:grid-cols-2">
          <Field label={t("workflow-wait-step-form-cron")} hint={t("workflow-wait-step-form-five-fields-0-9-1-5")}>
            <div className="flex flex-col gap-1.5">
              <RefSource value={u.cron} inputs={inputs} kind="text" disabled={disabled} onChange={(r) => set({ ...u, cron: r ?? "0 9 * * 1-5" })} />
              {!isInputRef(u.cron) && (
                <TextInput className="font-mono" value={typeof u.cron === "string" ? u.cron : ""} disabled={disabled} onChange={(e) => set({ ...u, cron: e.target.value })} />
              )}
            </div>
          </Field>
          <Field label={t("workflow-wait-step-form-timezone")} hint={t("workflow-wait-step-form-blank-utc-named-zone-follows-daylight")}>
            <TextInput className="font-mono" value={u.tz ?? ""} placeholder={t("workflow-wait-step-form-utc")} disabled={disabled} onChange={(e) => set({ ...u, tz: e.target.value.trim() || null })} />
          </Field>
        </div>
      )}
      {u.until === "signal" && (
        <SignalFilterFields name={u.name} fields={u.fields} disabled={disabled} templateHint={TEMPLATE_HINT} onChange={(next) => set({ ...u, ...next })} />
      )}
      {u.until === "message" && <MessageFilterFields value={u} inputs={inputs} disabled={disabled} templateHint={TEMPLATE_HINT} onChange={set} />}
      {u.until === "project" && <ProjectFilterFields value={u} inputs={inputs} disabled={disabled} onChange={set} />}
      {u.until === "run" && <RunFilterFields value={u} disabled={disabled} onChange={set} />}
      {u.until === "platform" && <PlatformFilterFields topic={u.topic} fields={u.fields} disabled={disabled} onChange={(next) => set({ ...u, ...next })} />}
      {u.until === "release" && <p className="text-2xs text-text-dim">{t("workflow-wait-step-form-run-holds-here-until-person-releases")}</p>}
      {u.until !== "release" && u.until !== "delay" && u.until !== "time" && u.until !== "schedule" && (
        <p className="text-2xs text-text-dim">{t("workflow-wait-step-form-heard-is-output", { step: step.id })}</p>
      )}
    </div>
  );
}
