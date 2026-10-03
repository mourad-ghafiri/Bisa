/**
 * A step's family, in ink: the classes a kind's glyph wears wherever a step
 * is drawn — the canvas, the palette, a thumbnail, a chip in a goal's strip,
 * a row of a run's progress. The colours are the theme's
 * (`theme/tokens.css`, the step families' inks); the family is the model's
 * (`stepKinds.mjs`). Written out whole so the stylesheet's build finds every
 * class in the source, never assembled from a family's name.
 */

import { familyOf } from "./stepKinds.mjs";
import type { Family } from "./stepKinds.mjs";

const INK: Record<Family, string> = {
  event: "text-step-event",
  gateway: "text-step-gateway",
  loop: "text-step-loop",
  task: "text-step-task",
};

/** The frame an event's ring or a gateway's diamond wears: its family's edge over a wash of the same ink. */
const FRAME: Record<Family, string> = {
  event: "border-step-event/45 bg-step-event/10 text-step-event",
  gateway: "border-step-gateway/45 bg-step-gateway/10 text-step-gateway",
  loop: "border-step-loop/45 bg-step-loop/10 text-step-loop",
  task: "border-step-task/45 bg-step-task/10 text-step-task",
};

/** The glyph's ink for a step of `kind`. */
export function familyInk(kind: string): string {
  return INK[familyOf(kind)] ?? "text-text-dim";
}

/** The ring's or the diamond's classes for a step of `kind`: edge, wash and ink. */
export function familyFrame(kind: string): string {
  return FRAME[familyOf(kind)] ?? "border-border text-text-dim";
}

/** The ink as a colour value, for a glyph drawn inside an SVG (a thumbnail), where a class cannot reach. */
export function familyColor(kind: string): string {
  return `var(--color-step-${familyOf(kind)})`;
}
