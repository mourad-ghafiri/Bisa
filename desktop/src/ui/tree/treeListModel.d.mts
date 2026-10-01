/**
 * Types for `treeListModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

/** The least a row must say to be navigated and dropped on. */
export interface TreeRowLike {
  id: string;
  depth: number;
  /** For type-ahead. */
  label?: string;
  /** False for a message row the cursor never lands on. Absent means true. */
  focusable?: boolean;
  expandable?: boolean;
  expanded?: boolean;
}

export type TreeKeyAction =
  | { kind: "cursor"; id: string }
  | { kind: "expand"; id: string }
  | { kind: "collapse"; id: string }
  | { kind: "expandAll"; ids: string[] };

export interface DropPlan {
  /** The parent the dragged row would join; null for the top level. */
  parent: string | null;
  /** Its index among that parent's children, the dragged row excluded. */
  index: number;
  mode: "before" | "after" | "inside";
  /** The row the pointer was over. */
  over: string;
  /** The depth the row would take — where the indicator draws. */
  depth: number;
  /** Exactly where it already is. */
  noop: boolean;
}

export declare function parentsFromDepth(rows: readonly TreeRowLike[]): Map<string, string | null>;
export declare function childrenOf(rows: readonly TreeRowLike[], parents: Map<string, string | null>, parentId: string | null): string[];
export declare function rowIndexOf(rows: readonly TreeRowLike[], id: string | null): number;
export declare function keyAction(rows: readonly TreeRowLike[], cursor: string | null, key: string, opts?: { page?: number }): TreeKeyAction | null;
export declare function typeAhead(rows: readonly TreeRowLike[], cursor: string | null, prefix: string): string | null;
export declare function dropPlan<T extends TreeRowLike>(
  rows: readonly T[],
  active: string | readonly string[] | null,
  over: string,
  ratio: number,
  opts?: { canNest?: (target: T) => boolean; canReorder?: (parent: T | null) => boolean },
): DropPlan | null;

/** What a drag is being told over a tree. */
export type DragCue = "none" | "refused" | "stay" | "nest" | "move";

export declare function indicatorStyle(plan: DropPlan | null, geom: { indent: number; base: number; rowTop: number; rowHeight: number }): { x: number; y: number } | null;
export declare function dragCue(plan: DropPlan | null, overTree: boolean): DragCue;
export declare function neighbourShift(plan: DropPlan | null, id: string, prev: string | null, next: string | null): -2 | 0 | 2;
export declare function dropSlots<T extends TreeRowLike>(
  rows: readonly T[],
  active: string | readonly string[] | null,
  opts?: { canNest?: (target: T) => boolean; canReorder?: (parent: T | null) => boolean },
): DropPlan[];
export declare function stepSlot(rows: readonly TreeRowLike[], slots: readonly DropPlan[], current: DropPlan | null, key: string): DropPlan | null;
