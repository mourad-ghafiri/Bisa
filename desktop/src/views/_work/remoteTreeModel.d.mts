/**
 * Types for `remoteTreeModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { TreeRowLike } from "../../ui/tree/treeListModel.mjs";
import type { RemoteGroup } from "./remoteActionsModel.mjs";

export type RemoteLayout = "tree" | "list";
export type GroupBranch = RemoteGroup["branches"][number];

export interface RemoteStanding {
  isDefault: boolean;
  trackedBy: string | null;
}

export interface RemoteDirRow extends TreeRowLike {
  kind: "dir";
  label: string;
  path: string;
  count: number;
}

export interface RemoteBranchRow extends TreeRowLike {
  kind: "branch";
  /** The basename in the tree, the whole name in the list. */
  label: string;
  branch: GroupBranch;
  standing: RemoteStanding;
  title: string;
}

export type RemoteRow = RemoteDirRow | RemoteBranchRow;

export declare function folderId(wid: string, remote: string, path: string): string;
export declare function standingOf(branch: Pick<GroupBranch, "name" | "trackedBy">, ctx?: { defaultBranch?: string | null }): RemoteStanding;
export declare function branchTitle(branch: Pick<GroupBranch, "remote" | "name" | "subject" | "head">): string;
export declare function remoteBranchRows(
  wid: string,
  group: RemoteGroup,
  opts: { layout: RemoteLayout; folded: ReadonlySet<string>; filter?: string; defaultBranch?: string | null },
): RemoteRow[];
export declare function foldableIds(wid: string, group: RemoteGroup): string[];
export declare function countWords(n: number): string;
export declare function wantsFilter(groups: readonly RemoteGroup[], threshold?: number): boolean;
