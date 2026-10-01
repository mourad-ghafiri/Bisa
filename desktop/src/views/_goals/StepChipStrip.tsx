/**
 * A run as a strip of step chips: kind glyph, filled and toned by state, the
 * current step ringed and a size larger. Chips are state, not actions —
 * clicking one opens the goal's Workflow tab focused on that step, one
 * gesture with one meaning. Roving tabindex, Left/Right between chips.
 *
 * Three sizes: `md` on the Goals list, `lg` in a goal's header, `compact`
 * where a strip is a detail in a header meta line. How a chip is painted is
 * `stepChipTones.mjs`, checked against the theme's roles.
 */
import { useRef } from "react";
import { Tooltip, cn, stepKindIcon } from "../../ui";
import type { RunStrip } from "../../types";
import { STEP_TONE_TOKENS } from "../_workflow/runView.mjs";
import { chipsOf } from "./goalStripModel.mjs";
import { CHIP_SIZE, chipClasses, type ChipSize } from "./stepChipTones.mjs";
import { t } from "../../i18n/l10n.mjs";

export function StepChipStrip({
  strip,
  compact = false,
  size,
  className,
  onOpenStep,
}: {
  strip: RunStrip;
  /** The header-meta size; shorthand for `size="compact"`. */
  compact?: boolean;
  size?: ChipSize;
  className?: string;
  onOpenStep?: (step: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const chips = chipsOf(strip);
  if (chips.length === 0) return null;
  const geometry = CHIP_SIZE[size ?? (compact ? "compact" : "md")];
  const move = (from: HTMLElement, dir: 1 | -1) => {
    const all = [...(ref.current?.querySelectorAll<HTMLElement>("[data-chip]") ?? [])];
    const next = all[all.indexOf(from) + dir];
    next?.focus();
  };
  return (
    <div
      ref={ref}
      role="group"
      aria-label={strip.workflow_name ? t("goals-step-chip-strip-run", { workflow_name: strip.workflow_name }) : t("goals-step-chip-strip-run-unnamed")}
      className={cn("flex min-w-0 flex-wrap items-center", geometry.gap, className)}
    >
      {chips.map((c, i) => {
        const Icon = stepKindIcon(c.kind);
        const role = STEP_TONE_TOKENS[c.state.state as keyof typeof STEP_TONE_TOKENS] ?? "text-dim";
        return (
          <Tooltip key={c.id} label={c.label}>
            <button
              type="button"
              data-chip
              tabIndex={i === 0 ? 0 : -1}
              aria-label={c.label}
              aria-current={c.current ? "step" : undefined}
              onClick={() => onOpenStep?.(c.id)}
              onKeyDown={(e) => {
                if (e.key === "ArrowRight") move(e.currentTarget, 1);
                if (e.key === "ArrowLeft") move(e.currentTarget, -1);
              }}
              className={cn(
                "anim inline-flex shrink-0 items-center justify-center rounded-full border",
                c.current ? geometry.current : geometry.chip,
                chipClasses(role),
                c.current && "ring-2 ring-accent ring-offset-2 ring-offset-surface",
              )}
            >
              <Icon size={c.current ? geometry.glyph + 1 : geometry.glyph} aria-hidden />
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}
