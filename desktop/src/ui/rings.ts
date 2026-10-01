/**
 * The three rings the kit draws, named once so a cursor, a focus and a focused
 * pane never drift into four opacities of the same accent (ide/03).
 */

/** Keyboard focus, visible only when the keyboard put it there — the app's one focus ring. */
export const FOCUS_RING = "outline-none focus-visible:ring-1 focus-visible:ring-accent";
/** The row the keyboard is on, in a tree or a list with a roving cursor. */
export const CURSOR_RING = "ring-1 ring-inset ring-accent/50";
/** The pane that has focus among several. */
export const PANE_RING = "ring-1 ring-inset ring-accent/30";
