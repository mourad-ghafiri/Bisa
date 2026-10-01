import type { GitConflictKind, GitFileRow, GitInProgress, GitOperationFacts } from "../../types";
import type { Sides } from "./conflictSidesModel.mjs";

/** What this app run started that stopped on conflicts — the 409 named the paths. */
export interface GitOperation {
  kind: GitInProgress;
  /** What came in — the branch merged, the branch rebased onto, the commits picked or reverted. */
  from: string;
  /** The branch the operation runs on. */
  to: string;
  commits: string[];
  /** The paths the operation stopped on, as the 409 named them. */
  paths: string[];
}

export interface ChecklistRow {
  path: string;
  kind: GitConflictKind | null;
  /** The kind's short word, or *settled*. */
  short: string;
  settled: boolean;
}
export interface OperationWords {
  title: string;
  explain: string;
  step: string | null;
  line: string;
  sides: Sides;
  /** The paths still conflicted. */
  files: string[];
  checklist: ChecklistRow[];
  settled: number;
  total: number;
}
export interface OperationVerb {
  label: string;
  disabled: boolean;
  reason: string | null;
}
export interface OperationVerbs {
  continue: OperationVerb;
  skip: OperationVerb | null;
  abort: OperationVerb;
}
export interface OperationConsent {
  title: string;
  body: string;
  confirm: string;
  danger: boolean;
  kind: "tree";
}

export declare function operationWord(inProgress: GitInProgress | null | undefined): string;
export declare function conflictedPaths(files: readonly GitFileRow[] | null | undefined): string[];
export declare function operationWords(inProgress: GitInProgress, facts: GitOperationFacts | null | undefined, operation: GitOperation | null | undefined, files: readonly GitFileRow[] | null | undefined): OperationWords;
export declare function operationVerbs(inProgress: GitInProgress, conflicted: readonly string[], busy: boolean): OperationVerbs;
export declare function continueConsent(inProgress: GitInProgress, facts: GitOperationFacts | null | undefined): OperationConsent;
export declare function skipConsent(inProgress: GitInProgress): OperationConsent;
export declare function abortConsent(inProgress: GitInProgress): OperationConsent;
export declare function continuedWords(inProgress: GitInProgress, stillIn: boolean): string;
export declare function skippedWords(inProgress: GitInProgress, stillIn: boolean): string;
export declare function nextConflict(conflicted: readonly string[], settled: string | null): string | null;
