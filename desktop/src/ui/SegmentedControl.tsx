/**
 * One choice out of a few, shown all at once.
 *
 * A segmented control earns its width by making the alternatives visible, so
 * it is for two to four short options — a sort order, a density, a filter
 * mode — or, with `iconOnly`, two to four glyphs each of which is a mark a
 * person recognises (an eye for a rendering, brackets for its source), the
 * word kept as the accessible name and the tooltip. Past that it is a
 * `<Select>`: five segments is a row of buttons pretending to be a menu.
 *
 * Built on ToggleGroup in `single` mode rather than on Tabs, because these
 * options are a value, not a place. Tabs move you between panels and belong
 * in the browser's history; this sets a variable.
 *
 * `stretch` is for a row that must fit its panel — the footer's resource
 * overlay, whose six dimensions would otherwise run past the popover's edge:
 * the control takes the panel's width, every segment its word's width plus
 * an equal share of what is left, and a word is never cut — a panel too
 * narrow for the row wraps it onto a second line, so every glyph and every
 * word stays visible.
 */

import * as G from "@radix-ui/react-toggle-group";
import { cn } from "./cn";
import type { LucideIcon } from "./icons";
import { FOCUS_RING } from "./rings";
import { Tooltip } from "./Tooltip";

export interface Segment<T extends string> {
  id: T;
  label: string;
  icon?: LucideIcon;
  /** The tooltip a glyph-only segment wears, when its word alone would not say enough; the word stays its accessible name. */
  hint?: string;
  /** Held: shown, not choosable — the hint says why. */
  disabled?: boolean;
}

export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  label,
  size = "md",
  iconOnly = false,
  stretch = false,
  className,
}: {
  options: ReadonlyArray<Segment<T>>;
  value: T;
  onChange: (value: T) => void;
  /** Names the group for assistive technology; there is no visible label. */
  label: string;
  size?: "sm" | "md";
  /** Glyphs alone: each segment's word becomes its accessible name and its tooltip. A segment with no glyph still shows its word. */
  iconOnly?: boolean;
  /** Take the parent's width, every segment its word plus an equal share of the rest; a row the panel cannot hold wraps, never cuts. */
  stretch?: boolean;
  className?: string;
}) {
  return (
    <G.Root
      type="single"
      value={value}
      aria-label={label}
      // Radix reports "" when the pressed item is toggled off. A segmented
      // control has no empty state — re-picking the current option is a
      // no-op, not a way to choose nothing.
      onValueChange={(next) => next && onChange(next as T)}
      // The ring on the root too: Radix's roving focus makes the group a tab
      // stop that hands focus on to the pressed segment, and the keyboard walk
      // found focus resting on the root itself in Board and Agent Mode with no
      // ring to show for it. The segments keep their own.
      className={cn(
        "items-center gap-0.5 rounded-control border border-hairline bg-surface-2/70 p-0.5",
        FOCUS_RING,
        stretch ? "flex w-full flex-wrap" : "inline-flex",
        className,
      )}
    >
      {options.map((o) => {
        const glyphOnly = iconOnly && o.icon !== undefined;
        const item = (
          <G.Item
            key={o.id}
            value={o.id}
            disabled={o.disabled}
            aria-label={glyphOnly ? o.label : undefined}
            className={cn(
              "anim inline-flex items-center gap-1.5 rounded-control font-medium outline-none",
              size === "sm" ? "h-5 text-2xs" : "h-6 text-xs",
              glyphOnly ? "px-1.5" : size === "sm" ? "px-2" : "px-2.5",
              stretch && "grow shrink-0 justify-center",
              "text-text-dim hover:text-text",
              "data-[state=on]:bg-surface data-[state=on]:text-text data-[state=on]:shadow-sm",
            )}
          >
            {o.icon && <o.icon size={13} aria-hidden className="shrink-0" />}
            {!glyphOnly && (stretch ? <span className="whitespace-nowrap">{o.label}</span> : o.label)}
          </G.Item>
        );
        return glyphOnly ? (
          <Tooltip key={o.id} label={o.hint ?? o.label}>
            {item}
          </Tooltip>
        ) : (
          item
        );
      })}
    </G.Root>
  );
}
