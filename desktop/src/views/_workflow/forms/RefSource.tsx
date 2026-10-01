/**
 * Where a value reference comes from: *A fixed value*, or *From input …* —
 * the inputs of the kind the field reads, so one template serves many
 * runs. Drawn only when the workflow declares such an input; the fixed
 * control beside it is the caller's. Shared by a wait's clock, a
 * schedule's cadence and a boundary event's seconds.
 */

import type { InputDef } from "../../../types";
import { Select } from "../../../ui";
import { t } from "../../../i18n/l10n.mjs";

export type Ref<T> = { input: string } | T;

/** Whether a reference reads an input. */
export const isInputRef = (v: unknown): v is { input: string } => !!v && typeof v === "object" && "input" in v;

export function RefSource<T>({
  value,
  inputs,
  kind,
  disabled,
  onChange,
}: {
  value: Ref<T> | null | undefined;
  inputs: readonly InputDef[];
  kind: InputDef["kind"];
  disabled?: boolean;
  onChange: (next: { input: string } | null) => void;
}) {
  const candidates = inputs.filter((i) => i.kind === kind);
  const asInput = isInputRef(value) ? value.input : null;
  if (candidates.length === 0) return null;
  return (
    <Select value={asInput ?? ""} disabled={disabled} aria-label={t("workflow-wait-step-form-value-source")} onChange={(e) => onChange(e.target.value ? { input: e.target.value } : null)}>
      <option value="">{t("workflow-agent-step-form-fixed-value")}</option>
      {candidates.map((i) => (
        <option key={i.name} value={i.name}>{t("workflow-agent-step-form-from-input", { i: i.name })}</option>
      ))}
    </Select>
  );
}
