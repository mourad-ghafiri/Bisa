import type { GitIdentityLike } from "./gitIdentityModel.mjs";
/**
 * Types for `gitFiles.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 */

import type { GitFileRow, GitStatusInfo, SuggestedCommitMessage } from "../../types";
import type { FileStanding } from "../../ui/fileStandingModel.mjs";

/** One of a file's two patches — and therefore which letter it reads. */
export type GitSide = "staged" | "unstaged";

/** Which sides a file has; the one the Changes tree's row draws its chips and verbs from. */
export interface Standing {
  /** The index holds something HEAD does not. */
  staged: boolean;
  /** The working tree differs from the index. Both can be true. */
  unstaged: boolean;
  untracked: boolean;
  /** Unmerged: neither side, and its own standing for that reason. */
  conflicted: boolean;
  /** The side a click selects and Space acts on. */
  primary: GitSide;
}

export interface GitGroups {
  /** The index holds something HEAD does not. */
  staged: GitFileRow[];
  /** The worktree differs from the index. A row can be in this *and* staged. */
  unstaged: GitFileRow[];
  untracked: GitFileRow[];
  /** Unmerged: neither staged nor unstaged, and its own group for that reason. */
  conflicted: GitFileRow[];
}

/** A path and which of its two patches is open. */
export interface GitSelection {
  path: string;
  /** The index against HEAD, rather than the working tree against the index. */
  staged: boolean;
}

/** What Suggest left in the box, and what to say under it. */
export interface SuggestionOutcome {
  /** `null` means leave the box exactly as it is — nothing was suggested. */
  message: string | null;
  /** A sentence for the failure, or `null` when there was none. */
  note: string | null;
}

export interface CommitGate {
  message: string;
  groups: GitGroups | null;
  exists?: boolean;
  git?: boolean;
  /** `GET …/git/identity`, when loaded; `null`/absent means "not known yet", which is not a block. */
  identity?: GitIdentityLike | null;
  /** HEAD's commit, when there is one — what an amend rewrites. */
  head?: string | null;
  /** The Amend switch is on: the last commit is rewritten rather than a new one made. */
  amend?: boolean;
}

export declare const KIND_MARK: Record<string, string>;

export declare function kindForLetter(letter: string | null | undefined): string | null;
export declare function standingOf(row: Pick<GitFileRow, "staged" | "unstaged" | "untracked" | "conflicted"> | null | undefined): Standing;
export declare function letterPair(row: GitFileRow | null | undefined): string;
/** The explorer's standings, one per changed path: the kind from git's letters and whether it is the index's. */
export declare function treeStandings(files: readonly Pick<GitFileRow, "path" | "index" | "worktree" | "untracked" | "conflicted">[] | null | undefined): FileStanding[];
export declare function groupGitFiles(files: readonly GitFileRow[] | null | undefined): GitGroups;
export declare function isClean(groups: GitGroups): boolean;
export declare function selectionOf(
  row: GitFileRow | null | undefined,
  side: GitSide,
): GitSelection | null;
export declare function isSelected(
  selection: GitSelection | null | undefined,
  row: GitFileRow | null | undefined,
  side: GitSide,
): boolean;
export declare function keepSelection(
  selection: GitSelection | null | undefined,
  files: readonly GitFileRow[] | null | undefined,
): GitSelection | null;
export declare function commitBlockedReason(gate: CommitGate): string | null;
/** Reads the three fields every suggestion answer carries — a workstream's or the notes repository's. */
export declare function suggestionOutcome(
  response: Pick<SuggestedCommitMessage, "suggested" | "message" | "error"> | null | undefined,
): SuggestionOutcome;
export declare function diffStat(patch: string | null | undefined): {
  insertions: number;
  deletions: number;
};
export declare function noCommitsYet(status: GitStatusInfo | null | undefined): boolean;

export type RowActionId = "stage" | "unstage" | "discard" | "delete";
export interface RowAction {
  id: RowActionId;
  icon: RowActionId;
  label: (path: string) => string;
  tone: "quiet" | "danger";
  hint: string;
}
export declare function rowActions(standing: Standing): RowAction[];

export interface StageScope {
  paths: string[];
  count: number;
}
export interface StageScopes {
  /** Everything git has not got yet — the modified tracked files and the untracked ones. */
  all: StageScope;
  /** The modified tracked files alone — `git add -u`'s meaning. */
  tracked: StageScope;
  untracked: StageScope;
  /** What *Unstage all* takes back. */
  staged: StageScope;
  /** What *Discard all changes…* puts back: the working-tree changes and the unmerged paths. */
  discardable: StageScope;
}
export declare function stageScopes(groups: GitGroups | null | undefined): StageScopes;

export type GitFileMenuId = "open" | "attach-agent" | "stage" | "unstage" | "discard" | "delete" | "copy-path" | "copy-absolute" | "reveal-files";
export declare function gitFileMenu(ctx: { standing: Standing; desktop: boolean; canOpen: boolean }): {
  id: GitFileMenuId;
  label: string;
  command?: string;
  separatorBefore?: boolean;
  disabled?: boolean;
  danger?: boolean;
}[];
