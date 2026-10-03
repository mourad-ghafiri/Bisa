/**
 * How the library lays its cards out — the one grid the Workflows screen's
 * cards and the template gallery share.
 *
 * The list fills the screen's width like every other list (the Goals list,
 * the Agents roster): a card has a fixed thumbnail height (`LibraryCard`), so
 * more room adds columns, never stretched cards. The library once sat in
 * a centred column (`mx-auto max-w-5xl`) that no other screen had, and the
 * gallery repeated the grid's classes by hand.
 */

/**
 * The cards' grid: as many columns of at least 13.75rem as the list's own
 * room holds — three beside a 1024px window's sidebar, four at 1440, six on
 * a 1920px display. Read from the room, not the window: a Details pane open
 * beside the list takes columns away, as it should.
 */
export const LIBRARY_GRID = "grid gap-3 grid-cols-[repeat(auto-fill,minmax(13.75rem,1fr))]";
