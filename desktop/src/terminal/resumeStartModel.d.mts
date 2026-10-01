import type { ResolvedSetting } from "../types";

export declare const RESUME_START_KEY: string;
export declare const RESUME_START_DEFAULT: boolean;
export declare const RESUME_START_WORDS: string;
export declare const START_QUIET_MS: number;
export declare const START_MIN_MS: number;
export declare const START_WITHIN_MS: number;

/** Where a nudge stands: armed, what the PTY has shown, whether it is spent. */
export interface StartState {
  armedAt: number;
  firstOutputAt: number | null;
  lastOutputAt: number | null;
  spent: boolean;
}

export declare function readResumeStart(resolved: readonly ResolvedSetting[] | null | undefined): boolean;
export declare function armed(now: number): StartState;
export declare function onOutput(state: StartState, now: number): StartState;
export declare function onInput(state: StartState): StartState;
export declare function onExit(state: StartState): StartState;
export declare function due(state: StartState, now: number): boolean;
export declare function said(state: StartState): StartState;
export declare function pending(state: StartState, now: number): boolean;
export declare function nudgeText(): string;
