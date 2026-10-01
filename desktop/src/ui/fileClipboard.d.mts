export type ClipKind = "copy" | "cut";
export interface Clip {
  kind: ClipKind;
  /** The tree's key (`scope:id`). */
  root: string;
  paths: string[];
}
export interface PasteOp {
  from: string;
  to: string;
  op: "copy" | "move";
}
export declare function clipFrom(kind: ClipKind, root: string, paths: Iterable<string>): Clip | null;
export declare function isCut(clip: Clip | null, path: string): boolean;
export declare function pasteTargets(clip: Clip | null, into: { targetDir: string; root: string; existing: Iterable<string> }): { ops: PasteOp[] } | { refused: string; /** The paste would change nothing: said if asked, never as a failure. */ idle?: true };
export declare function afterPaste(clip: Clip | null): Clip | null;
export declare function pasteSummary(ops: readonly PasteOp[]): string;
