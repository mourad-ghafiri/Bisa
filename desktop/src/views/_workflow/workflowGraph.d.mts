/**
 * Types for `workflowGraph.mjs`. This file is the only reason TypeScript never
 * has to read it.
 */

import type { NewWorkflowBody, Step, Workflow, WorkflowRun } from "../../types";
import type { StepKindName } from "./stepKinds.mjs";

/** Any definition — a draft being edited or a stored workflow. */
export type Definition = NewWorkflowBody | Workflow;

export interface GraphNode {
  id: string;
  step: Step;
}

export interface GraphEdge {
  id: string;
  from: string;
  to: string;
  branch: string | null;
  /** `boundary`: a diverting boundary event's path, leaving from its chip — taken only when it fired. */
  kind: "then" | "on_fail" | "boundary";
  /** A flow back to an earlier step — routed around the side, kept out of the ranking. */
  loop: boolean;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export type ConnectResult<D extends Definition> =
  | { ok: true; wf: D }
  | { ok: false; reason: string };

/** What may change on a canvas: nothing when read-only; the named steps in an amendment. */
export interface EditGate {
  readOnly?: boolean;
  editable?: Set<string> | null;
}

export type StepResult = { ok: true; step: Step } | { ok: false; reason: string };

export declare function toGraph(wf: Definition): Graph;
export declare function mayEdit(gate: EditGate | null | undefined, id?: string | null): boolean;
export declare function relabelBranch(step: Step, from: string, to: string): StepResult;
export declare function incoming(wf: Definition, id: string): { from: string; branch: string | null }[];
/** The steps a flow leads from into `id`, however many flows away, in the definition's order. */
export declare function upstreamOf(wf: Definition, id: string): Definition["steps"];
export declare function branchesOf(step: Step | null | undefined): string[];
export declare function isBranching(step: Step | null | undefined): boolean;
export declare function uniqueId(wf: Definition, kind: string): string;
export declare function addStep<D extends Definition>(wf: D, kind: StepKindName, id?: string, position?: { x: number; y: number } | null): { wf: D; id: string };
export declare function setPosition<D extends Definition>(wf: D, id: string, position: { x: number; y: number }): D;
export declare function setPositions<D extends Definition>(wf: D, positions: Map<string, { x: number; y: number }>): D;
export declare const DUPLICATE_OFFSET: number;
export declare function removeStep<D extends Definition>(wf: D, id: string): D;
export declare function connect<D extends Definition>(
  wf: D,
  from: string,
  to: string,
  branch?: string | null,
): ConnectResult<D>;
export declare function disconnect<D extends Definition>(wf: D, from: string, to: string, branch?: string | null): D;
/** A step a flow may go to, and whether a plain flow already does. */
export interface ThenTarget {
  id: string;
  label: string;
  on: boolean;
}
/** Where a step's flows may go: plain targets, or a branching kind's branches with where each goes. */
export interface ThenChoices {
  branching: boolean;
  targets: ThenTarget[];
  branches: { branch: string; to: string | null }[];
}
export declare function thenChoices(wf: Definition, id: string): ThenChoices;
/** One plain flow on or off, through `connect`'s refusals. */
export declare function setThen<D extends Definition>(wf: D, from: string, to: string, on: boolean): ConnectResult<D>;
/** A branch pointed at a step, or at nothing (`null`). */
export declare function setBranchTarget<D extends Definition>(wf: D, from: string, branch: string, to: string | null): ConnectResult<D>;
export declare function renameStep<D extends Definition>(wf: D, from: string, to: string): D;
export declare function renameInput<D extends Definition>(wf: D, from: string, to: string): D;
/** The steps a failure may be routed to: every other step something may flow into — never a start. */
export declare function failTargets(steps: readonly Step[] | null | undefined, id: string): Step[];
/** The `on_fail` a choice in the form writes; `null` when the choice is a route and there is nowhere to route. */
export declare function failChoice(steps: readonly Step[] | null | undefined, step: Pick<Step, "id" | "on_fail">, word: string): NonNullable<Step["on_fail"]> | null;
/** A branch name the step has for nothing else: what *Add a rule*, *Add a case* and *Add an option* name theirs. */
export declare function freshBranch(step: Step | null | undefined): string;
export declare function setOnFail<D extends Definition>(wf: D, id: string, onFail: Step["on_fail"]): D;
export declare function duplicateStep<D extends Definition>(wf: D, id: string): { wf: D; id: string | null };
export declare function replaceStep<D extends Definition>(wf: D, id: string, step: Step): D;
export declare function backEdges(order: readonly string[], edges: readonly { id: string; from: string; to: string }[]): Set<string>;
