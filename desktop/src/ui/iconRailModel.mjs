/**
 * A rail of icon tabs, as facts: the vertical strip at a panel's edge where
 * every occupant is a glyph — the Project IDE's right panel (`OccupantRail`)
 * and the Workflow Designer's (`designerPanelModel.mjs`) draw the same strip
 * and follow the same two rules, so the rules live once.
 *
 * - **A press is one gesture with one meaning** — "is this thing showing":
 *   closed → open on it; open elsewhere → switch; open on it → close the
 *   column. The rail itself stays.
 * - **The keys move focus, not selection**: ↑↓ walk the tabs, Home and End
 *   jump, nothing wraps; the rail keeps one tab stop, the showing tab or the
 *   first, so a closed column never makes the rail unreachable.
 */

/**
 * A rail tab pressed.
 * @template {string} T
 * @param {{open: boolean, tab: T}} state
 * @param {T} target
 * @returns {{open: boolean, tab: T}}
 */
export function pressRailTab({ open, tab }, target) {
  if (open && tab === target) return { open: false, tab };
  return { open: true, tab: target };
}

/**
 * Where focus goes for a key on the rail: an index into the tabs, or `null`
 * for a key the rail does not take. Off the rail (`at` −1), ↓ lands on the
 * first tab and ↑ on the last.
 * @param {string} key
 * @param {number} at
 * @param {number} count
 * @returns {number | null}
 */
export function nextRailIndex(key, at, count) {
  if (count <= 0) return null;
  const last = count - 1;
  switch (key) {
    case "Home":
      return 0;
    case "End":
      return last;
    case "ArrowDown":
      return at === -1 ? 0 : Math.min(last, at + 1);
    case "ArrowUp":
      return at === -1 ? last : Math.max(0, at - 1);
    default:
      return null;
  }
}

/**
 * The rail's one tab stop: the tab showing while the column is open, else
 * the first.
 * @template {string} T
 * @param {readonly T[]} tabs
 * @param {{open: boolean, tab: T}} state
 * @returns {T | null}
 */
export function railAnchor(tabs, { open, tab }) {
  if (open && tabs.includes(tab)) return tab;
  return tabs[0] ?? null;
}
