/**
 * A dialog's layout, as the classes that make it (`Dialog.tsx`).
 *
 * Kept apart from the component so the two rules that keep a button on
 * screen are a test's to hold, not a reviewer's to remember:
 *
 * 1. **The body scrolls, never the panel.** The panel is a column — header,
 *    body, footer — capped at the viewport; the header and the footer keep
 *    their height and the body takes what is left. A long body scrolls under
 *    a footer that stays where it is. When the panel itself was the
 *    scrollport, a long body scrolled the buttons out of sight.
 * 2. **A footer wraps.** Its row is right-aligned and the kit's `Button`
 *    never shrinks, so a row wider than the panel has nowhere to go but out
 *    of the panel's *left* edge — and the first button, by convention
 *    Cancel, is the one cut off. A row that cannot fit breaks onto a second
 *    line instead; nothing is clipped at any width.
 */

export const OVERLAY = "motion-overlay fixed inset-0 z-50 bg-overlay backdrop-blur-[1px]";

/** The panel: a column capped at the viewport, clipping nothing of its own. */
export const PANEL =
  "motion-panel fixed top-[8vh] left-1/2 z-50 flex max-h-[84vh] w-[calc(100vw-3rem)] -translate-x-1/2 flex-col overflow-hidden rounded-card border border-border bg-surface shadow-xl outline-none";

/** The header keeps its height whatever the body holds. */
export const HEADER = "shrink-0 border-b border-hairline px-5 pt-4 pb-3";

/** The body is the one scrollport. */
export const BODY = "min-h-0 flex-1 overflow-y-auto @container px-5 py-4";

/** The footer keeps its height, aligns its actions right, and wraps a row that cannot fit. */
export const FOOTER = "flex shrink-0 flex-wrap justify-end gap-2 border-t border-hairline bg-surface-2/40 px-5 py-3";

/**
 * A destructive confirmation's action, filled: the kit's `danger` button is
 * a quiet outline — right beside other verbs — and the one button a person
 * came to a confirmation to press says so louder.
 */
export const SOLID_DANGER = "border-transparent bg-danger text-accent-contrast hover:bg-danger hover:opacity-90";
