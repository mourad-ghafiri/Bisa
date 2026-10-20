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

export declare const WRITES_TREE: readonly string[];
export declare const MAX_IDLE_ROOTS: number;

export interface IdleRoot {
  subscribers: number;
  inFlight: boolean;
  flush: unknown;
  /** The count at which the root was last left by its last subscriber; 0 for one never shown. */
  leftAt: number;
}
export declare function idleRootsToDrop(entries: Iterable<readonly [string, IdleRoot]>, max: number): string[];

export type Reread = { kind: "shown" } | { kind: "root"; id: string } | { kind: "none" };
export declare function rereadsFor(frame: { type?: string; scope?: string; id?: string; path?: string; kind?: string } | null | undefined): Reread;
