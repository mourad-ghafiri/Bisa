/** Types for `paneTreeModel.mjs`, plain JavaScript so `node --test` runs it. */

export interface Leaf {
  kind: "leaf";
  id: string;
  tabs: string[];
  active: string | null;
}
export interface Split {
  kind: "split";
  id: string;
  dir: "row" | "col";
  ratio: number;
  a: PaneNode;
  b: PaneNode;
}
export type PaneNode = Leaf | Split;
export type Direction = "left" | "right" | "up" | "down";

export interface Rect {
  leafId: string;
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface Divider {
  splitId: string;
  dir: "row" | "col";
  x: number;
  y: number;
  w: number;
  h: number;
  /** The divider's position along the split axis, as a fraction of the panel. */
  at: number;
}

export declare function singleLeaf(id: string, tabs?: string[], active?: string | null): Leaf;
export declare function leaves(tree: PaneNode | null | undefined): Leaf[];
export declare function findLeaf(tree: PaneNode, leafId: string): Leaf | null;
export declare function leafOfTab(tree: PaneNode, tabKey: string): Leaf | null;
export declare function normalize(tree: PaneNode): PaneNode;
export declare function addTab(tree: PaneNode, leafId: string, tabKey: string, activate?: boolean): PaneNode;
export declare function removeTab(tree: PaneNode, tabKey: string, collapse?: boolean): PaneNode;
export declare function moveWithin(tree: PaneNode, tabKey: string, index: number): PaneNode;
export declare function setActiveTab(tree: PaneNode, leafId: string, tabKey: string): PaneNode;
export declare function splitLeaf(tree: PaneNode, leafId: string, dir: "row" | "col", splitId: string, newLeafId: string, moveActive?: boolean): PaneNode;
export declare function closeLeaf(tree: PaneNode, leafId: string): PaneNode;
export declare function setRatio(tree: PaneNode, splitId: string, ratio: number): PaneNode;
export declare function rects(tree: PaneNode | null | undefined): Rect[];
export declare function dividers(tree: PaneNode | null | undefined): Divider[];
export declare function neighbor(tree: PaneNode, leafId: string, direction: Direction): string | null;
export declare function parseTree(json: unknown, validTabs: Iterable<string>): PaneNode | null;
