/**
 * Types for `dragData.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step.
 */

export interface PathDrag {
  type: "path";
  scope: string;
  id: string;
  path: string;
  dir: boolean;
  /** Everything on the move, `path` included. */
  paths: string[];
  label: string;
}
export interface HunkDrag {
  type: "hunk";
  path: string;
  staged: boolean;
  text: string;
  id: string;
  label: string;
}
export interface DocTabDrag {
  type: "doc-tab";
  id: string;
  pane: string;
  label: string;
}
export interface TerminalTabDrag {
  type: "terminal-tab";
  key: string;
  pane: string;
  label: string;
}
export interface RailRowDrag {
  type: "rail-row";
  kind: string;
  id: string;
  ctx: string;
  label: string;
}

export interface NavRowDrag {
  type: "nav-row";
  key: string;
  label: string;
  /** The `ICON` name the destination wears. */
  glyph: string;
}

export interface WorkstreamCardDrag {
  type: "workstream-card";
  id: string;
  column: string;
  label: string;
}

/** Everything a drag in the app can carry. */
export type DragData = PathDrag | HunkDrag | DocTabDrag | TerminalTabDrag | RailRowDrag | NavRowDrag | WorkstreamCardDrag;

export declare function pathDrag(p: { scope: string; id: string; path: string; dir: boolean; paths?: readonly string[] }): PathDrag;
export declare function hunkDrag(p: { path: string; staged: boolean; text: string; id: string }): HunkDrag;
export declare function docTabDrag(id: string, pane: string, label: string): DocTabDrag;
export declare function terminalTabDrag(key: string, pane: string, label: string): TerminalTabDrag;
export declare function railRowDrag(kind: string, id: string, ctx: string, label: string): RailRowDrag;
export declare function navRowDrag(key: string, label: string, glyph: string): NavRowDrag;
export declare function workstreamCardDrag(id: string, column: string, label: string): WorkstreamCardDrag;
export declare function dragType(data: unknown): DragData["type"] | null;
export declare function isDragOf<K extends DragData["type"]>(data: unknown, ...types: K[]): data is Extract<DragData, { type: K }>;

/** The `ICON` name the drag ghost wears for a payload, or null. */
export declare function dragGlyph(data: unknown): string | null;
/** How many things a drag moves. */
export declare function dragCount(data: unknown): number;
