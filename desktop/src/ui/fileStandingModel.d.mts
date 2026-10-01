/**
 * Types for `fileStandingModel.mjs`, plain JavaScript so `node --test` reads
 * it without a build step.
 */

export type StandingKind = "conflict" | "deleted" | "renamed" | "modified" | "added" | "untracked";
export type StandingTone = "ok" | "warn" | "danger";

/** One file's standing as the views derive it from git's letters. */
export interface FileStanding {
  path: string;
  kind: StandingKind;
  /** The kind is the index's — the change is staged. */
  staged?: boolean;
}

/** What a row wears: a file's own kind, or `holds` for a folder with the strongest kind beneath. */
export interface Standing {
  kind: StandingKind | "holds";
  staged: boolean;
  strongest: StandingKind;
}

export declare const STANDING_KINDS: readonly StandingKind[];
export declare function foldStandings(files: readonly FileStanding[] | null | undefined): Map<string, Standing>;
export declare function standingTone(standing: Standing | null | undefined): StandingTone | null;
export declare function standingMark(standing: Standing | null | undefined): string | null;
export declare function standingHint(standing: Standing | null | undefined): string | null;
