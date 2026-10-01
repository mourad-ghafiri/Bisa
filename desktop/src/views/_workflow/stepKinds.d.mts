/**
 * Types for `stepKinds.mjs`. This file is the only reason TypeScript never has
 * to read it.
 */

import type {
  AskOption,
  BoundaryOn,
  Boundary,
  CheckKind,
  Condition,
  FireOn,
  Finish,
  Join,
  NewWorkflowBody,
  OnFail,
  ProblemKind,
  ProjectChange,
  RunEnd,
  StartOn,
  Step,
  StepKind,
  WaitFor,
} from "../../types";

export type StepKindName = StepKind["kind"];
export type Family = "event" | "gateway" | "loop" | "task";

export interface KindDef {
  kind: StepKindName;
  family: Family;
  label: string;
  explain: string;
}

export declare const FAMILIES: readonly Family[];
export declare const FAMILY_LABEL: Readonly<Record<Family, string>>;
export declare const STEP_KINDS: readonly KindDef[];
export declare function familyOf(kind: string): Family;
export declare function kindLabel(kind: string): string;
export declare function mayCarryBoundaries(step: Step | null | undefined): boolean;
export declare const CONDITIONS: readonly { condition: Condition["condition"]; label: string }[];
export declare const WAITS: readonly { until: WaitFor["until"]; label: string }[];
export declare function blankWait(until: WaitFor["until"]): WaitFor;
export declare const START_EVENTS: readonly { event: StartOn["event"]; label: string }[];
export declare const BOUNDARY_EVENTS: readonly { event: BoundaryOn["event"]; label: string }[];
export declare const BOUNDARY_ACTS: readonly { act: Boundary["act"]; label: string }[];
export declare const END_FINISHES: readonly { finish: Finish; label: string }[];
export declare const DECIDE_PICKS: readonly { pick: "first" | "every"; label: string }[];
export declare const OVERLAPS: readonly { overlap: "queue" | "skip" | "parallel"; label: string }[];
export declare const MESSAGE_FROM: readonly { from: "you" | "agents" | "someone"; label: string }[];
export declare const PROJECT_CHANGES: readonly { change: ProjectChange; label: string }[];
export declare const RUN_ENDS: readonly { outcome: RunEnd; label: string }[];
export declare const FIRE_ON: readonly { fire_on: FireOn; label: string }[];
export declare const CHECKS: readonly { check: CheckKind["check"]; label: string }[];
export declare const JOINS: readonly { join: Join; label: string }[];
export declare const ON_FAILS: readonly { on_fail: OnFail["on_fail"]; label: string }[];
export declare const TEMPLATE_HINT: string;
export declare const PROBLEM_KIND_LABEL: Record<ProblemKind, string>;
export declare const STEP_MIME: string;
export declare const BRANCH_HANDLE_PREFIX: string;
export declare function branchHandle(branch: string): string;
export declare function branchOfHandle(handle: string | null | undefined): string | null;
export declare function freshOptionId(options: readonly Pick<AskOption, "id">[]): string;
export declare const DEFAULT_MAX_VISITS: number;
export declare const DEFAULT_MAX_ITERATIONS: number;
export declare const COMBINATORS: readonly Condition["condition"][];
export declare const MAX_CONDITION_DEPTH: number;
export declare const FIXED_BRANCHES: Readonly<Record<"if" | "for_each" | "while", readonly string[]>>;
export declare const START_STEP_ID: string;

export declare function blankCondition(): Condition;
export declare function blankStep(kind: StepKindName, id: string): Step;
export declare function blankWorkflow(name?: string): NewWorkflowBody;
export declare function kindBranchesOf(step: Step | null | undefined): string[];
export declare function divertNamesOf(step: Step | null | undefined): string[];
export declare function branchesOf(step: Step | null | undefined): string[];
export declare function isBranching(step: Step | null | undefined): boolean;
