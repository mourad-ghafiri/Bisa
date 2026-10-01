/**
 * A bounded number you set by dragging, and see the effect of immediately.
 *
 * Built on a native `<input type="range">` rather than the Radix primitive
 * every other control here uses, and that is a deliberate exception: the
 * native element already has the keyboard model (arrows, Home/End, Page
 * Up/Down), the ARIA role and the value announcement, and there is no
 * behaviour on this one worth a dependency. The Radix components earn their
 * place by replacing things the platform does *badly* — a menu's focus
 * trapping, a dialog's inertness. A range input is not one of them.
 *
 * The value sits beside the label rather than under the thumb, because a
 * bubble that tracks the thumb is unreadable exactly when you are dragging,
 * and every use here is of a setting that applies live: the real feedback is
 * the thing on screen changing, not the number.
 */

import { useId } from "react";
import { cn } from "./cn";

export function Slider({
  value,
  onChange,
  min,
  max,
  step = 1,
  label,
  hint,
  /** Rendered beside the label. Defaults to the bare number. */
  format,
  disabled,
  className,
}: {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  label: string;
  hint?: string;
  format?: (value: number) => string;
  disabled?: boolean;
  className?: string;
}) {
  const id = useId();
  return (
    <div className={cn("block", className)}>
      <div className="mb-1 flex items-baseline justify-between gap-2">
        <label htmlFor={id} className="text-2xs font-medium text-text-dim">
          {label}
        </label>
        {/* Tabular figures: without them the row twitches sideways as the
            digits change under a drag. */}
        <span className="text-2xs tabular-nums text-text-dim">
          {format ? format(value) : value}
        </span>
      </div>
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(Number(e.target.value))}
        className={cn(
          "h-4 w-full cursor-pointer appearance-none bg-transparent outline-none",
          "disabled:pointer-events-none disabled:opacity-45",
          // The track and the thumb have no cross-browser styling hook, so
          // each engine's pseudo-element is addressed directly. This webview
          // is WebKit; the Firefox rules are here so the same file renders in
          // a browser tab during development rather than falling back to the
          // platform's own blue.
          "[&::-webkit-slider-runnable-track]:h-1 [&::-webkit-slider-runnable-track]:rounded-full [&::-webkit-slider-runnable-track]:bg-surface-2",
          "[&::-moz-range-track]:h-1 [&::-moz-range-track]:rounded-full [&::-moz-range-track]:bg-surface-2",
          "[&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:-mt-1.5 [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border [&::-webkit-slider-thumb]:border-accent [&::-webkit-slider-thumb]:bg-accent",
          "[&::-moz-range-thumb]:h-4 [&::-moz-range-thumb]:w-4 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border [&::-moz-range-thumb]:border-accent [&::-moz-range-thumb]:bg-accent",
          "focus-visible:[&::-webkit-slider-thumb]:ring-2 focus-visible:[&::-webkit-slider-thumb]:ring-accent/40",
          "focus-visible:[&::-moz-range-thumb]:ring-2 focus-visible:[&::-moz-range-thumb]:ring-accent/40",
        )}
      />
      {hint && (
        <span className="mt-1 block text-2xs text-text-dim">{hint}</span>
      )}
    </div>
  );
}
