import type { Text } from "../../i18n/l10n.mjs";

export interface ProposalStepLike {
  id: string;
  name: string;
  kind: string;
  /** A `start` step's event word (`manual`, `schedule`, `hook`, …); absent or `null` for any other kind. */
  event?: string | null;
  /** One line a person judges the step by — a `Text` the reader renders (`Step::summary`). */
  summary?: Text | null;
  assignee?: string | null;
  join_any?: boolean;
  on_fail?: string | null;
  max_visits?: number;
}
export interface ProposalEdgeLike {
  from: string;
  to: string;
  branch?: string | null;
  kind: string;
}
export interface ProposalInputLike {
  name: string;
  label?: string;
  required?: boolean;
  default?: unknown;
}
export interface ProposalLike {
  name?: string;
  revision?: number;
  description?: string;
  steps?: readonly ProposalStepLike[];
  edges?: readonly ProposalEdgeLike[];
  inputs?: readonly ProposalInputLike[];
  /** The inputs listening asks, when the design begins on events. */
  listening_needs?: readonly string[];
  /** Only events begin it: adopting it makes the goal listen, never run. */
  event_only?: boolean;
}
export interface StepLine {
  index: number;
  id: string;
  name: string;
  kind: string;
  summary: Text | null;
  assignee: string | null;
  notes: string[];
}
export declare function proposalHeadline(p: ProposalLike | null | undefined): string;
export declare function flowSentence(p: ProposalLike | null | undefined): string;
export declare function stepLines(p: ProposalLike | null | undefined): StepLine[];
export declare function loopSentences(p: ProposalLike | null | undefined): string[];
export declare function startsOn(p: ProposalLike | null | undefined): Text[];
export declare function listens(p: ProposalLike | null | undefined): boolean;
export declare function adoptionInputs<P extends ProposalLike>(p: P | null | undefined): NonNullable<P["inputs"]>[number][];
export declare function inputsNeeded(p: ProposalLike | null | undefined): { total: number; required: string[] };
export declare function changeRequest(text: string | null | undefined): string | null;
