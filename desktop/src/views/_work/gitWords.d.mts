export type Verb = "stage" | "unstage" | "discard" | "delete" | "drop" | "abort" | "continue" | "skip" | "revert" | "switch" | "detach" | "restore" | "apply" | "pop" | "merge" | "rebase" | "rename" | "commit" | "amend" | "push" | "forcePush" | "pull" | "fetch" | "stash";
export type Place = "changes" | "branches" | "safety" | "history" | "stashes" | "checkout" | "settings" | "inbox";
export type GitSide = "staged" | "unstaged";
export type StandingId = "staged" | "unstaged" | "untracked" | "conflicted";
export interface StandingChip {
  id: StandingId;
  /** The patch the chip opens. */
  side: GitSide;
  word: string;
  tone: "accent" | "quiet" | "warn";
  hint: string;
}

export declare const VERB: Readonly<Record<Verb, string>>;
export declare const PLACE: Readonly<Record<Place, string>>;
export declare const SAFETY_SENTENCE: string;
export declare const SAFETY_LINE: string;
export declare const CHIP_IDS: readonly string[];
export declare function confirmLabel(verb: Verb, of?: { count?: number; noun?: string }): string;
export declare function bulkLabel(verb: "stage" | "unstage", count?: number): string;
export declare function kindWord(row: { index?: string | null; worktree?: string | null; untracked?: boolean; conflicted?: boolean } | null | undefined, side: GitSide): string;
export declare const STANDING_TONE: Readonly<Record<StandingId, "accent" | "quiet" | "warn">>;
export declare function standingChips(row: { index?: string | null; worktree?: string | null; staged?: boolean; unstaged?: boolean; untracked?: boolean; conflicted?: boolean } | null | undefined): StandingChip[];
export declare function standingHint(id: StandingId, word: string): string;
export declare function fileSummary(groups: { staged: readonly { path: string }[]; unstaged: readonly { path: string }[]; untracked: readonly { path: string }[]; conflicted: readonly { path: string }[] } | null | undefined): string | null;
export declare function sideWords(staged: boolean): string;
