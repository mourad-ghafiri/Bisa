export interface StripStepLike {
  id: string;
  name?: string;
  kind: string;
  state: { state: string };
}
export interface ProjectRowLike {
  goals?: readonly string[];
}
export interface InboxRowLike {
  key: string;
  needs_action?: readonly unknown[];
}
export interface AssigneeLike {
  agent?: string;
  team?: string;
  human?: string;
}
export declare const STATUS_TONE: Readonly<Record<string, string>>;
export declare function statusTone(status: string | null | undefined): string;
/** A goal's status in words; a word this build does not know is the node's own. */
export declare function statusWord(status: string | null | undefined): string;
export declare function secondLine(row: { title?: string | null; statement?: string | null } | null | undefined): string | null;
export declare function compactChips<T extends StripStepLike>(steps: readonly T[] | null | undefined, current: readonly string[] | null | undefined, max?: number): { shown: T[]; hidden: number };
export declare function projectsOf<P extends ProjectRowLike>(goalId: string, projects: readonly P[] | null | undefined): P[];
export declare function needsYouCount(goalId: string, inbox: readonly InboxRowLike[] | null | undefined): number;
export declare function assigneeSummary(assignees: readonly AssigneeLike[] | null | undefined, max?: number): { shown: { kind: string; word: string }[]; more: number };
export declare function workflowWord(strip: { workflow_name?: string | null } | null | undefined): string | null;
export interface RowVerbsLike {
  status?: string;
  closed?: unknown;
  run_status?: string | null;
  queued?: number;
  holder?: string;
  run?: string | null;
  workflow?: string | null;
  listening?: unknown;
}
export declare function rowVerbs(row: RowVerbsLike | null | undefined, liveSessions?: number): import("../_goal/runControl.mjs").RunVerbs;
/** What a goal card's `⋮` can do: open the goal, the run's verbs, delete it. */
export type CardMenuId = "open" | "start" | "restart" | "stop" | "delete";
export interface CardMenuItem {
  id: CardMenuId;
  label: string;
  danger?: boolean;
  separatorBefore?: boolean;
}
export declare function cardMenu(verbs: import("../_goal/runControl.mjs").RunVerbs | null | undefined): CardMenuItem[];
export declare function listeningChip(row: { listening?: { paused?: unknown } | null } | null | undefined): { words: string; tone: "accent" | "warn"; paused: boolean } | null;
export declare function queuedChip(row: { queued?: number } | null | undefined): string | null;
export declare function designingRow(row: { holder?: string } | null | undefined, designsInMode: boolean): boolean;
