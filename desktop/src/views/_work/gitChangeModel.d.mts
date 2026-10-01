/**
 * Types for `gitChangeModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

export type GitChange = "worktree" | "index" | "head" | "refs" | "stash" | "operation";

export declare const GIT_CHANGES: readonly GitChange[];
export declare const GIT_COALESCE_MS: number;

export declare function gitChangeOf(path: string): GitChange | null;
export declare function kindsOf(frame: { path: string; kind: string }): Set<GitChange>;

export interface GitReads {
  status: boolean;
  files: boolean;
  history: boolean;
  branches: boolean;
  stashes: boolean;
}
export declare function readsFor(kinds: Iterable<GitChange>): GitReads;
