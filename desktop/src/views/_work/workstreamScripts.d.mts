export type ScriptPhaseId = "pre_create" | "post_create" | "clean";

export interface PhaseSpec {
  readonly id: ScriptPhaseId;
  readonly key: string;
  readonly label: string;
  readonly when: string;
  readonly cwd: string;
}

export interface ScriptView {
  phase: string;
  command: string;
  trusted: boolean;
}

export interface ScriptsView {
  timeout_secs: number;
  scripts: readonly ScriptView[];
}

export interface ScriptEdits {
  texts: Record<string, string>;
  timeout: number;
}

export declare const PHASES: readonly PhaseSpec[];
export declare const TIMEOUT_KEY: string;
export declare const ENV_VARS: readonly (readonly [string, string])[];
export declare const MAX_SCRIPT_CHARS: number;

export declare function scriptEdits(view: ScriptsView | null | undefined): ScriptEdits;
export declare function validateScript(text: string): string | null;
export declare function isDirty(current: ScriptEdits, edits: ScriptEdits): boolean;
export declare function trustLine(script: ScriptView | undefined): { tone: "ok" | "warn" | "quiet"; text: string } | null;
export declare function needsApproval(view: ScriptsView | null | undefined): boolean;
export declare function scriptRefusal(err: { message: string; detail?: { phase?: unknown; output?: unknown } | null }): {
  title: string;
  message: string;
  output: string;
};
