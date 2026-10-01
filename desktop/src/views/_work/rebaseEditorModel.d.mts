import type { GitRebaseAction, GitRebasePlan } from "../../types";
import type { ActConsent } from "./mergeModel.mjs";

export interface EditorRow {
  id: string;
  short: string;
  subject: string;
  author: string;
  action: GitRebaseAction;
  message: string;
}
/** The plan as the route takes it — `GitRebasePlan`, the steps oldest first. */
export type RebasePlanBody = GitRebasePlan;

export declare const REBASE_ACTIONS: readonly GitRebaseAction[];
export declare const REBASE_ACTION_WORDS: Readonly<Record<GitRebaseAction, { label: string; meaning: string }>>;
export declare function editorRows(commits: readonly { id: string; short: string; subject: string; author?: string }[]): EditorRow[];
export declare function moveStep(rows: readonly EditorRow[], i: number, dir: "up" | "down"): EditorRow[];
export declare function setAction(rows: readonly EditorRow[], i: number, action: GitRebaseAction): EditorRow[];
export declare function setMessage(rows: readonly EditorRow[], i: number, message: string): EditorRow[];
export declare function planProblem(rows: readonly EditorRow[]): string | null;
export declare function planSummary(rows: readonly EditorRow[]): string;
export declare function planIsIdentity(rows: readonly EditorRow[], commits: readonly { id: string }[]): boolean;
export declare function planOf(rows: readonly EditorRow[], upstream: string, onto?: string | null): RebasePlanBody;
export declare function planConsent(current: string, onto: string, summary: string): ActConsent;
