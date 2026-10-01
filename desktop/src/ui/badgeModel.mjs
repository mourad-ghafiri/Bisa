/**
 * What a count badge reads (`Badge.tsx`). A badge is a glance, not a ledger:
 * past its cap it says *that many and more*, and the exact number is the
 * tooltip's. The cap follows the size — a mark in an icon's corner has room
 * for a digit and a plus; three characters there cover the icon it marks.
 */

/** The most a badge counts to before it says *and more*, by size. */
export const BADGE_MAX = Object.freeze({ md: 99, sm: 9 });

/**
 * The text a badge draws, or null when it draws nothing: a count of zero or
 * less, or one that is not a number. An explicit `max` wins over the size's.
 * @param {number} count
 * @param {"md" | "sm"} [size]
 * @param {number} [max]
 * @returns {string | null}
 */
export function badgeText(count, size = "md", max) {
  if (!Number.isFinite(count) || count <= 0) return null;
  const cap = typeof max === "number" && max > 0 ? max : (BADGE_MAX[size] ?? BADGE_MAX.md);
  const whole = Math.floor(count);
  return whole > cap ? `${cap}+` : String(whole);
}
