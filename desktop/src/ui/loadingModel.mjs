/**
 * When a loading indicator earns its place — no React. A read that lands
 * within a beat is instant to a person, and a placeholder drawn for it is
 * only a flash: the skeleton, the *reading …* line and the spinner all wait
 * `INDICATOR_DELAY_MS` before they show, and a read that answers first
 * shows nothing but its answer. The beat belongs to a place that is already
 * drawn: inside a surface that has just opened — a dialog, a popover — the
 * indicator is immediate (`beatMs`), since a blank panel behind the veil
 * would be the flash. No pending piece is exempt — the placeholder block a
 * screen sizes itself waits too (`placeholderFill`), holding its box unseen,
 * because a tinted rectangle drawn for two frames is the flash at its worst.
 * Plain `.mjs`, so `node --test` holds the beat and the rule.
 */

/**
 * The beat under which a read is instant: long enough that a fast answer
 * never flashes a placeholder, short enough that a slow one is not a blank
 * pane for long. Held in the 100–300 ms band by the test.
 */
export const INDICATOR_DELAY_MS = 200;

/**
 * Whether an indicator mounted at `mountedAt` is due at `now`.
 * @param {number} mountedAt milliseconds
 * @param {number} now milliseconds
 * @param {number} [delayMs]
 */
export function indicatorDue(mountedAt, now, delayMs = INDICATOR_DELAY_MS) {
  if (!(delayMs > 0)) return true;
  return now - mountedAt >= delayMs;
}

/**
 * The beat an indicator waits before it shows: the kit's `INDICATOR_DELAY_MS`
 * in a place that is already drawn — a fast read then never flashes a
 * placeholder — and none inside a surface that has just opened (a dialog, a
 * popover), whose blank body would be the flash.
 * @param {boolean} inSurface
 */
export function beatMs(inSurface) {
  return inSurface ? 0 : INDICATOR_DELAY_MS;
}

/**
 * What a placeholder block wears: nothing a person can see until the beat
 * has passed — `invisible`, so the box its call site sized still holds its
 * place and the layout does not jump when the answer, or the pulse, arrives
 * — then the fill and its pulse.
 * @param {boolean} due whether the beat has passed (`indicatorDue`)
 */
export function placeholderFill(due) {
  return due ? "motion-pulse bg-surface-2" : "invisible";
}
