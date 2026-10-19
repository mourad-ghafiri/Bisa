/** Types for `inspectorModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { NewWorkflowBody, Problem, Step, StepRecord, Workflow, WorkflowRun } from "../../types";
import type { StepAction } from "./runView.mjs";
import type { FlowFacts, FlowSelection, Selection } from "./selectionModel.mjs";

/** What the hosts know of why nothing may be edited; the first that holds is the reason. */
export interface ReadOnlyWhy {
  closed?: boolean;
  runLive?: boolean;
  runningIn?: number;
  runFinished?: boolean;
  proposed?: boolean;
  ownDesign?: boolean;
  library?: boolean;
}

export interface ReadOnlyDoor {
  act: "edit" | "designer";
  label: string;
}

export interface ReadOnly {
  reason: string;
  door: ReadOnlyDoor | null;
}

export type InspectorSection = "run" | "identity" | "kind" | "flow" | "problems" | "danger" | "runs" | "inputs" | "many";

export interface InspectorProblems {
  list: readonly Problem[];
  count: number;
  canRun: boolean;
  unreadable?: string | null;
}

/** A step's record in the run shown, and whose move it is. */
export interface StepRunFacts {
  record: StepRecord;
  yours: boolean;
  actions: StepAction[];
  /** The progress row the goal's list draws for it; `null` for a start or a parallel. */
  row: { state: { state: string }; holder: string | null; startedAt: number | null; duration: string | null; error: string | null; output: unknown; workItem: string | null; answer: unknown } | null;
}

interface ViewBase {
  title: string;
  subtitle: string;
  /** Whether the fields are disabled. */
  frozen: boolean;
  readOnly: ReadOnly | null;
  sections: InspectorSection[];
  problems: InspectorProblems;
  remove: boolean;
}

export interface WorkflowView extends ViewBase {
  kind: "workflow";
}

export interface StepView extends ViewBase {
  kind: "step";
  step: Step;
  stepStarted: boolean;
  run: StepRunFacts | null;
}

export interface FlowView extends ViewBase {
  kind: "flow";
  flow: FlowFacts;
}

export interface ManyView extends ViewBase {
  kind: "many";
  many: { steps: Step[]; flows: FlowSelection[]; n: number };
}

export type InspectorView = WorkflowView | StepView | FlowView | ManyView;

export declare function readOnlyReason(readOnly: boolean, why: ReadOnlyWhy | null | undefined): ReadOnly | null;
export declare function inspectorView(facts: {
  selection: Selection | undefined;
  workflow: NewWorkflowBody | Workflow;
  problems?: readonly Problem[];
  unreadable?: string | null;
  edit?: { readOnly?: boolean; editable?: Set<string> | null } | null;
  why?: ReadOnlyWhy | null;
  run?: WorkflowRun | null;
}): InspectorView;
