export interface WorkstreamMenuItem {
  id: string;
  label: string;
  /** The harness this item starts, for a `harness:<id>` or a `fresh:<id>` item. */
  harness?: string;
  /** A new session, beside an item that resumes. */
  fresh?: boolean;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
}
export interface AgentRowMenuItem {
  id: "show" | "answer" | "abort" | "terminate";
  label: string;
  danger?: boolean;
  separatorBefore?: boolean;
}
export declare function agentRowMenuSpec(row: {
  parent: string | null;
  terminalKey?: string | null;
  gateId?: string | null;
  state: import("../../ui/sessionState.mjs").StateLike;
}): AgentRowMenuItem[];
export declare function workstreamMenuSpec(ctx: {
  primary: boolean;
  exists: boolean;
  /** `resumes`: opening it continues the session it left in this workstream. */
  harnesses: readonly { id: string; label: string; resumes?: boolean }[];
  rename: boolean;
  close: boolean;
}): WorkstreamMenuItem[];
