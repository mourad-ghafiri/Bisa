import type { GitFileRow, GitStash, GraphRefScope } from "../../types";
import type { GitSelection } from "./gitFiles.mjs";
import type { GitOperation } from "./operationModel.mjs";
import type { Publish } from "./PublishOutcomeBanner";
import type { PullBanner } from "./syncModel.mjs";

export declare const MAX_SESSIONS: number;
/** How many of one component's own drafts the memory tier keeps, the least recently touched going first. */
export declare const MAX_SESSION_DRAFTS: number;

export interface GitDraft {
  readonly message: string;
}
export declare const EMPTY_DRAFT: GitDraft;

export type GitBusy =
  | "stage"
  | "unstage"
  | "discard"
  | "delete"
  | "commit"
  | "amend"
  | "suggest"
  | "stash"
  | "fetch"
  | "pull"
  | "push"
  | "abort"
  | "continue"
  | "skip"
  | "resolve"
  | "checkout"
  | "merge_branch"
  | "rebase"
  | "cherry_pick"
  | "revert"
  | "branch"
  | "upstream"
  | "delete_remote"
  | "tag"
  | "restore"
  | "push_lease"
  | "pr"
  | "merge";

export type FailureKind = "stage" | "unstage" | "discard" | "delete" | "commit" | "amend" | "suggest" | "stash" | "stash_apply" | "stash_pop" | "stash_drop" | "branch";

export interface GitFailure {
  readonly kind: FailureKind;
  readonly error: string;
  readonly paths: readonly string[];
}

export type GitPending =
  /** A discard of files — one, several, or every file under `under`, which the confirmation names. */
  | { kind: "discard"; paths: string[]; under?: string | null }
  /** A delete of untracked files through the IDE's disposal — one, the untracked files under `under`, or every one in the checkout (`all`, the toolbar's). */
  | { kind: "delete"; paths: string[]; under?: string | null; all?: boolean }
  | { kind: "stash_pop"; entry: GitStash }
  | { kind: "stash_drop"; entry: GitStash }
  /** The last commit rewritten with the draft and what is staged (ide/04 §Amend). */
  | { kind: "amend" };

export interface GitSession {
  readonly busy: GitBusy | null;
  /** The composer's Amend switch: the primary verb rewrites the last commit instead of making one. */
  readonly amend: boolean;
  /** Suggest's explanation, or the sentence a failed suggest left. */
  readonly note: string | null;
  readonly failure: GitFailure | null;
  readonly pending: GitPending | null;
  readonly selection: GitSelection | null;
  /** The Stash… dialog: `[]` for the whole tree, paths for a row's. */
  readonly stashing: string[] | null;
  readonly shownStash: string | null;
  /** The rows the last write answered with; `null` once a read replaced them. */
  readonly files: readonly GitFileRow[] | null;
  readonly pullBanner: PullBanner | null;
  /** The sync bar's push report. */
  readonly publish: Publish;
  /** The lifecycle's report — a pull request opened, a merge. */
  readonly prPublish: Publish;
  readonly prOpen: boolean;
  readonly afterMerge: { primaryBusy: number | null } | null;
  /** Bumped when a write landed; a mounted reader reloads on change. */
  readonly stale: number;
  /** The History view's one filter, kept per checkout so a view switch keeps it (ide/05). */
  readonly graphRefs: GraphRefScope;
  /** The Changes tree's rows folded shut — sections and folders, by row id (`gitTreeModel`). */
  readonly changesFolds: readonly string[];
  /** The operation this app run started that stopped on conflicts — what the Resolve card names (`operationModel`); null when none, or when it was started elsewhere. */
  readonly operation: GitOperation | null;
}
export declare const EMPTY_SESSION: GitSession;

/** How the panel stood — the part of a session a restart keeps. */
export interface GitView {
  selection: GitSelection | null;
  changesFolds: string[];
  graphRefs: GraphRefScope;
}
export declare function gitViewOf(session: Pick<GitSession, "selection" | "changesFolds" | "graphRefs">): GitView;
export declare function sameGitView(a: GitSession, b: GitSession): boolean;
export declare function parseGitView(raw: unknown): GitView | null;
export declare function withGitView<S extends GitSession>(session: S, kept: unknown): S;

export declare function applied(session: GitSession, files: readonly GitFileRow[]): GitSession;
export declare function failureOf(kind: FailureKind, error: string, paths?: readonly string[]): GitFailure;
export declare function remember<T>(byScope: Readonly<Record<string, T>>, touched: string, value: T, max?: number): Record<string, T>;
export declare function fileDraftKey(scope: string, path: string, facet: string): string;
export declare function draftStorageKey(scope: string): string;
export declare function fingerprint(text: string): string;
