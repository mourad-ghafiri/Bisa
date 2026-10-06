/** The least the centre keeps. */
export declare const CENTRE_MIN: number;
/** The occupant rail's column, which never moves. */
export declare const ICON_RAIL_PX: number;
/** A column's resize handle, in the row between the columns it parts. */
export declare const HANDLE_PX: number;
/** What the row spends on what never resizes: the occupant rail and one handle per open side column. */
export declare function fixedWidth(p: { railOpen: boolean; rightOpen: boolean }): number;
/** The Project IDE's columns at a width: the side columns give way to the centre, the right panel first, then the rail folds. */
export declare function fitColumns(p: {
  total: number;
  fixed: number;
  rail: number;
  railMin: number;
  railOpen: boolean;
  right: number;
  rightMin: number;
  rightOpen: boolean;
}): { rail: number | null; right: number | null; railFolded: boolean };
