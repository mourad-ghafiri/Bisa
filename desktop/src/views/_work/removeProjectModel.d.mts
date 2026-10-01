export type RemoveAct = "archive" | "forget" | "delete";
export const REMOVE_ACTS: readonly RemoveAct[];

export interface RemoveChoice {
  id: RemoveAct;
  label: string;
  description: string;
  tone: "default" | "danger";
  disabled: string | null;
}

export function checkoutWords(n: number): string;
export function removeChoices(facts: { archived: boolean; adopted: boolean; trash: boolean; checkouts?: number }): RemoveChoice[];
export function asksAgain(act: RemoveAct, trash: boolean): boolean;
export function forGoodWords(facts: { path?: string | null; checkouts?: number }): { title: string; body: string; confirmLabel: string };
export function removedSaid(act: RemoveAct, trash: boolean, answer?: { removed_tree?: boolean; kept?: string | null; path?: string } | null): { tone: "ok" | "warn"; text: string };
export function isAdopted(project: { root?: { type?: string } | null } | null | undefined): boolean;
/** The roots a project stands on: its primary and every other workstream of it. */
export function projectRoots(workstreams: readonly { workstream: { id: string; project: string } }[] | null | undefined, pid: string): string[];
/** Whether an act takes the project's roots away — a forgotten or a deleted project, never an archived one. */
export function rootsGo(act: RemoveAct): boolean;
/** Whether the person stands on what was removed. */
export function standsOn(current: { scope: string; id: string } | null | undefined, roots: readonly string[]): boolean;
