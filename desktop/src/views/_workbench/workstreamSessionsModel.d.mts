import type { SessionRow } from "../../types";
import type { TerminalSessionState, Liveness } from "../../shell/terminalsModel.mjs";
import type { Effort, SessionState } from "../../types";

export interface WorkstreamSessionRow {
  kind: "terminal" | "agent";
  id: string;
  workstream: string;
  label: string;
  harness: string | null;
  /** The model the session runs on, as the harness names it; null until the harness says. */
  model: string | null;
  /** The effort the session runs at, fitted to its model; null when the row carries none. */
  effort: Effort | null;
  agent?: string | null;
  /** The live `SessionState`, on an agent row. */
  state?: SessionState;
  activity?: string;
  since?: number;
  started?: number | null;
  workItem?: string | null;
  goal?: string | null;
  gateId?: string | null;
  /** The PTY liveness, on a terminal row. */
  liveness?: Liveness;
  openedAt?: number | null;
  exitedAt?: number | null;
  /** How many sub-agents nest under this session (agent rows only). */
  childCount?: number;
  /** The terminal tab a reported session lives in (agent rows only); null for an engine session. */
  terminalKey?: string | null;
  /** The session id a sub-agent nests under; null otherwise. */
  parent: string | null;
}

export declare function claimedSessions(sessions: readonly SessionRow[], terminals: readonly TerminalSessionState[]): Map<string, string>;
export declare const WORK_KINDS: readonly string[];
export declare function isDrawn(session: SessionRow, claimed: Map<string, string>): boolean;
export declare function workstreamSessionRows(
  sessions: readonly SessionRow[],
  terminals: readonly TerminalSessionState[],
  workstream: string,
): WorkstreamSessionRow[];

/**
 * The rows a surface paints once folded harnesses' sub-agents are hidden
 *: drops rows whose `parent` session is collapsed. Terminals and
 * top-level sessions are always kept.
 */
export declare function visibleSessionRows(
  rows: readonly WorkstreamSessionRow[],
  isCollapsed: ((sessionId: string) => boolean) | { has(id: string): boolean },
): WorkstreamSessionRow[];
