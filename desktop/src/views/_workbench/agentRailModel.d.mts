import type { AgentDef, Effort, SessionCost, SessionState } from "../../types";

export interface RailChild {
  id: string;
  name: string;
  description: string;
  state: SessionState;
  since: number;
}
export interface RailRow {
  id: string;
  label: string;
  kind: string;
  state: SessionState;
  since: number;
  goal: string | null;
  workItem: string | null;
  workstream: string | null;
  harness: string | null;
  model: string | null;
  /** The effort the turn runs at, fitted to its model; null when the row carries none. */
  effort: Effort | null;
  cost: SessionCost | null;
  gateId: string | null;
  abortable: boolean;
  transcript: boolean;
  terminalKey: string | null;
  children: RailChild[];
}
export interface PaneSummary {
  live: number;
  waiting: number;
  working: number;
  state: string;
  words: string;
  activity: string | null;
  stoppable: RailRow | null;
}
export declare function addressee(agents: readonly AgentDef[], defaultId: string | null | undefined, generalId: string, originKind?: string): AgentDef | null;
