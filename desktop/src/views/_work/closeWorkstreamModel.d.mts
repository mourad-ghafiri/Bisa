import type { SessionRow } from "../../types";
import type { TerminalSessionState } from "../../shell/terminalsModel.mjs";

/** Live harness tabs, live plain shells, and live engine sessions not backed by a tab — what a close terminates. */
export interface TerminationCounts {
  harnesses: number;
  shells: number;
  agents: number;
}

export declare function terminationCounts(
  sessions: readonly Pick<SessionRow, "workstream" | "kind" | "state">[] | null | undefined,
  terminals: readonly Pick<TerminalSessionState, "scope" | "id" | "harness" | "liveness">[] | null | undefined,
  workstream: string,
): TerminationCounts;
/** *1 harness terminated, 2 shells closed and 1 agent session aborted*, or null. */
export declare function terminationWords(counts: TerminationCounts): string | null;
/** The consent sentence for a close dialog, or null when nothing stands in the workstream. */
export declare function terminationConsent(counts: TerminationCounts): string | null;
