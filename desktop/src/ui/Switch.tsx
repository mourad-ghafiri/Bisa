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
  disabled,
  className,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
  className?: string;
}) {
  const id = useId();
  return (
    <div className={cn("flex items-start gap-2.5", className)}>
      <S.Root
        id={id}
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled}
        className={cn(
          "anim mt-0.5 h-4 w-7 shrink-0 rounded-full border border-border outline-none",
          "data-[state=unchecked]:bg-surface-2",
          "data-[state=checked]:border-accent data-[state=checked]:bg-accent",
          "disabled:pointer-events-none disabled:opacity-45",
        )}
      >
        <S.Thumb className="anim block h-3 w-3 rounded-full bg-surface shadow-sm data-[state=checked]:translate-x-3.5 data-[state=unchecked]:translate-x-0.5" />
      </S.Root>
      <label htmlFor={id} className="text-xs">
        {label}
        {hint && <span className="mt-0.5 block text-2xs text-text-dim">{hint}</span>}
      </label>
    </div>
  );
}
