/**
 * Types for `agentEditsModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */
import type { SessionRow, SessionState } from "../../types";

/** An edit asked of an agent for one document, and when. */
export interface AgentEditRecord {
  scope: string;
  id: string;
  agentId: string;
  /** Unix seconds the request was posted. */
  at: number;
}

export declare const LIVE: readonly string[];
export declare const SETTLED: readonly string[];
export declare function recordKey(scope: string, id: string, path: string): string;
/** Whether a roster move settles the record: same workstream and agent, live → settled, after the ask. */
export declare function settles(record: AgentEditRecord, prev: SessionState | string | null, row: SessionRow): boolean;
export declare function settledWords(agentName: string, path: string, state: string): string;
export declare function settledTone(state: string): "ok" | "error" | "info";
