import type { BranchInfo, RemoteBranchInfo } from "../../types";

export type BranchActionId = "switch" | "merge" | "rebase" | "cherry_pick" | "rebase_plan" | "push_lease" | "upstream" | "workstream" | "rename" | "delete";
export type RemoteBranchActionId = "checkout" | "merge" | "rebase" | "cherry_pick" | "fetch" | "workstream" | "delete_remote" | "copy_name" | "copy_full";
export type BranchConsentKind = "switch" | "checkout_remote" | "delete_branch" | "delete_remote" | "delete_tag" | "restore" | "push_lease";
export type BranchDoneKind = BranchConsentKind | "merge" | "rebase" | "rebase_plan" | "cherry_pick" | "revert" | "rename" | "create" | "create_switch" | "tag" | "upstream" | "abort";
export type BranchActionIcon = "switchBranch" | "merge" | "rebase" | "cherryPick" | "send" | "link" | "workstream" | "edit" | "delete" | "refresh" | "copy";

export interface BranchAction<Id extends string = BranchActionId> {
  id: Id;
  label: string;
  icon: BranchActionIcon;
  /** The row's one verb at rest on hover; the rest are the `⋮` menu's. */
  hover: boolean;
  consented: boolean;
  danger: boolean;
  disabled: boolean;
  reason: string | null;
  separatorBefore?: boolean;
}
export interface BranchContext {
  current?: string | null;
  defaultBranch?: string | null;
  busy?: boolean;
  inProgress?: string | null;
}
export interface ConsentCopy {
  title: string;
  body: string;
  confirm: string;
  danger: boolean;
  /** The recovery kind the act writes, for the dialog's SafetyNote. */
  kind: "commit" | "tree" | "stash";
}
export interface RecoveryFacts {
  ref_name: string;
  kind: "commit" | "tree" | "stash";
  branch?: string | null;
}

export declare function halfDoneReason(inProgress: string | null | undefined): string;
export declare function branchActions(branch: Pick<BranchInfo, "name" | "current"> & { upstream?: string | null }, ctx?: BranchContext): BranchAction[];
export declare function menuActions(branch: Pick<BranchInfo, "name" | "current"> & { upstream?: string | null }, ctx?: BranchContext): BranchAction[];
export declare function remoteBranchActions(branch: Pick<RemoteBranchInfo, "remote" | "name"> & { tracked?: boolean }, ctx?: BranchContext): BranchAction<RemoteBranchActionId>[];
export declare function consentWords(kind: BranchConsentKind, facts?: { name?: string; from?: string; remote?: string; current?: string | null; rec?: RecoveryFacts | null }): ConsentCopy;
export declare function doneWords(kind: BranchDoneKind, facts?: { name?: string; to?: string; op?: string; count?: number }): string;
export declare function branchFilter<B extends { name: string; upstream?: string | null }>(rows: readonly B[], text: string | null | undefined): B[];
export declare function branchOrder<B extends { name: string; current: boolean }>(rows: readonly B[], defaultBranch: string | null): B[];
export declare function localNameFor(branch: Pick<RemoteBranchInfo, "remote" | "name">, locals: readonly { name: string }[]): string;
export declare function standingWords(branch: Pick<BranchInfo, "ahead" | "behind">): string;
