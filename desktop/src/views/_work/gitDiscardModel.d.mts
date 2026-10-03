import type { Disposal, GitRecoveryKind, GitRecoveryRef } from "../../types";

export declare function recoveryWords(kind: GitRecoveryKind): string;
export declare function recoveryOpWords(op: string): string;
export declare function restoreWords(rec: Pick<GitRecoveryRef, "ref_name" | "kind" | "branch">): string;

export interface DiscardCopy {
  title: string;
  body: string;
  confirm: string;
  danger: boolean;
}

export declare function shortRef(ref: string | null | undefined): string;
export declare function discardCopy(paths: readonly string[], of?: { under?: string | null }): DiscardCopy;
export declare function deleteCopy(paths: readonly string[], disposal: Disposal | null, of?: { under?: string | null; all?: boolean }): DiscardCopy;
export declare function discardedWords(paths: readonly string[], ref: string): string;
export declare function deletedWords(paths: readonly string[], disposal: Disposal): string;
