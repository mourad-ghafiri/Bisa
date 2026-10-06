/**
 * How the Project IDE's columns share a window (ide/01): the project rail on
 * the left, the documents in the centre, the right panel, and the occupant
 * rail's fixed column at the edge. The side columns keep the widths the
 * person set while the centre keeps `CENTRE_MIN`; past that the right panel
 * gives way first, down to its least, then the rail down to its least, and
 * then the rail folds — while the window is that narrow, never in the
 * person's stored widths or switches. The centre is the work: it is the one
 * column that never goes. Plain `.mjs`, so `node --test` reads the rule.
 */

/** The least the centre keeps: a document's tab strip and a line of code or prose. */
export const CENTRE_MIN = 360;

/** The occupant rail's column (`IconRail`, `w-10`), which never moves. */
export const ICON_RAIL_PX = 40;

/** A column's resize handle (`ResizeHandle`, its bar `w-1`): in the row, between the columns it parts. */
export const HANDLE_PX = 4;

/**
 * What the row spends on what never resizes: the occupant rail, and one
 * handle beside each side column that is open. Left out of the budget, the
 * handles pushed the row past the window by their width whenever the centre
 * stood at its least — the occupant rail's right edge was drawn off screen.
 * @param {{ railOpen: boolean, rightOpen: boolean }} p
 * @returns {number}
 */
export function fixedWidth(p) {
  return ICON_RAIL_PX + HANDLE_PX * ((p.railOpen ? 1 : 0) + (p.rightOpen ? 1 : 0));
}

/**
 * The columns as drawn at a width.
 * @param {{
 *   total: number,
 *   fixed: number,
 *   rail: number, railMin: number, railOpen: boolean,
 *   right: number, rightMin: number, rightOpen: boolean,
 * }} p `total` the row's measured width (0 before it is measured); `fixed` what never moves (the occupant rail)
 * @returns {{ rail: number | null, right: number | null, railFolded: boolean }} `null` for a column not drawn; `railFolded` when the rail is open but there is no room for it
 */
export function fitColumns(p) {
  const asked = { rail: p.railOpen ? p.rail : null, right: p.rightOpen ? p.right : null, railFolded: false };
  // Not measured yet: as the person set it, until the first frame says the width.
  if (!(p.total > 0)) return asked;
  let rail = p.railOpen ? p.rail : 0;
  let right = p.rightOpen ? p.right : 0;
  const short = () => CENTRE_MIN - (p.total - p.fixed - rail - right);
  if (short() <= 0) return asked;
  if (p.rightOpen) right = Math.max(Math.min(p.rightMin, p.right), right - short());
  if (short() > 0 && p.railOpen) rail = Math.max(Math.min(p.railMin, p.rail), rail - short());
  if (short() > 0 && p.railOpen) return { rail: null, right: p.rightOpen ? right : null, railFolded: true };
  return { rail: p.railOpen ? rail : null, right: p.rightOpen ? right : null, railFolded: false };
}
