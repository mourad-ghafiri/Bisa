/**
 * How a step chip is painted, from the step's state: a border-and-text tone
 * and a soft fill, both theme roles — never a colour. `runView.mjs` says
 * which role a state has; this says how a chip wears it. Checked in the
 * test against the roles `theme/tokens.css` declares.
 */

/** Border and text per role. */
export const TONE_CLASS = Object.freeze({
  "text-dim": "text-text-dim border-border",
  accent: "text-accent-ink border-accent",
  warn: "text-warn border-warn",
  ok: "text-ok border-ok",
  danger: "text-danger border-danger",
});

/** The soft fill per role — what makes a chip read as a filled state, not an outline. */
export const TONE_FILL = Object.freeze({
  "text-dim": "bg-surface-2",
  accent: "bg-accent-soft",
  warn: "bg-warn-soft",
  ok: "bg-ok-soft",
  danger: "bg-danger-soft",
});

/** Chip geometry per size: the disc, the glyph, the gap between chips. */
export const CHIP_SIZE = Object.freeze({
  compact: { chip: "size-4", current: "size-4", glyph: 9, gap: "gap-0.5" },
  md: { chip: "size-7", current: "size-8", glyph: 14, gap: "gap-1.5" },
  lg: { chip: "size-9", current: "size-10", glyph: 16, gap: "gap-2" },
});

/** The classes a chip in `role` wears. An unknown role paints as dim. */
export function chipClasses(role) {
  const r = role in TONE_CLASS ? role : "text-dim";
  return `${TONE_CLASS[r]} ${TONE_FILL[r]}`;
}
