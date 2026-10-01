import type { DragData, DropPlan } from "../../ui";
import type { RailRow, RailTreeRow } from "./projectRailModel.mjs";
import type { RailOrder } from "./railOrderModel.mjs";

/** What a drop the tree planned means. */
export type RailDropVerdict =
  | { kind: "order"; order: RailOrder }
  | { kind: "regroup"; project: string; group: string | null; order: RailOrder; words: string }
  | { kind: "sessions"; key: string; before: number };

/** A rail with no heading at all — nobody made a group, every project at the top. */
export declare function isHeadless(rows: readonly RailRow[]): boolean;
/** What a row carries when dragged; null for one that stays put. */
export declare function railDragOf(row: RailRow, opts?: { renaming?: { kind: string; id: string } | null }): DragData | null;
/** Whether a payload may nest into a row. */
export declare function railCanNest(data: DragData, target: RailRow): boolean;
/** Whether a payload may land among a parent's children (`null` for the top). */
export declare function railCanReorder(data: DragData, parent: RailRow | null, opts?: { headless?: boolean }): boolean;
/** The next order, a project moving house, or a shell's place — or null when nothing would change. */
export declare function railDropVerdict(order: RailOrder, data: DragData, plan: DropPlan | null, rows: readonly RailRow[], treeRows: readonly RailTreeRow[]): RailDropVerdict | null;
