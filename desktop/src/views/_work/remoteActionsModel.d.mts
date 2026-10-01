import type { BranchInfo, RemoteBranchInfo, RemoteInfo } from "../../types";

export type RemoteActionId = "fetch" | "edit_url" | "copy_url" | "open_host" | "delete";
export type RemoteActionIcon = "refresh" | "edit" | "copy" | "open" | "delete" | "workstream";

export interface RemoteAction<Id extends string = RemoteActionId> {
  id: Id;
  label: string;
  icon: RemoteActionIcon;
  hover: boolean;
  consented: boolean;
  danger: boolean;
  disabled: boolean;
  reason: string | null;
  separatorBefore?: boolean;
}

export interface RemoteGroup {
  remote: RemoteInfo;
  branches: (RemoteBranchInfo & { full: string; trackedBy: string | null })[];
}

export declare function remoteActions(remote: RemoteInfo, ctx?: { busy?: boolean }): RemoteAction[];
export declare function hostPage(url: string | null | undefined): string | null;
export declare function groupRemoteBranches(remotes: readonly RemoteInfo[], remoteBranches: readonly RemoteBranchInfo[], branches: readonly Pick<BranchInfo, "name" | "upstream">[]): RemoteGroup[];
export declare function fetchWords(count: number, opts?: { running?: boolean }): string;
export declare function fetchedWords(results: readonly { name: string; ok: boolean; error?: string }[]): string;
export declare function deleteRemoteWords(remote: RemoteInfo): { title: string; body: string; confirm: string; danger: boolean; url: string; done: string };
export declare function noBranchesWords(remote: string): string;
export declare function remoteTitle(remote: RemoteInfo): string;
