/**
 * Motion durations, read from the same tokens the CSS animations use.
 *
 * Enter and exit for overlays is CSS (`theme/motion.css`), because Radix
 * drives it off `data-state` and nothing has to remember to wire it up. What
 * is left for JavaScript is the motion CSS cannot express: an item leaving a
 * list while the items below it close the gap, and an indicator sliding
 * between two tabs that are separate elements. Those need `motion`.
 *
 * The numbers still come from `--motion-*`, resolved once from the document,
 * so "make the app calmer" stays one edit in one file rather than an edit in
 * the stylesheet and a second, easily-forgotten edit in every animated
 * component. The constants below are a fallback for the case where there is
 * no document to read (a test runner), not a second source of truth.
 */

import { useReducedMotion } from "motion/react";

const FALLBACK_MS = { fast: 120, base: 160, slow: 240 } as const;

function readMs(name: string, fallback: number): number {
  if (typeof document === "undefined") return fallback;
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const n = Number.parseFloat(raw);
  if (!Number.isFinite(n)) return fallback;
  return raw.endsWith("ms") || !raw.endsWith("s") ? n : n * 1000;
}

/**
 * Seconds, because that is the unit `motion` takes. Resolved lazily on first
 * use so the stylesheet has certainly been applied, and cached because
 * `getComputedStyle` forces a style recalculation and these are read on every
 * list render.
 */
let cached: { fast: number; base: number; slow: number } | null = null;

export function durations(): { fast: number; base: number; slow: number } {
  if (!cached) {
    cached = {
      fast: readMs("--motion-fast", FALLBACK_MS.fast) / 1000,
      base: readMs("--motion-base", FALLBACK_MS.base) / 1000,
      slow: readMs("--motion-slow", FALLBACK_MS.slow) / 1000,
    };
  }
  return cached;
}

export const EASE = [0.2, 0, 0, 1] as const;

/**
 * A transition for the caller to spread, already reduced to nothing when the
 * reader asked for that.
 *
 * Reduced motion means *no motion*, not slower motion: the duration goes to
 * zero rather than shortening, so an item still appears and disappears but
 * never travels. Radix's own exits are handled by the token collapse in
 * `tokens.css`; this is the same promise for the components that animate in
 * JavaScript.
 */
export function useMotionTiming(speed: "fast" | "base" | "slow" = "base"): {
  duration: number;
  ease: readonly [number, number, number, number];
} {
  const reduced = useReducedMotion();
  return { duration: reduced ? 0 : durations()[speed], ease: EASE };
}

/** The one enter/exit a list row uses, so insertions read the same everywhere. */
export const LIST_ITEM_MOTION = {
  initial: { opacity: 0, y: -4 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -4 },
} as const;
