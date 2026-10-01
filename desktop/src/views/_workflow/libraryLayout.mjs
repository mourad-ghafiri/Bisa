/**
 * How the library lays its cards out — the one grid the Workflows screen's
 * cards and the template gallery share.
 *
 * The list fills the screen's width like every other list (the Goals list,
 * the Agents roster): a card has a fixed thumbnail height (`LibraryCard`), so
 * a wider window adds columns, never stretched cards. The library once sat in
 * a centred column (`mx-auto max-w-5xl`) that no other screen had, and the
 * gallery repeated the grid's classes by hand.
 */

/** The cards' grid: two across on a small window, three, then four on a wide one. */
export const LIBRARY_GRID = "grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4";
