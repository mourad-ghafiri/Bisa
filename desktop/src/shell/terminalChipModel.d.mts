import type { TerminalSessionState } from "./terminalsModel.mjs";

/** A terminal chip's name: the tab's words, numbered along the strip when two tabs read alike. */
export declare function terminalChipName(session: TerminalSessionState, sessions: readonly TerminalSessionState[]): string;
