/**
 * Types for `designerSession.mjs`. This file is the only reason TypeScript
 * never has to read it.
 */

import type { NewWorkflowBody, Problem, PutWorkflowBody, Workflow } from "../../types";
import type { History } from "./history.mjs";

export type SessionStatus = "idle" | "saving" | "failed" | "rejected" | "conflict";

export interface Session {
  /** The stored workflow as last confirmed by the node. */
  base: Workflow;
  history: History<NewWorkflowBody>;
  /** The body last saved, by reference; the head equal to it means clean. */
  savedBody: NewWorkflowBody | null;
  /** The body a save in flight carries. */
  inflight: NewWorkflowBody | null;
  status: SessionStatus;
  failure: string | null;
  /** Failed saves in a row. */
  failures: number;
  /** The body the node could not read, by reference; the head equal to it is not sent again. */
  rejected: NewWorkflowBody | null;
  conflict: { theirs: Workflow } | null;
  problems: Problem[];
  /** When the head last moved, in ms since the epoch; `null` before any edit. */
  editedAt: number | null;
  /** When the first edit since the last save was made; `null` when none is waiting. */
  dirtySince: number | null;
}

/** What a save came back as — `settle` turns it into the session. */
export type SaveOutcome =
  | { kind: "stored"; workflow: Workflow; problems: Problem[] }
  | { kind: "conflict"; theirs: Workflow }
  | { kind: "rejected"; message: string }
  | { kind: "failed"; message: string; problems: Problem[] };

export type SaveRequest = { kind: "update"; id: string; revision: number; body: NewWorkflowBody };

export declare const RETRY_CAP_MS: number;
export declare const MAX_QUIET_MULTIPLE: number;

export declare function bodyOf(workflow: Workflow): NewWorkflowBody;
/** A definition as the wire takes it: the six keys `NewWorkflowBody` declares, by name. */
export declare function definitionBody(body: NewWorkflowBody): NewWorkflowBody;
/** What `PUT /workflows/{id}` takes: the definition by name, and the revision it was edited from. */
export declare function putBody(body: NewWorkflowBody, revision: number): PutWorkflowBody;
export declare function open(workflow: Workflow, problems?: Problem[]): Session;
export declare function edit(s: Session, body: NewWorkflowBody, at?: number): Session;
export declare function undoEdit(s: Session, at?: number): Session;
export declare function redoEdit(s: Session, at?: number): Session;
export declare function canUndoEdit(s: Session): boolean;
export declare function canRedoEdit(s: Session): boolean;
export declare function present(s: Session): NewWorkflowBody;
export declare function withProblems(s: Session, problems: Problem[]): Session;
export declare function dirty(s: Session): boolean;
export declare function saveRequest(s: Session): SaveRequest | null;
export declare function saveDue(s: Session, now: number, delay: number): number | null;
export declare function saveStarted(s: Session, body: NewWorkflowBody): Session;
export declare function saveSucceeded(s: Session, saved: Workflow, problems?: Problem[]): Session;
export declare function saveFailed(s: Session, message: string): Session;
export declare function saveRejected(s: Session, message: string): Session;
export declare function settle(s: Session, outcome: SaveOutcome): Session;
/** What a save the node did not store came back as: a conflict only when the stored copy moved past the revision sent. */
export declare function refusalOutcome(refusal: { status: number | null; message: string; body?: unknown }, sent: number, theirs: Workflow | null | undefined): Exclude<SaveOutcome, { kind: "stored" }>;
/** Whether a refused save is worth reading the stored copy for. */
export declare function asksStored(status: number | null): boolean;
export declare function retryDelayMs(s: Session): number;
export declare function conflict(s: Session, theirs: Workflow): Session;
export declare function keepMine(s: Session): Session;
export declare function takeTheirs(s: Session): Session;
export declare function remoteDecision(s: Session, revision: number): "ignore" | "reload";
export declare function remoteLoaded(s: Session, theirs: Workflow, problems?: Problem[]): Session;
/** A session opened on the answer kept from the last visit, once the node's own has landed. */
export declare function caughtUp(s: Session, stored: Workflow, problems?: Problem[]): Session;
/** Whether the row the designer read is behind what the session stands on: read it again. */
export declare function rowBehind(row: { workflow: Pick<Workflow, "id" | "revision"> } | null | undefined, s: Pick<Session, "base"> | null | undefined): boolean;
export declare function problemsFromErrorBody(body: unknown): Problem[];
export declare function statusLine(s: Session): string;

export declare const WORKFLOW_FACTS: readonly string[];
/** What of an engine fact the two readers below look at. */
export interface WorkflowFact {
  type: string;
  workflow?: string;
  revision?: number;
}
export declare function remoteAction(s: Session | null | undefined, payload: WorkflowFact | null | undefined): "leave" | "remark" | "reload" | "ignore";
export declare function goalTabRemote(
  tab: { workflow: string | null | undefined; drawing: boolean; running: boolean } | null | undefined,
  payload: WorkflowFact | null | undefined,
): "reload" | "warn" | "ignore";

