/**
 * What a number field commits, decided where `node --test` can reach it.
 *
 * A number is typed as text and committed on blur or Enter, never per
 * keystroke: a field bound per keystroke turns "1" into 1 before you can
 * type "12", and clamps "" to its minimum while you are still deleting. So
 * the draft is a string, and this is the one rule that turns it into a value.
 */

/**
 * The integer `text` means, held to `[min, max]`; `fallback` when it means
 * nothing (blank, not a number). A decimal is truncated, never rounded, so
 * what you see is what you get.
 */
export function parseBounded(text, { min = -Infinity, max = Infinity, fallback = 0 } = {}) {
  const trimmed = String(text ?? "").trim();
  if (trimmed === "") return fallback;
  const n = Number(trimmed);
  if (!Number.isFinite(n)) return fallback;
  return Math.max(min, Math.min(max, Math.trunc(n)));
}
