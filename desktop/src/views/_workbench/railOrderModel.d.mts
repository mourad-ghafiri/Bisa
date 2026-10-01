/**
 * The manual project-rail ordering: read the `rail.order` setting and
 * produce the next value after a drag.
 */

/** The `rail.order` setting shape. */
export interface RailOrder {
  projects?: string[];
  groups?: string[];
  workstreams?: Record<string, string[]>;
}

/** Which list a drag reorders. */
export type RailDimension =
  | { kind: "projects" }
  | { kind: "groups" }
  | { kind: "workstreams"; project: string };

export declare function projectOrder(order: RailOrder | null | undefined): string[];
export declare function groupOrder(order: RailOrder | null | undefined): string[];
export declare function workstreamOrder(order: RailOrder | null | undefined, project: string): string[];
export declare function orderBy<T>(items: readonly T[], saved: readonly string[] | undefined, idOf: (item: T) => string): T[];
export declare function placeAmong(all: readonly string[], siblings: readonly string[], id: string, index: number): readonly string[];
export declare function reorderAmong(
  order: RailOrder | null | undefined,
  dimension: RailDimension,
  all: readonly string[],
  siblings: readonly string[],
  id: string,
  index: number,
): RailOrder;
export declare function renameGroup(order: RailOrder | null | undefined, old: string, next: string): RailOrder;
