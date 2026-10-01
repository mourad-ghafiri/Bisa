/** Types for `anchorModel.mjs`, plain JavaScript so `node --test` reads it. */

/** A row as the list lays it out: its key, its top and its height, in the list's own pixels. */
export interface AnchorRow {
  key: string;
  top: number;
  height: number;
}

/** A list's place as a row: the first row in view, and how far into it the viewport's top edge stood. */
export interface Anchor {
  key: string;
  offset: number;
}

export declare function firstVisible(rows: readonly AnchorRow[], top: number): number;
export declare function anchorOf(rows: readonly AnchorRow[], top: number): Anchor | null;
export declare function scrollFor(anchor: Anchor | null | undefined, rows: readonly AnchorRow[]): number | null;
/** An anchor read back from a memory that outlives the window, or null for what is no anchor. */
export declare function parseAnchor(raw: unknown): Anchor | null;
