import type { Workstream, WorkstreamStatus } from "../../types";

export interface CardChip {
  id: string;
  text: string;
  tone: "quiet" | "neutral" | "accent" | "warn" | "ok";
  title: string;
  href?: string;
}

/** The title a workstream goes by — the one rule the panel, the rail, the Board and the footer read. */
export declare function cardTitle(w: Pick<Workstream, "id" | "name" | "kind"> | null | undefined, s: Pick<WorkstreamStatus, "branch"> | null | undefined): string;
/** What a rename writes: the name trimmed, `null` for an empty one. */
export declare function renameBody(value: string | null | undefined): { name: string | null };
/** Whether what was typed changes the name at all. */
export declare function renames(name: string | null | undefined, value: string | null | undefined): boolean;
/** The toast once a rename landed. */
export declare function renamedWords(body: { name: string | null }): string;
/** A state's word, as a person reads it. */
export declare function stateWord(state: string | null | undefined): string;
export declare function cardChips(w: Workstream | null | undefined, s: WorkstreamStatus | null | undefined): CardChip[];
export declare function stateLabel(state: { state: string; number?: number } | null | undefined): string;
