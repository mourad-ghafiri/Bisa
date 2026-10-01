/** Types for `threadPlaceModel.mjs`. */

/** Where a thread was being read: a message, and how far the viewport's top edge is inside it. */
export interface ThreadPlace {
  message: string;
  offset: number;
}

/** A message row as measured: from the content's top, in pixels. */
export interface ThreadRow {
  id: string;
  top: number;
  height: number;
}

export type RestoreStep =
  | { do: "wait" }
  | { do: "scroll"; message: string; offset: number }
  | { do: "older" }
  | { do: "bottom"; forget: boolean };

export declare const RESTORE_PAGES: number;
export declare function placeFrom(rows: readonly ThreadRow[], top: number): ThreadPlace | null;
export declare function parseThreadPlace(raw: unknown): ThreadPlace | null;
export declare function restoreStep(facts: {
  kept: ThreadPlace | null | undefined;
  loading: boolean;
  ids: readonly string[];
  hasOlder: boolean;
  pagesLoaded: number;
}): RestoreStep;
