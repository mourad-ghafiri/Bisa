/**
 * A document strip's keyboard, as facts (`ui/TabStrip.tsx`).
 *
 * The strip is one tab stop, as a tab list should be: Tab lands on the open
 * tab — or the first, when nothing in the strip is open — and the arrows walk
 * the rest. ← and → move focus one tab, Home and End jump to the ends, and
 * nothing wraps, the way the icon rail's keys behave (`iconRailModel.mjs`).
 * Focus moves; selection does not: Enter or Space opens the focused tab,
 * because a strip of terminals that switched on every arrow would start
 * redrawing a terminal per key.
 */

/**
 * Where focus goes for a key on the strip: an index into the tabs, or `null`
 * for a key the strip does not take.
 * @param {string} key
 * @param {number} at the focused tab's index
 * @param {number} count
 * @returns {number | null}
 */
export function nextStripIndex(key, at, count) {
  if (count <= 0 || at < 0) return null;
  const last = count - 1;
  switch (key) {
    case "Home":
      return 0;
    case "End":
      return last;
    case "ArrowRight":
      return Math.min(last, at + 1);
    case "ArrowLeft":
      return Math.max(0, at - 1);
    default:
      return null;
  }
}

/**
 * The strip's one tab stop: the open tab when it is in the strip, else the
 * first — so a strip whose open document lives elsewhere is still reachable.
 * @param {readonly string[]} ids
 * @param {string | null} active
 * @returns {string | null}
 */
export function stripTabStop(ids, active) {
  if (active !== null && ids.includes(active)) return active;
  return ids[0] ?? null;
}
