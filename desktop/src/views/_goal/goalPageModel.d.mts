/** Types for `goalPageModel.mjs`. */

import type { GateEntry, Goal, Listening, Step, WorkflowRun } from "../../types";

/** What of a run says where it stands. */
type RunEnd = Pick<WorkflowRun, "outcome" | "cancelled">;
/** A design, as far as its ways in go. */
type Design = { origin?: { origin?: string }; steps?: readonly Step[] };

export interface ListeningFacts {
  listening: Listening | null;
  /** The design begins on events: the goal's start arms them. */
  listens: boolean;
  /** The design's start by hand — where *Run now* begins; `null` when only events begin it. */
  manualEntry: string | null;
}

export declare const ADOPT_SUBJECT: string;
/** What the header names the goal by: its title over its statement, else the statement alone — never its id. */
export declare function headerWords(goal: { title?: string | null; statement?: string | null } | null | undefined): { title: string; subtitle: string | null };
export declare function adoptGateOf(pendingGates: readonly GateEntry[] | null | undefined): GateEntry | undefined;
export declare function runStanding(run: RunEnd | null | undefined): { finished: boolean; unfinished: boolean };
export declare function isProposed(goal: Goal | null | undefined, pendingGates: readonly GateEntry[] | null | undefined): boolean;
export declare function ownDesignOf<T extends { origin?: { origin?: string } }>(startable: T | null | undefined): T | null;
/** What the close dialog holds: the person's word on why, and the goal that takes this one's place. */
export interface CloseForm {
  rationale?: string | null;
  replacedBy?: string | null;
}
type GoalLike = { id: string; title?: string | null; statement?: string; status?: string; archived?: unknown };
export declare function replacements(goals: readonly GoalLike[] | null | undefined, goal: string): { id: string; label: string }[];
/** The body of `POST /goals/{id}/close`: a rationale, or the goal that replaces this one — never both. */
export declare function closeBody(form: CloseForm | null | undefined, goal: string): import("../../types").CloseGoalBody;
export declare function closeWords(form: CloseForm | null | undefined, goals: readonly GoalLike[] | null | undefined, goal: string): { records: string; closed: string };
export declare function listeningFacts(goal: Goal | null | undefined, startable: Design | null | undefined): ListeningFacts;
export declare function pageFacts<T extends Design>(data: {
  goal: Goal | null | undefined;
  run: RunEnd | null | undefined;
  pendingGates: readonly GateEntry[] | null | undefined;
  startable: T | null | undefined;
}): { closed: boolean; finished: boolean; unfinished: boolean; adoptGate: GateEntry | undefined; proposed: boolean; ownDesign: T | null } & ListeningFacts;
