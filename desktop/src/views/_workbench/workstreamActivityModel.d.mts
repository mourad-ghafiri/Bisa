import type { SessionRow } from "../../types";
import type { TerminalSessionState } from "../../shell/terminalsModel.mjs";
import type { SessionWord } from "../../ui/sessionState.mjs";

export interface ActivityCounts {
  needsYou: number;
  working: number;
  done: number;
  live: number;
  agents: number;
}
export interface Activity {
  state: SessionWord;
  counts: ActivityCounts;
}
export declare function terminalState(s: TerminalSessionState): SessionWord;
export declare function fold(words: readonly SessionWord[]): SessionWord;
export declare function workstreamActivity(
  sessions: readonly SessionRow[],
  terminals: readonly TerminalSessionState[],
  workstream: string,
): Activity;
export declare function projectActivity(byWorkstream: readonly Activity[]): Activity;
