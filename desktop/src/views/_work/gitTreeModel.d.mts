/**
 * Types for `gitTreeModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { GitFileRow } from "../../types";
import type { TreeRowLike } from "../../ui/tree/treeListModel.mjs";
import type { GitSelection, RowAction, RowActionId, Standing } from "./gitFiles.mjs";
import type { ChangesLayout } from "../_workbench/rightPanelModel.mjs";

export interface DirRow extends TreeRowLike {
  kind: "dir";
  /** The one group above the folders — the unmerged paths — rather than a folder of the project. */
  group?: "conflicted";
  /** The folder's name — one segment, as the explorer draws it. */
  label: string;
  /** The folder's full path — its fold id. */
  path: string;
  /** Every file under it, in the tree's order. */
  paths: string[];
  count: number;
  /** The files under it each verb takes. */
  stageable: string[];
  unstageable: string[];
  discardable: string[];
  deletable: string[];
}
export interface FileRow extends TreeRowLike {
  kind: "file";
  /** The basename in the tree layout, the full path in the list. */
  label: string;
  row: GitFileRow;
  standing: Standing;
}
export type ChangesRow = DirRow | FileRow;

export interface ActTarget {
  kind: "dir" | "file";
  paths: string[];
  /** The folder named in a confirmation, for a folder's verb. */
  under: string | null;
}

export declare function dirId(path: string): string;
export declare function fileId(path: string): string;
export declare function changesTreeRows(files: readonly GitFileRow[] | null | undefined, layout: ChangesLayout, folded: ReadonlySet<string>): ChangesRow[];
export declare function foldableIds(files: readonly GitFileRow[] | null | undefined): string[];
export declare function toggledFold(folds: readonly string[], id: string, open: boolean): string[];
export declare function selectedIds(selection: GitSelection | null | undefined): Set<string>;
export declare function nextCursor(prevRows: readonly { id: string }[], cursor: string | null, rows: readonly { id: string }[]): string | null;
export declare function actOn(rows: readonly ChangesRow[], id: string | null, verb: RowActionId): ActTarget | null;
export declare function spaceVerb(row: ChangesRow): "stage" | "unstage" | null;
export declare function deleteVerb(row: ChangesRow): "discard" | "delete" | null;
export declare function folderActions(dir: Pick<DirRow, "path" | "stageable" | "unstageable" | "discardable" | "deletable">): RowAction[];

export type GitFolderMenuId = "stage" | "unstage" | "discard" | "delete" | "copy-path" | "copy-absolute" | "reveal-files";
export declare function gitFolderMenu(
  dir: Pick<DirRow, "stageable" | "unstageable" | "discardable" | "deletable">,
  ctx: { desktop: boolean },
): {
  id: GitFolderMenuId;
  label: string;
  command?: string;
  separatorBefore?: boolean;
  danger?: boolean;
}[];
export declare const CONFLICTED_GROUP: string;
/** A folder row's tooltip: the conflicted group's, or the folder's path and its count. */
export declare function dirRowTitle(row: Pick<DirRow, "group" | "path" | "count">): string;
