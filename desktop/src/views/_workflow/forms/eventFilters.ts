/**
 * The event filters' shapes as the forms hold them — every field that takes
 * one value or a list as `OneOrMany`, the three kinds the designer gained
 * (a goal's, a workflow's and an agent's move) beside the wire's. Written
 * by hand beside the generated types so the forms compile before and after
 * `just gen-types` regenerates them: once it has, each of these is the
 * wire's own shape restated, and the start and wait unions below widen
 * nothing.
 */

import type { ProjectChange, RunEnd, StartOn, WaitFor } from "../../../types";
import type { AssigneeRef } from "./assigneeRefModel.mjs";
import type { OneOrMany } from "./manyModel.mjs";

export type { OneOrMany };

/** A project named by its id, or read from an input of kind project. */
export type ProjectRef = string | { input: string };

export type ProjectChangeEx = ProjectChange | "branch" | "pull" | "workstream";
export type MessageFromWord = "you" | "agents" | "people" | "anyone";
export type MessagePlace = "channel" | "direct" | "goal" | "workstream" | "conversation";
export type RunChange = "queued" | "started" | "ended" | "step";
export type GoalChange = "created" | "updated" | "closed" | "archived" | "restored" | "deleted" | "attached" | "detached" | "proposed" | "asked" | "decided";
export type WorkflowChange = "saved" | "proposed" | "archived" | "restored" | "deleted";
export type AgentChange = "started" | "waiting" | "replied" | "done" | "failed" | "aborted";

export interface ProjectFilterEx {
  project?: OneOrMany<ProjectRef> | null;
  change?: OneOrMany<ProjectChangeEx> | null;
  branch?: OneOrMany<string> | null;
  glob?: OneOrMany<string> | null;
}

export interface MessageFilterEx {
  in?: OneOrMany<string> | null;
  from?: OneOrMany<MessageFromWord | AssigneeRef> | null;
  mentions?: OneOrMany<AssigneeRef> | null;
  contains?: string | null;
  kind?: OneOrMany<MessagePlace> | null;
}

export interface RunFilterEx {
  workflow?: OneOrMany<string> | null;
  change?: OneOrMany<RunChange> | null;
  outcome?: OneOrMany<RunEnd> | null;
  step?: OneOrMany<string> | null;
  state?: OneOrMany<string> | null;
  goal?: OneOrMany<string> | null;
}

export interface GoalFilterEx {
  goal?: OneOrMany<string> | null;
  change?: OneOrMany<GoalChange> | null;
}

export interface WorkflowFilterEx {
  workflow?: OneOrMany<string> | null;
  change?: OneOrMany<WorkflowChange> | null;
}

export interface AgentFilterEx {
  agent?: OneOrMany<AssigneeRef> | null;
  project?: OneOrMany<ProjectRef> | null;
  harness?: OneOrMany<string> | null;
  change?: OneOrMany<AgentChange> | null;
}

export interface PlatformFilterEx {
  topic: OneOrMany<string>;
  fields?: Record<string, string>;
}

/** A start's event, the three kinds included. */
export type StartOnEx = StartOn | ({ event: "goal" } & GoalFilterEx) | ({ event: "workflow" } & WorkflowFilterEx) | ({ event: "agent" } & AgentFilterEx);

/** A wait's catch, the three kinds included. */
export type WaitForEx = WaitFor | ({ until: "goal" } & GoalFilterEx) | ({ until: "workflow" } & WorkflowFilterEx) | ({ until: "agent" } & AgentFilterEx);
