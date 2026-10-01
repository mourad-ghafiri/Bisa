/** A node as the mirror holds it: the caller's fields plus what xyflow measured. */
export interface MirrorNode {
  id: string;
  position: { x: number; y: number };
  type?: string;
  selected?: boolean;
  draggable?: boolean;
  connectable?: boolean;
  data?: unknown;
  measured?: { width?: number; height?: number };
  width?: number;
  height?: number;
  [k: string]: unknown;
}

export interface MirrorEdge {
  id: string;
  source: string;
  target: string;
  sourceHandle?: string | null;
  targetHandle?: string | null;
  label?: unknown;
  animated?: boolean;
  className?: string;
  type?: string;
  selected?: boolean;
  data?: { waypoints?: readonly { x: number; y: number }[] } | undefined;
  [k: string]: unknown;
}

export declare function sameShallow<T extends object>(a: T | null | undefined, b: T | null | undefined, fields?: readonly (keyof T & string)[]): boolean;
export declare function reuse<T>(prev: T | undefined, next: T, same: (prev: T, next: T) => boolean): T;
export declare function reconcileNodes<N extends MirrorNode>(prev: readonly N[], next: readonly N[]): N[];
export declare function reconcileEdges<E extends MirrorEdge>(prev: readonly E[], next: readonly E[]): E[];
