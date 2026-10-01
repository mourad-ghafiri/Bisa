import type { GitStash, GitStatusInfo } from "../../types";

export type StashEntryLike = Pick<GitStash, "index" | "untracked" | "at"> & { message?: string | null; branch?: string | null };

export interface StashRow {
  /** `stash@{n}` */
  id: string;
  /** The person's message, or `WIP on <branch>`. */
  title: string;
  /** `on <branch>`, or `detached`. */
  where: string;
  untracked: boolean;
  at: number;
}

export interface CanStash {
  ok: boolean;
  reason: string | null;
}

export interface StashPushCopy {
  title: string;
  description: string;
  messageHint: string;
  untracked: { label: string; hint: string };
  keepIndex: { label: string; hint: string };
  confirm: string;
}

export interface StashConfirm {
  title: string;
  body: string;
  confirm: string;
  danger: boolean;
}

export type StashRefusalKind = "conflict" | "nothing_to_stash" | "stash_moved" | "in_progress" | "dirty" | "error";

export interface StashRefusal {
  kind: StashRefusalKind;
  sentence: string;
}

export declare function stashRow(entry: StashEntryLike): StashRow;
export declare function canStash(status: GitStatusInfo, includeUntracked?: boolean): CanStash;
export declare function stashPushCopy(paths: readonly string[]): StashPushCopy;
export declare function stashablePaths(
  files: readonly { path: string; staged?: boolean; unstaged?: boolean; untracked?: boolean; conflicted?: boolean }[] | null | undefined,
): string[];
export declare function stashConfirm(kind: "pop" | "drop", entry: StashEntryLike): StashConfirm;
export declare function stashedWords(entry: StashEntryLike, ref: string): string;
export declare function appliedWords(entry: StashEntryLike, ref: string): string;
export declare function poppedWords(entry: StashEntryLike, ref: string): string;
export declare function droppedWords(entry: StashEntryLike, ref: string): string;
export declare function stashRefusal(
  err: { code?: string | null; status?: number; message: string; detail?: Record<string, unknown> | null },
  verb: "push" | "apply" | "pop" | "drop",
): StashRefusal;
