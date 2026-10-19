/** Types for `selectionModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { Step } from "../../types";

export interface StepSelection {
  kind: "step";
  id: string;
}

/** One flow by its ends; a failure route has no branch. */
export interface FlowSelection {
  kind: "flow";
  from: string;
  to: string;
  branch: string | null;
  route: "then" | "on_fail";
}

export interface ManySelection {
  kind: "many";
  steps: string[];
  flows: FlowSelection[];
}

export type Selection = StepSelection | FlowSelection | ManySelection | null;

/** What the canvas reports picked: node ids and edge ids. */
export interface Pick {
  nodes: readonly string[];
  edges: readonly string[];
}

export interface FlowFacts {
  exists: boolean;
  from: Step | null;
  to: Step | null;
  branch: string | null;
  route: "then" | "on_fail";
  loop: boolean;
  boundary: boolean;
  /** The flow's word: a plain flow, a branch's, the otherwise, a boundary's path, the failure route. */
  word: string;
  /** Whether the branch may be renamed from the panel. */
  relabel: boolean;
}

export declare function stepSelection(id: string | null | undefined): StepSelection | null;
export declare function stepOf(sel: unknown): string | null;
export declare function flowRef(from: string, to: string, branch?: string | null, route?: "then" | "on_fail"): FlowSelection;
export declare function flowRefOf(edge: { from: string; to: string; branch: string | null; kind: string }): FlowSelection;
export declare function edgeIdOf(ref: { from: string; to: string; branch: string | null; route: string }): string;
export declare function flowRefOfEdgeId(id: unknown): FlowSelection | null;
export declare function sameSelection(a: unknown, b: unknown): boolean;
export declare function selectionOf(pick: Pick | null | undefined): Selection;
export declare function selectedIds(sel: Selection | undefined): { nodes: string[]; edges: string[] };
export declare function keptSelection(sel: Selection | undefined, wf: { steps?: readonly { id: string }[] } | null | undefined): Selection;
export declare function selectionAfterDelete(sel: Selection | undefined, nodeIds: readonly string[], edgeIds: readonly string[]): Selection;
export declare function readSelection(raw: unknown): StepSelection | FlowSelection | null;
export declare function storedSelection(sel: unknown): string | null;
export declare function propertiesScrollName(sel: Selection | undefined): string;
export declare function flowRefAfterRelabel<S extends Selection>(sel: S, from: string, was: string, now: string): S | FlowSelection;
export declare function flowFacts(wf: { steps?: readonly Step[] } | null | undefined, ref: FlowSelection): FlowFacts;
