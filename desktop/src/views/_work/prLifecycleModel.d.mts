import type { CheckRun, CodeHostCapabilities, PullRequest } from "../../types";
import type { ReviewFacts } from "./reviewStepModel.mjs";

export type StepId = "commit" | "push" | "open_pr" | "checks" | "review" | "merge" | "cleanup";
/** `current` is the one step in hand — the act's; `waiting` is in flight and nobody's act; `blocked` says why the act is off. */
export type StepStatus = "done" | "current" | "waiting" | "todo" | "blocked";
export interface LifecycleStep {
  id: StepId;
  label: string;
  status: StepStatus;
  note: string | null;
  /** Checks still running, comments still open, a standing request for changes — which the merge names and its confirmation lists; never a block. */
  cautions: string[];
}
export interface LifecycleCta {
  /** The verb pushes the branch first, then opens the request. */
  pushes?: boolean;
  /** What the act does. */
  id: StepId;
  /** The step the act is drawn under — always the step in hand. */
  step: StepId;
  label: string;
  /** A step done elsewhere — the button navigates rather than acts. */
  secondary?: boolean;
  /** The one act is not legal yet, and this is why. */
  blocked?: string;
  /** The act is legal, and these are worth a second look first. */
  cautions?: string[];
}

export declare const STEPS: readonly StepId[];
export declare function prTitleFrom(subject: string | null | undefined, branch: string | null | undefined): string;
export declare function readWords(at: number | null | undefined, following: boolean, agent: string | null | undefined, now?: number): string | null;
export declare function lifecycle(facts: {
  isGit: boolean;
  state: string;
  aheadOfBase: number | null | undefined;
  upstream: string | null | undefined;
  ahead: number | null | undefined;
  base: string | null | undefined;
  pr: PullRequest | null | undefined;
  checks: readonly CheckRun[] | null | undefined;
  review: ReviewFacts;
  caps: CodeHostCapabilities | null | undefined;
  /** The host's word — *pull request*, or GitLab's *merge request*. */
  noun?: string;
}): { steps: LifecycleStep[]; current: StepId | null; cta: LifecycleCta | null; note: string | null };
