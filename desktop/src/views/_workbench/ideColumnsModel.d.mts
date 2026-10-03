/** The least the centre keeps. */
export declare const CENTRE_MIN: number;
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
