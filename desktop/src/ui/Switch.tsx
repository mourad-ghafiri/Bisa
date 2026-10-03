/**
 * A setting that takes effect immediately.
 *
 * That is the whole difference from {@link Checkbox}: a switch is a state you
 * are changing now, a checkbox is a value you are entering and will submit.
 * Using a switch for something that only applies when the dialog is saved
 * tells the reader the change already happened.
 */

import * as S from "@radix-ui/react-switch";
import { useId } from "react";
import { cn } from "./cn";

export function Switch({
  checked,
  onChange,
  label,
  hint,
  hideLabel = false,
  disabled,
  className,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** The switch's name — always given, even when the row beside it already shows it. */
  label: string;
  hint?: string;
  /**
   * Keep the name for assistive technology but draw none: for a switch at
   * the end of a row whose own words already say what it turns on.
   */
  hideLabel?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  return (
    <div className={cn("flex items-start gap-2.5", className)}>
      <S.Root
        id={id}
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled}
        aria-describedby={hintId}
        className={cn(
          "anim mt-0.5 h-4 w-7 shrink-0 rounded-full border border-border outline-none",
          "data-[state=unchecked]:bg-surface-2 data-[state=unchecked]:hover:border-text-dim/40",
          "data-[state=checked]:border-accent data-[state=checked]:bg-accent",
          "disabled:pointer-events-none disabled:opacity-45",
        )}
      >
        <S.Thumb className="anim block h-3 w-3 rounded-full bg-surface shadow-sm data-[state=checked]:translate-x-3.5 data-[state=unchecked]:translate-x-0.5" />
      </S.Root>
      <div className={cn("text-xs", hideLabel && "sr-only")}>
        <label htmlFor={id}>{label}</label>
        {hint && (
          <span id={hintId} className="mt-0.5 block text-2xs text-text-dim">
            {hint}
          </span>
        )}
      </div>
    </div>
  );
}
