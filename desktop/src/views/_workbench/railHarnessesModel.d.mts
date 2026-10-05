import type { SessionRow, SessionState } from "../../types";
import type { TerminalSessionState } from "../../shell/terminalsModel.mjs";

/** A harness open in a workstream: who, what it is doing, and the session or tab a click would land in. */
export interface StandingHarness {
  harness: string;
  /** The session's live state; null for a shell whose harness does not report. */
  state: SessionState | null;
  sessionId: string | null;
  terminalKey: string | null;
}

/** A standing harness with its place, as a heading carries it. */
export interface HeadingHarness extends StandingHarness {
  projectId: string;
  project: string;
  workstream: string;
}

export interface HeadingHarnessWords {
  title: string;
  lines: { harness: string; text: string }[];
  more: number;
}

export declare const MAX_HEADING_MARKS: number;
export declare const MAX_HEADING_LINES: number;
export declare function standingHarnesses(sessions: readonly SessionRow[], terminals: readonly TerminalSessionState[], workstream: string): StandingHarness[];
export declare function headingMarks(list: readonly { harness: string }[]): string[];
export declare function headingHarnessWords(list: readonly HeadingHarness[], labels: Readonly<Record<string, string>>): HeadingHarnessWords;
