/**
 * The effort control (06 §Effort): how hard a model works, picked on an
 * agent's plan, on one model of it, and on a workflow's agent step — the
 * same control in all three, offering only what the model takes on the
 * harness that will run it.
 *
 * A native select, as the strategy above it is: the keyboard reaches it, and
 * a screen reader reads its options. What it offers, the value it keeps when
 * a model stops offering it, and every word are `effortModel.mjs`'s; this
 * file draws them. With nothing to pick it draws nothing — the sentence
 * beside it (`effortWords`) says why.
 */

import { useResolvedSettings } from "../../shell/useResolvedSettings";
import type { EffortChoice, SettingOrigin } from "../../types";
import { Select } from "../../ui";
import { effortChoice, effortSetting, offersEffort } from "./effortModel.mjs";
import type { EffortOption } from "./effortModel.mjs";

/**
 * The `agents.effort` setting — the last link of the chain — as it resolves
 * for the workspace or for one project, with the layer that holds it.
 */
export function useEffortSetting(project: string | null): { setting: EffortChoice; origin: SettingOrigin } {
  const { resolved } = useResolvedSettings(project);
  return effortSetting(resolved);
}

export function EffortPicker({
  value,
  options,
  onChange,
  caption,
  label,
  disabled,
  className,
}: {
  /** What is asked for; nothing inherits. */
  value: EffortChoice | null | undefined;
  /** What to offer — `effortOptions`, asked with the saved value. */
  options: readonly EffortOption[];
  onChange: (next: EffortChoice | null) => void;
  /** A small word before the control, in a row that names its controls. */
  caption?: string;
  /** The control's name, where no visible label wraps it. */
  label?: string;
  disabled?: boolean;
  className?: string;
}) {
  if (!offersEffort(options)) return null;
  const select = (
    <Select className={className} value={value ?? ""} disabled={disabled} aria-label={label} onChange={(e) => onChange(effortChoice(e.target.value))}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </Select>
  );
  if (!caption) return select;
  return (
    <label className="flex shrink-0 items-center gap-1 text-2xs text-text-dim">
      {caption}
      {select}
    </label>
  );
}
