export type CommitActionId = "cherry_pick" | "revert" | "checkout" | "branch" | "tag" | "inspect" | "copy_sha" | "copy_short" | "attach";
export type ConsentedKind = "checkout" | "cherry_pick" | "revert" | "switch" | "delete_tag";
export type NamedKind = "branch" | "tag";
export type RefActionId = "switch" | "open_workstream" | "copy_ref" | "delete_tag";
/** The `ICON` key an action draws with. */
export type ActionIcon = "cherryPick" | "revert" | "checkout" | "branchHere" | "tagHere" | "inspect" | "copy" | "agent" | "switchBranch" | "delete";

export interface CommitActionDef {
  id: CommitActionId;
  label: string;
  menuLabel: string;
  icon: ActionIcon;
  consented: boolean;
  hint: string;
}
export interface CommitAction extends CommitActionDef {
  disabled: boolean;
  reason: string | null;
}
export interface MenuAction extends CommitAction {
  separatorBefore: boolean;
}
export interface RefAction {
  id: RefActionId;
  label: string;
  icon: ActionIcon;
  consented: boolean;
  danger: boolean;
  disabled: boolean;
  reason: string | null;
}
export interface ActionContext {
  busy?: boolean;
  headId?: string | null;
  inProgress?: string | null;
}
export interface RefContext {
  busy?: boolean;
  inProgress?: string | null;
  currentBranch?: string | null;
}
export interface ConsentCopy {
  title: string;
  body: string;
  confirm: string;
  danger: boolean;
}

export declare const COMMIT_ACTIONS: readonly CommitActionDef[];
export declare const MENU_SECTIONS: readonly (readonly CommitActionId[])[];
export declare function disabledReason(id: CommitActionId, row: { id: string }, ctx?: ActionContext): string | null;
export declare function commitActions(row: { id: string }, ctx?: ActionContext): CommitAction[];
export declare function menuActions(row: { id: string }, ctx?: ActionContext): MenuAction[];
export declare function refActions(ref: { name: string; kind: string }, ctx?: RefContext): RefAction[];
export declare function headOf(rows: readonly ({ id: string; refs: readonly { kind: string }[] } | undefined | null)[]): string | null;
export declare function refNameProblem(name: string | null | undefined): string | null;
export declare function shortRecovery(ref: string | null | undefined): string;
export declare function consentWords(kind: ConsentedKind, target: { short: string; name?: string; parents?: number }): ConsentCopy;
export declare function doneWords(kind: ConsentedKind | NamedKind, facts: { short: string; name?: string; recovery?: string | null }): string;
export declare function conflictWords(err: { status?: number; code?: string | null; detail?: unknown } | null | undefined, fallback: string): string;
