/**
 * A cadence's fields — every so many seconds, or at each occurrence of a
 * cron expression in a time zone, exactly one of the two — shared by a
 * start on a schedule, an outside platform's poll and a check start. The
 * seconds and the cron are fixed, or read from an input of the right kind
 * (a number, a text) the host listens with, so one template serves many.
 */

import type { InputDef, Schedule } from "../../../types";
import { Field, NumberInput, Select, TextInput } from "../../../ui";
import { DEFAULT_CRON, DEFAULT_EVERY_SECS, cadenceOf, setCadence } from "./startForm.mjs";
import { RefSource, isInputRef } from "./RefSource";
import { t } from "../../../i18n/l10n.mjs";

/** A cadence as the wire states it: the schedule's own three fields. */
export type Cadence = Pick<Schedule, "every" | "cron" | "tz">;

export function ScheduleFields<S extends Cadence>({
  value,
  inputs,
  onChange,
  disabled,
}: {
  value: S;
  inputs: readonly InputDef[];
  onChange: (next: S) => void;
  disabled?: boolean;
}) {
  const cadence = cadenceOf(value);
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-schedule-fields-cadence")}>
        <Select value={cadence} disabled={disabled} onChange={(e) => onChange(setCadence(value, e.target.value as "every" | "cron"))}>
          <option value="every">{t("workflow-schedule-fields-every-seconds")}</option>
          <option value="cron">{t("workflow-schedule-fields-on-cron")}</option>
        </Select>
      </Field>
      {cadence === "every" ? (
        <Field label={t("workflow-wait-step-form-seconds")} hint={t("workflow-schedule-fields-seconds-hint")}>
          <div className="flex flex-col gap-1.5">
            <RefSource value={value.every} inputs={inputs} kind="number" disabled={disabled} onChange={(r) => onChange({ ...value, every: r ?? DEFAULT_EVERY_SECS })} />
            {!isInputRef(value.every) && (
              <NumberInput className="w-32" value={typeof value.every === "number" ? value.every : DEFAULT_EVERY_SECS} min={1} disabled={disabled} onCommit={(every) => onChange({ ...value, every })} aria-label={t("workflow-wait-step-form-seconds")} />
            )}
          </div>
        </Field>
      ) : (
        <div className="grid gap-3 @xs:grid-cols-2">
          <Field label={t("workflow-wait-step-form-cron")} hint={t("workflow-wait-step-form-five-fields-0-9-1-5")}>
            <div className="flex flex-col gap-1.5">
              <RefSource value={value.cron} inputs={inputs} kind="text" disabled={disabled} onChange={(r) => onChange({ ...value, cron: r ?? DEFAULT_CRON })} />
              {!isInputRef(value.cron) && (
                <TextInput className="font-mono" value={typeof value.cron === "string" ? value.cron : ""} disabled={disabled} onChange={(e) => onChange({ ...value, cron: e.target.value })} />
              )}
            </div>
          </Field>
          <Field label={t("workflow-wait-step-form-timezone")} hint={t("workflow-wait-step-form-blank-utc-named-zone-follows-daylight")}>
            <TextInput className="font-mono" value={value.tz ?? ""} placeholder={t("workflow-wait-step-form-utc")} disabled={disabled} onChange={(e) => onChange({ ...value, tz: e.target.value.trim() || null })} />
          </Field>
        </div>
      )}
    </div>
  );
}
